//! 沙盒任务执行器。

use std::sync::Arc;

use chrono::Utc;
use tracing::{error, info};

use super::progress::{
    RunEndState, RunResources, apply_conclusion, attach_stage_diagnostics, cleanup_resources,
    finalize_run, mark_stage_failure, mark_stage_running, mark_stage_stopped, mark_stage_success,
    skip_following_stages,
};
use super::steps::{
    stage_analyse_runtime_output, stage_preflight_check, stage_prepare_workspace, stage_run_wpgen,
    stage_start_daemon,
};
use super::{SandboxStage, SandboxState, SandboxTaskHandle, TaskStatus};

/// 在独立 Tokio 任务中执行沙盒运行，结束后回调队列。
pub fn spawn_sandbox_execution(state: SandboxState, task: Arc<SandboxTaskHandle>) {
    tokio::spawn(async move {
        run_sandbox_task(state.clone(), task.clone()).await;
        state.on_task_completed(task).await;
    });
}

/// 顺序执行单个沙盒任务的全部阶段，并在结束后触发统一收尾。
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
    let mut resources = RunResources::new(task.system(), options.clone());
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
