use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::time::sleep;
use tracing::{error, info};

use crate::error::AppError;
use crate::server::sandbox_analyzer::{self, RuntimeMetrics, StageError};
use crate::server::sandbox_diagnostics;
use crate::utils::{
    constants::SANDBOX_RUNTIME_UDP_PORT,
    process_guard::{self, DaemonProcess},
    sandbox_workspace::SandboxWorkspace,
};

use super::sandbox::{
    Conclusion, DiagnosticHit, FileOverride, OutputFileStatus, RunOptions, SandboxStage,
    SandboxState, SandboxTaskHandle, StageStatus, TaskStatus,
};

/// 在独立 Tokio 任务中执行沙盒运行，结束后回调队列。
pub fn spawn_sandbox_execution(state: SandboxState, task: Arc<SandboxTaskHandle>) {
    tokio::spawn(async move {
        run_sandbox_task(state.clone(), task.clone()).await;
        state.on_task_completed(task).await;
    });
}

async fn run_sandbox_task(state: SandboxState, task: Arc<SandboxTaskHandle>) {
    let task_id = task.task_id().to_string();
    let guard = state.mutex.lock().await;
    info!("开始执行沙盒任务: {}", task_id);

    let snapshot = task.snapshot().await;
    let overrides = snapshot.overrides.clone();
    let options = snapshot.options.clone();

    task.with_run_mut(|run| {
        run.status = TaskStatus::Running;
        run.started_at = Some(Utc::now());
    })
    .await;

    let cancel_token = task.cancel_token();
    let mut end_state = RunEndState::Success;
    let mut resources = RunResources::new(options.clone());
    for stage in SandboxStage::ordered() {
        if cancel_token.is_cancelled() {
            info!(
                "沙盒任务被取消: task_id={}, stage={}",
                task.task_id(),
                stage
            );
            mark_stage_stopped(&task, stage, "任务被用户终止").await;
            end_state = RunEndState::Stopped;
            skip_following_stages(&task, stage).await;
            break;
        }

        info!("沙盒阶段开始: task_id={}, stage={}", task.task_id(), stage);
        mark_stage_running(&task, stage).await;
        let stage_result = match stage {
            SandboxStage::PrepareWorkspace => {
                stage_prepare_workspace(&task, &mut resources, &overrides).await
            }
            SandboxStage::PreflightCheck => stage_preflight_check(&task, &resources).await,
            SandboxStage::StartDaemon => stage_start_daemon(&task, &mut resources).await,
            SandboxStage::RunWpgen => stage_run_wpgen(&task, &mut resources).await,
            SandboxStage::AnalyseRuntimeOutput => {
                stage_analyse_runtime_output(&task, &mut resources).await
            }
            _ => Ok("该阶段已合并".to_string()),
        };

        let workspace_dir = resources.workspace_dir();
        match stage_result {
            Ok(summary) => {
                mark_stage_success(&task, stage, summary.clone()).await;
                attach_stage_diagnostics(&task, stage, workspace_dir.as_deref()).await;
                info!(
                    "沙盒阶段成功: task_id={}, stage={}, summary={}",
                    task.task_id(),
                    stage,
                    summary
                );
            }
            Err(err) => {
                mark_stage_failure(&task, stage, &err.summary, err.code.clone()).await;
                attach_stage_diagnostics(&task, stage, workspace_dir.as_deref()).await;
                error!(
                    "沙盒阶段失败: task_id={}, stage={}, code={:?}, summary={}",
                    task.task_id(),
                    stage,
                    err.code,
                    err.summary
                );
                end_state = RunEndState::Failed {
                    stage,
                    summary: err.summary.clone(),
                };
                skip_following_stages(&task, stage).await;
                break;
            }
        }

        if matches!(end_state, RunEndState::Stopped) {
            break;
        }
    }

    apply_conclusion(&task, &mut resources).await;
    cleanup_resources(&mut resources).await;
    let final_label = end_state.as_label();
    finalize_run(&task, end_state).await;
    info!(
        "沙盒任务结束: task_id={}, final_state={}",
        task.task_id(),
        final_label
    );
    drop(guard);
}

async fn stage_prepare_workspace(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
    overrides: &[FileOverride],
) -> Result<String, StageError> {
    let workspace = SandboxWorkspace::prepare(task.task_id(), overrides).map_err(to_stage_error)?;
    let workspace_path = workspace.display_relative(&workspace.project_dir);
    task.with_run_mut(|run| {
        run.workspace_path = Some(workspace_path);
    })
    .await;

    let mut log_lines = vec![
        format!("task_id: {}", task.task_id()),
        format!(
            "source_project_root: {}",
            workspace.display_relative(&workspace.source_project_root)
        ),
        format!(
            "sandbox_project_dir: {}",
            workspace.display_relative(&workspace.project_dir)
        ),
        "目录结构（截断预览）:".to_string(),
        format!(
            "$ tree -L 4 {}",
            workspace.display_relative(&workspace.project_dir)
        ),
    ];
    match workspace.render_tree_listing(4, 200) {
        Ok(tree) => log_lines.push(tree),
        Err(err) => log_lines.push(format!("生成目录树失败: {}", err)),
    }
    log_lines.push("\n沙盒运行时 UDP 配置:".to_string());
    log_lines.push(format!(
        "{} -> connect=syslog_udp_src, port={}",
        workspace.display_relative(&workspace.project_dir.join("topology/sources/wpsrc.toml")),
        SANDBOX_RUNTIME_UDP_PORT
    ));
    log_lines.push(format!(
        "{} -> connect=syslog_udp_sink, port={}",
        workspace.display_relative(&workspace.project_dir.join("conf/wpgen.toml")),
        SANDBOX_RUNTIME_UDP_PORT
    ));
    log_lines.push(format!(
        "{} -> 已固定复写为沙盒输出 sink",
        workspace.display_relative(
            &workspace
                .project_dir
                .join("topology/sinks/business.d/sink.toml"),
        )
    ));
    let log_path = workspace
        .write_text_log("prepare.log", &log_lines.join("\n"))
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::PrepareWorkspace,
        &log_path,
        Some(&workspace),
    )
    .await;

    resources.workspace = Some(workspace);
    Ok("已复制到沙盒目录".to_string())
}

async fn stage_preflight_check(
    task: &Arc<SandboxTaskHandle>,
    resources: &RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let mut log_lines: Vec<String> = Vec::new();

    log_lines.push("开始预检查：验证命令与必要文件".to_string());

    log_lines.push("\n检查 wparse 命令".to_string());
    match process_guard::command_version_output("wparse").await {
        Ok(version) => {
            let version_text = if version.is_empty() {
                "wparse --version 未输出版本信息".to_string()
            } else {
                format!("wparse --version 输出: {}", version)
            };
            log_lines.push(version_text);
        }
        Err(err) => {
            log_lines.push(format!("wparse --version 执行失败: {}", err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "PREFLIGHT_CHECK_FAILED",
            )
            .await;
        }
    }

    log_lines.push("\n检查 wpgen 命令".to_string());
    match process_guard::command_version_output("wpgen").await {
        Ok(version) => {
            let version_text = if version.is_empty() {
                "wpgen --version 未输出版本信息".to_string()
            } else {
                format!("wpgen --version 输出: {}", version)
            };
            log_lines.push(version_text);
        }
        Err(err) => {
            log_lines.push(format!("wpgen --version 执行失败: {}", err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "PREFLIGHT_CHECK_FAILED",
            )
            .await;
        }
    }

    log_lines.push(format!("\n检查 UDP 端口 {}", SANDBOX_RUNTIME_UDP_PORT));
    match process_guard::ensure_udp_port_available(SANDBOX_RUNTIME_UDP_PORT) {
        Ok(_) => {
            log_lines.push(format!("UDP 端口 {} 可用", SANDBOX_RUNTIME_UDP_PORT));
        }
        Err(err) => {
            log_lines.push(format!(
                "UDP 端口 {} 不可用: {}",
                SANDBOX_RUNTIME_UDP_PORT, err
            ));
            let summary = format!(
                "UDP 端口 {} 已被占用，请点击查看详情",
                SANDBOX_RUNTIME_UDP_PORT
            );
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                &summary,
                "SANDBOX_UDP_PORT_UNAVAILABLE",
            )
            .await;
        }
    }

    log_lines.push("\n检查 wproj 命令".to_string());
    match process_guard::command_version_output("wproj").await {
        Ok(version) => {
            let version_text = if version.is_empty() {
                "wproj --version 未输出版本信息".to_string()
            } else {
                format!("wproj --version 输出: {}", version)
            };
            log_lines.push(version_text);
        }
        Err(err) => {
            log_lines.push(format!("wproj --version 执行失败: {}", err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "PREFLIGHT_CHECK_FAILED",
            )
            .await;
        }
    }

    let wpgen_conf = workspace.project_dir.join("conf/wpgen.toml");
    log_lines.push(format!(
        "\n检查文件：{}",
        workspace.display_relative(&wpgen_conf)
    ));
    if let Err(err) = ensure_exists(&wpgen_conf) {
        log_lines.push(format!("检查失败：{}", err.summary));
        return fail_preflight_check(
            task,
            &workspace,
            &log_lines,
            "命令检查失败，请点击查看详情",
            "PREFLIGHT_CHECK_FAILED",
        )
        .await;
    }

    log_lines.push("命令与文件检查通过".to_string());
    log_lines.push("\n执行 wproj check".to_string());
    match process_guard::run_wproj_check(&workspace.project_dir).await {
        Ok(output) => {
            if output.is_empty() {
                log_lines.push("wproj check 通过: 未返回额外输出".to_string());
            } else {
                log_lines.push(format!("wproj check 通过: {}", output));
            }
        }
        Err(err) => {
            log_lines.push(format!("wproj check 失败: {}", err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "WPROJ_CHECK_FAILED",
            )
            .await;
        }
    }

    log_lines.push("预检查全部通过".to_string());
    write_preflight_log(task, &workspace, &log_lines).await?;
    Ok("命令检查均通过".to_string())
}

async fn write_preflight_log(
    task: &Arc<SandboxTaskHandle>,
    workspace: &SandboxWorkspace,
    log_lines: &[String],
) -> Result<(), StageError> {
    let log_path = workspace
        .write_text_log("check.log", &log_lines.join("\n"))
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::PreflightCheck,
        &log_path,
        Some(workspace),
    )
    .await;
    Ok(())
}

async fn fail_preflight_check(
    task: &Arc<SandboxTaskHandle>,
    workspace: &SandboxWorkspace,
    log_lines: &[String],
    summary: &str,
    code: &str,
) -> Result<String, StageError> {
    write_preflight_log(task, workspace, log_lines).await?;
    Err(StageError::with_code(summary, code))
}

async fn stage_start_daemon(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let log_path = workspace.log_path("wparse.log");
    if let Err(err) = process_guard::ensure_udp_port_available(SANDBOX_RUNTIME_UDP_PORT) {
        let log_text = format!(
            "启动 wparse 前检查 UDP 端口失败\nUDP 端口 {} 不可用: {}\n",
            SANDBOX_RUNTIME_UDP_PORT, err
        );
        let written_log = workspace
            .write_text_log("wparse.log", &log_text)
            .map_err(to_stage_error)?;
        set_stage_log_path(
            task,
            SandboxStage::StartDaemon,
            &written_log,
            Some(&workspace),
        )
        .await;
        return Err(StageError::with_code(
            format!(
                "UDP 端口 {} 已被占用，请点击查看详情",
                SANDBOX_RUNTIME_UDP_PORT
            ),
            "SANDBOX_UDP_PORT_UNAVAILABLE",
        ));
    }

    let mut daemon = process_guard::spawn_daemon(&workspace.project_dir, &log_path)
        .await
        .map_err(to_stage_error)?;
    resources.set_daemon_log(log_path.clone());
    let wait_result = daemon.wait_for_marker(
        "engine started",
        Duration::from_millis(resources.options.startup_timeout_ms),
    );

    set_stage_log_path(task, SandboxStage::StartDaemon, &log_path, Some(&workspace)).await;

    match wait_result.await {
        Ok(_) => {
            resources.metrics_mut().daemon_ready = true;
            resources.daemon = Some(daemon);
            Ok("wparse已启动, 等待模拟数据发送分析".to_string())
        }
        Err(err) => {
            resources.daemon = Some(daemon);
            Err(StageError::with_code(
                "wparse启动失败，请点击查看详情",
                format!("DAEMON_START_FAILED: {}", err),
            ))
        }
    }
}

async fn stage_run_wpgen(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let log_path = workspace.log_path("wpgen.log");
    let output = process_guard::run_wpgen(
        &workspace.project_dir,
        &log_path,
        resources.options.sample_count,
        Duration::from_millis(resources.options.wpgen_timeout_ms),
    )
    .await
    .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::RunWpgen,
        &output.log_path,
        Some(&workspace),
    )
    .await;

    let (count, _) = sandbox_analyzer::analyse_wpgen_result(&output.log_path).map_err(|err| {
        StageError::with_code(
            "wpgen启动失败，请点击查看详情",
            err.code
                .unwrap_or_else(|| "WPGEN_ANALYSE_FAILED".to_string()),
        )
    })?;
    let metrics = resources.metrics_mut();
    metrics.wpgen_exit_code = output.exit_code;
    metrics.input_count = count;
    metrics.wpgen_generated = Some(count);

    Ok(format!("wpgen已启动, 已发送{}条消息", count))
}

async fn stage_analyse_runtime_output(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let wait_ms = resources.options.runtime_collect_ms;
    sleep(Duration::from_millis(wait_ms)).await;

    let workspace = resources.workspace()?.clone();
    let daemon_log = resources
        .daemon_log
        .clone()
        .ok_or_else(|| StageError::new("daemon 尚未启动"))?;

    let expected_success = if resources.metrics.input_count > 0 {
        resources.metrics.input_count
    } else {
        resources.options.sample_count as usize
    };
    let analysis = sandbox_analyzer::analyse_runtime_output(
        &workspace.project_dir,
        &daemon_log,
        expected_success,
    )?;
    resources.output_checks = analysis.output_checks.clone();
    let metrics_mut = resources.metrics_mut();
    metrics_mut.miss_count = analysis.metrics.miss_count;
    metrics_mut.error_count = analysis.metrics.error_count;
    metrics_mut.output_count = analysis.metrics.output_count;

    let mut log_text = format!("等待 {}ms 收集 wparse 输出\n\n", wait_ms);
    log_text.push_str(&analysis.log_text);
    let log_path = workspace
        .write_text_log("analysis.log", &log_text)
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::AnalyseRuntimeOutput,
        &log_path,
        Some(&workspace),
    )
    .await;

    if analysis.passed {
        Ok(format!(
            "已模拟{}条消息，成功输出{}条。",
            expected_success, analysis.metrics.output_count
        ))
    } else {
        let mut details: Vec<String> = analysis
            .output_checks
            .iter()
            .filter(|check| !check.is_empty)
            .map(|check| format!("{} 非空（{}行）", check.relative_path, check.line_count))
            .collect();
        if analysis.metrics.error_count > 0 {
            details.push(format!(
                "wparse ERROR 日志 {} 条",
                analysis.metrics.error_count
            ));
        }
        if analysis.metrics.miss_count > 0 {
            details.push(format!("rule miss 日志 {} 条", analysis.metrics.miss_count));
        }
        if analysis.metrics.output_count != expected_success {
            details.push(format!(
                "成功输出数量 {} 条，期望 {} 条",
                analysis.metrics.output_count, expected_success
            ));
        }
        let summary = if details.is_empty() {
            "结果检查失败，请点击查看详情".to_string()
        } else {
            format!("结果检查失败：{}", details.join("；"))
        };
        Err(StageError::with_code(summary, "RUNTIME_ANALYSIS_FAILED"))
    }
}

async fn cleanup_resources(resources: &mut RunResources) {
    if let Some(daemon) = resources.daemon.take() {
        let _ = daemon.terminate().await;
    }
    if let Some(workspace) = resources.workspace.take() {
        let _ = workspace.cleanup_after_run(resources.options.keep_workspace);
    }
}

fn ensure_exists(path: &Path) -> Result<(), StageError> {
    if path.exists() {
        Ok(())
    } else {
        Err(StageError::new(format!("缺少必要文件: {}", path.display())))
    }
}

async fn mark_stage_running(task: &Arc<SandboxTaskHandle>, stage: SandboxStage) {
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

async fn mark_stage_success(task: &Arc<SandboxTaskHandle>, stage: SandboxStage, summary: String) {
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

async fn mark_stage_failure(
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

async fn mark_stage_stopped(task: &Arc<SandboxTaskHandle>, stage: SandboxStage, summary: &str) {
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

async fn set_stage_log_path(
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

async fn skip_following_stages(task: &Arc<SandboxTaskHandle>, current: SandboxStage) {
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

async fn apply_conclusion(task: &Arc<SandboxTaskHandle>, resources: &mut RunResources) {
    if resources.output_checks.is_empty() && resources.metrics.input_count == 0 {
        return;
    }
    let base_conclusion =
        sandbox_analyzer::finalize_conclusion(&resources.output_checks, &resources.metrics);
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
            .filter(|item| !item.is_empty)
            .map(|item| item.relative_path.clone())
            .collect();
        run.conclusion = Some(updated);
    })
    .await;
}

async fn finalize_run(task: &Arc<SandboxTaskHandle>, state: RunEndState) {
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

async fn attach_stage_diagnostics(
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
    let hits = sandbox_diagnostics::collect_stage_hits(stage, log_path.as_deref(), workspace_dir);
    store_stage_diagnostics(task, stage, hits).await;
}

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

enum RunEndState {
    Success,
    Failed {
        stage: SandboxStage,
        summary: String,
    },
    Stopped,
}

impl RunEndState {
    fn as_label(&self) -> &'static str {
        match self {
            RunEndState::Success => "success",
            RunEndState::Failed { .. } => "failed",
            RunEndState::Stopped => "stopped",
        }
    }
}

struct RunResources {
    workspace: Option<SandboxWorkspace>,
    daemon: Option<DaemonProcess>,
    output_checks: Vec<OutputFileStatus>,
    metrics: RuntimeMetrics,
    options: RunOptions,
    daemon_log: Option<PathBuf>,
}

impl RunResources {
    fn new(options: RunOptions) -> Self {
        Self {
            workspace: None,
            daemon: None,
            output_checks: Vec::new(),
            metrics: RuntimeMetrics::default(),
            options,
            daemon_log: None,
        }
    }

    fn workspace(&self) -> Result<&SandboxWorkspace, StageError> {
        self.workspace
            .as_ref()
            .ok_or_else(|| StageError::new("沙盒目录尚未准备"))
    }

    fn workspace_dir(&self) -> Option<PathBuf> {
        self.workspace.as_ref().map(|ws| ws.project_dir.clone())
    }

    fn metrics_mut(&mut self) -> &mut RuntimeMetrics {
        &mut self.metrics
    }

    fn set_daemon_log(&mut self, path: PathBuf) {
        self.daemon_log = Some(path);
    }
}

fn to_stage_error(err: AppError) -> StageError {
    StageError::new(err.to_string())
}
