//! 沙盒任务查询相关业务。

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use crate::constants::sandbox::MAX_LOG_LINES;
use crate::db::{
    count_sandbox_runs_by_release, find_release_by_id, find_sandbox_run_by_task_id,
    list_sandbox_runs_by_release,
};
use crate::error::AppError;
use crate::server::Setting;

use super::{
    SandboxHistoryItem, SandboxHistoryResponse, SandboxLatestResponse, SandboxRun, SandboxStage,
    SandboxStageLogResponse, SandboxState,
};

/// 查询指定任务 ID 的沙盒运行记录。
pub async fn get_sandbox_run_logic(
    state: SandboxState,
    task_id: &str,
) -> Result<SandboxRun, AppError> {
    if let Some(run) = state.snapshot_by_id(task_id).await {
        return Ok(run);
    }
    if let Some(run) = find_sandbox_run_by_task_id(task_id)
        .await
        .map_err(AppError::from)?
    {
        return Ok(run);
    }
    Err(AppError::not_found("沙盒任务不存在"))
}

/// 查询指定发布的沙盒历史记录，包含正在执行的任务。
pub async fn list_sandbox_history_logic(
    state: SandboxState,
    release_id: i32,
    limit: u64,
) -> Result<SandboxHistoryResponse, AppError> {
    find_release_by_id(release_id)
        .await?
        .ok_or_else(|| AppError::not_found("发布记录不存在"))?;

    let capped = limit.max(1);
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    let active_runs = state.active_runs_for_release(release_id).await;
    for run in active_runs {
        seen.insert(run.task_id.clone());
        items.push(history_item_from_run(&run));
    }

    let remaining = capped.saturating_sub(items.len() as u64);
    if remaining == 0 {
        items.truncate(capped as usize);
        return Ok(SandboxHistoryResponse {
            total: items.len(),
            items,
        });
    }

    let db_limit = remaining.saturating_add(seen.len() as u64);
    let db_runs = list_sandbox_runs_by_release(release_id, Some(db_limit))
        .await
        .map_err(AppError::from)?;

    for run in db_runs {
        if seen.contains(&run.task_id) {
            continue;
        }
        items.push(history_item_from_run(&run));
        if items.len() as u64 >= capped {
            break;
        }
    }

    let total_count = count_sandbox_runs_by_release(release_id)
        .await
        .map_err(AppError::from)?
        .max(items.len() as u64) as usize;

    Ok(SandboxHistoryResponse {
        total: total_count,
        items,
    })
}

/// 获取指定阶段的日志内容。
pub async fn get_stage_logs_logic(
    state: SandboxState,
    task_id: &str,
    stage: SandboxStage,
) -> Result<SandboxStageLogResponse, AppError> {
    let run = if let Some(current) = state.snapshot_by_id(task_id).await {
        current
    } else {
        find_sandbox_run_by_task_id(task_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::not_found("沙盒任务不存在"))?
    };

    let stage_info = run
        .stages
        .iter()
        .find(|item| item.stage == stage)
        .ok_or_else(|| AppError::not_found("阶段信息不存在"))?;

    let content = if let Some(path) = &stage_info.log_path {
        read_log_content(path)
    } else {
        Ok(String::new())
    }?;

    Ok(SandboxStageLogResponse {
        task_id: run.task_id,
        stage,
        content,
        log_path: stage_info.log_path.clone(),
    })
}

/// 查询指定发布最近一次沙盒结果。
pub async fn get_latest_sandbox_run_logic(
    state: SandboxState,
    release_id: i32,
) -> Result<SandboxLatestResponse, AppError> {
    if let Some(run) = state.latest_for_release(release_id).await {
        return Ok(SandboxLatestResponse {
            release_id,
            task_id: run.task_id,
            status: run.status,
            passed: run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false),
            failed_stage: run.conclusion.as_ref().and_then(|c| c.failed_stage),
            ended_at: run.ended_at,
        });
    }

    let mut records = list_sandbox_runs_by_release(release_id, Some(1))
        .await
        .map_err(AppError::from)?;
    if let Some(run) = records.pop() {
        return Ok(SandboxLatestResponse {
            release_id,
            task_id: run.task_id,
            status: run.status,
            passed: run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false),
            failed_stage: run.conclusion.as_ref().and_then(|c| c.failed_stage),
            ended_at: run.ended_at,
        });
    }

    Err(AppError::not_found("该发布暂无沙盒记录"))
}

/// 读取日志内容并做截断展示。
fn read_log_content(path: &str) -> Result<String, AppError> {
    if path.is_empty() {
        return Ok("日志文件路径为空".to_string());
    }
    let resolved = resolve_log_path(path);
    let path_ref = resolved.as_path();
    if !path_ref.exists() {
        return Ok("日志文件不存在，可能沙盒目录已清理或尚未生成。".to_string());
    }
    let content = std::fs::read_to_string(path_ref).map_err(AppError::internal)?;
    if content.trim().is_empty() {
        return Ok("日志为空（命令未产生任何输出）".to_string());
    }
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Ok("日志为空（命令未产生任何输出）".to_string());
    }
    if lines.len() > MAX_LOG_LINES {
        let start = lines.len() - MAX_LOG_LINES;
        let mut truncated = lines[start..].join("\n");
        truncated.push_str(&format!(
            "\n...（日志超出 {} 行，已截断，仅展示最新内容）",
            MAX_LOG_LINES
        ));
        Ok(truncated)
    } else {
        Ok(content)
    }
}

/// 将相对日志路径解析到工作区。
fn resolve_log_path(path: &str) -> PathBuf {
    let path_ref = Path::new(path);
    if path_ref.is_absolute() {
        path_ref.to_path_buf()
    } else {
        Setting::workspace_root().join(path_ref)
    }
}

/// 将运行记录压缩为历史列表项。
fn history_item_from_run(run: &SandboxRun) -> SandboxHistoryItem {
    let passed = run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false);
    let failed_stage = run.conclusion.as_ref().and_then(|c| c.failed_stage);
    SandboxHistoryItem {
        task_id: run.task_id.clone(),
        status: run.status,
        passed,
        failed_stage,
        sample_count: run.options.sample_count,
        started_at: run.started_at,
        ended_at: run.ended_at,
        duration_ms: compute_duration_ms(run.started_at, run.ended_at),
    }
}

/// 计算运行持续时长。
fn compute_duration_ms(
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
) -> Option<u64> {
    match (started_at, ended_at) {
        (Some(start), Some(end)) => {
            let duration = end.signed_duration_since(start);
            if duration.num_milliseconds() < 0 {
                Some(0)
            } else {
                Some(duration.num_milliseconds() as u64)
            }
        }
        _ => None,
    }
}
