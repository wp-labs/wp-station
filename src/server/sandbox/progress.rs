//! 沙盒执行过程中的阶段状态与收尾辅助。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::server::sandbox::analyze::{self, RuntimeMetrics, StageError};
use crate::server::sandbox::diagnostics;
use crate::utils::SystemKind;
use crate::utils::sandbox::{DaemonProcess, SandboxWorkspace};

use super::{
    Conclusion, DiagnosticHit, OutputFileStatus, RunOptions, SandboxStage, SandboxTaskHandle,
    StageStatus, TaskStatus,
};

/// 运行结束态，用于在主流程完成后统一落最终状态。
pub(super) enum RunEndState {
    Success,
    Failed {
        stage: SandboxStage,
        summary: String,
    },
    Stopped,
}

impl RunEndState {
    /// 返回日志使用的最终状态标签。
    pub(super) fn as_label(&self) -> &'static str {
        match self {
            RunEndState::Success => "success",
            RunEndState::Failed { .. } => "failed",
            RunEndState::Stopped => "stopped",
        }
    }
}

/// 沙盒运行过程中需要跨阶段共享的资源。
pub(super) struct RunResources {
    system: SystemKind,
    workspace: Option<SandboxWorkspace>,
    daemon: Option<DaemonProcess>,
    output_checks: Vec<OutputFileStatus>,
    metrics: RuntimeMetrics,
    pub(super) options: RunOptions,
    daemon_log: Option<PathBuf>,
}

impl RunResources {
    /// 根据任务参数创建空的运行资源容器。
    pub(super) fn new(system: SystemKind, options: RunOptions) -> Self {
        Self {
            system,
            workspace: None,
            daemon: None,
            output_checks: Vec::new(),
            metrics: RuntimeMetrics::default(),
            options,
            daemon_log: None,
        }
    }

    /// 返回当前任务所属系统。
    pub(super) fn system(&self) -> SystemKind {
        self.system
    }

    /// 返回当前沙盒工作区；若还未准备完成则报错。
    pub(super) fn workspace(&self) -> Result<&SandboxWorkspace, StageError> {
        self.workspace
            .as_ref()
            .ok_or_else(|| StageError::new("沙盒目录尚未准备"))
    }

    /// 返回工作区目录路径，用于诊断器读取阶段日志。
    pub(super) fn workspace_dir(&self) -> Option<PathBuf> {
        self.workspace.as_ref().map(|ws| ws.project_dir.clone())
    }

    /// 返回运行指标的可变引用。
    pub(super) fn metrics_mut(&mut self) -> &mut RuntimeMetrics {
        &mut self.metrics
    }

    /// 返回运行指标的只读视图。
    pub(super) fn metrics(&self) -> &RuntimeMetrics {
        &self.metrics
    }

    /// 记录 wparse 日志路径，供后续结果分析复用。
    pub(super) fn set_daemon_log(&mut self, path: PathBuf) {
        self.daemon_log = Some(path);
    }

    /// 注入准备好的工作区。
    pub(super) fn set_workspace(&mut self, workspace: SandboxWorkspace) {
        self.workspace = Some(workspace);
    }

    /// 保存 wparse 进程句柄。
    pub(super) fn set_daemon(&mut self, daemon: DaemonProcess) {
        self.daemon = Some(daemon);
    }

    /// 取出当前 daemon 句柄。
    pub(super) fn take_daemon(&mut self) -> Option<DaemonProcess> {
        self.daemon.take()
    }

    /// 更新输出文件检查结果。
    pub(super) fn set_output_checks(&mut self, output_checks: Vec<OutputFileStatus>) {
        self.output_checks = output_checks;
    }

    /// 返回 daemon 日志路径。
    pub(super) fn daemon_log(&self) -> Option<PathBuf> {
        self.daemon_log.clone()
    }
}

/// 停止后台进程并按策略清理沙盒目录。
pub(super) async fn cleanup_resources(resources: &mut RunResources) {
    if let Some(daemon) = resources.take_daemon() {
        let _ = daemon.terminate().await;
    }
    if let Some(workspace) = resources.workspace.take() {
        let _ = workspace.cleanup_after_run(resources.options.keep_workspace);
    }
}

/// 将指定阶段标记为运行中。
pub(super) async fn mark_stage_running(task: &Arc<SandboxTaskHandle>, stage: SandboxStage) {
    task.with_run_mut(|run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            stage_info.status = StageStatus::Running;
            stage_info.started_at = Some(Utc::now());
            stage_info.summary = None;
            stage_info.error_code = None;
        }
    })
    .await;
}

/// 将指定阶段标记为成功并写入摘要。
pub(super) async fn mark_stage_success(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    summary: String,
) {
    task.with_run_mut(|run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            let now = Utc::now();
            stage_info.status = StageStatus::Success;
            stage_info.ended_at = Some(now);
            stage_info.duration_ms = compute_duration(stage_info.started_at, now);
            stage_info.summary = Some(summary);
        }
    })
    .await;
}

/// 将指定阶段标记为失败并记录错误码。
pub(super) async fn mark_stage_failure(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    summary: &str,
    error_code: Option<String>,
) {
    task.with_run_mut(|run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            let now = Utc::now();
            stage_info.status = StageStatus::Failed;
            stage_info.ended_at = Some(now);
            stage_info.duration_ms = compute_duration(stage_info.started_at, now);
            stage_info.summary = Some(summary.to_string());
            stage_info.error_code = error_code;
        }
    })
    .await;
}

/// 将指定阶段标记为用户停止。
pub(super) async fn mark_stage_stopped(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    summary: &str,
) {
    task.with_run_mut(|run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            let now = Utc::now();
            stage_info.status = StageStatus::Stopped;
            stage_info.ended_at = Some(now);
            stage_info.duration_ms = compute_duration(stage_info.started_at, now);
            stage_info.summary = Some(summary.to_string());
        }
    })
    .await;
}

/// 为指定阶段记录日志文件路径，便于前端回看。
pub(super) async fn set_stage_log_path(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    path: &Path,
    workspace: Option<&SandboxWorkspace>,
) {
    let path_str = workspace
        .map(|ws| ws.display_relative(path))
        .unwrap_or_else(|| path.to_string_lossy().to_string());
    task.with_run_mut(|run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            stage_info.log_path = Some(path_str);
        }
    })
    .await;
}

/// 将当前失败阶段之后的剩余阶段全部标记为跳过。
pub(super) async fn skip_following_stages(task: &Arc<SandboxTaskHandle>, current: SandboxStage) {
    task.with_run_mut(|run| {
        let mut mark = false;
        for stage_order in SandboxStage::ordered() {
            if stage_order == current {
                mark = true;
                continue;
            }
            if mark
                && let Some(stage_info) = run.stage_mut(stage_order)
                && matches!(
                    stage_info.status,
                    StageStatus::Pending | StageStatus::Running
                )
            {
                stage_info.status = StageStatus::Skipped;
                stage_info.summary = None;
            }
        }
    })
    .await;
}

/// 汇总阶段诊断与输出检查，生成任务最终结论。
pub(super) async fn apply_conclusion(task: &Arc<SandboxTaskHandle>, resources: &mut RunResources) {
    if resources.output_checks.is_empty() && resources.metrics.input_count == 0 {
        return;
    }
    let base_conclusion =
        analyze::finalize_conclusion(&resources.output_checks, &resources.metrics);
    let output_checks = resources.output_checks.clone();
    task.with_run_mut(|run| {
        let mut updated = base_conclusion.clone();
        let mut all_hits: Vec<&DiagnosticHit> = Vec::new();
        for stage in &run.stages {
            for hit in &stage.diagnostics {
                all_hits.push(hit);
            }
        }
        all_hits.sort_by_key(|hit| hit.priority);
        let mut dedup = HashSet::new();
        updated.top_suggestions = all_hits
            .into_iter()
            .filter_map(|hit| {
                if dedup.insert(hit.suggestion.clone()) {
                    Some(hit.suggestion.clone())
                } else {
                    None
                }
            })
            .take(5)
            .collect();
        updated.output_file_checks = output_checks.clone();
        updated.suspected_files = updated
            .output_file_checks
            .iter()
            .filter(|item| item.affects_pass && !item.is_empty)
            .map(|item| item.relative_path.clone())
            .collect();
        run.conclusion = Some(updated);
    })
    .await;
}

/// 根据运行结束态写入任务最终状态和结论。
pub(super) async fn finalize_run(task: &Arc<SandboxTaskHandle>, state: RunEndState) {
    task.with_run_mut(|run| {
        let now = Utc::now();
        run.ended_at.get_or_insert(now);
        match state {
            RunEndState::Success => {
                if run.status != TaskStatus::Stopped && run.status != TaskStatus::Failed {
                    run.status = TaskStatus::Success;
                    run.conclusion.get_or_insert_with(Conclusion::passed);
                }
            }
            RunEndState::Failed { stage, summary } => {
                run.status = TaskStatus::Failed;
                let mut conclusion = run.conclusion.take().unwrap_or_else(Conclusion::default);
                conclusion.passed = false;
                conclusion.failed_stage = Some(stage);
                if !summary.is_empty()
                    && !conclusion
                        .top_suggestions
                        .iter()
                        .any(|item| item == &summary)
                {
                    conclusion.top_suggestions.insert(0, summary);
                }
                run.conclusion = Some(conclusion);
            }
            RunEndState::Stopped => {
                run.status = TaskStatus::Stopped;
                run.conclusion.get_or_insert_with(Conclusion::stopped);
            }
        }
    })
    .await;
}

/// 为阶段追加基于日志的诊断建议。
pub(super) async fn attach_stage_diagnostics(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    workspace_dir: Option<&Path>,
) {
    let log_path = {
        let snapshot = task.snapshot().await;
        snapshot
            .stages
            .iter()
            .find(|item| item.stage == stage)
            .and_then(|item| item.log_path.clone())
    };
    let hits = diagnostics::collect_stage_hits(stage, log_path.as_deref(), workspace_dir);
    store_stage_diagnostics(task, stage, hits).await;
}

/// 计算阶段或任务耗时，若时间异常则回退为 0。
fn compute_duration(started_at: Option<DateTime<Utc>>, ended_at: DateTime<Utc>) -> Option<u64> {
    started_at.map(|start| {
        let duration = ended_at.signed_duration_since(start);
        if duration.num_milliseconds() < 0 {
            0
        } else {
            duration.num_milliseconds() as u64
        }
    })
}

/// 将当前阶段收集到的诊断建议回写到运行快照中。
async fn store_stage_diagnostics(
    task: &Arc<SandboxTaskHandle>,
    stage: SandboxStage,
    hits: Vec<DiagnosticHit>,
) {
    task.with_run_mut(move |run| {
        if let Some(stage_info) = run.stage_mut(stage) {
            stage_info.diagnostics = hits;
        }
    })
    .await;
}
