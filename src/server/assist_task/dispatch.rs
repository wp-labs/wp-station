//! 辅助任务提交前检查与后台派发逻辑。

use crate::constants::assist::STALE_AI_TASK_RELEASE_SECONDS;
use crate::db::{
    AssistTargetRule, AssistTask, AssistTaskStatus, AssistTaskType,
    find_active_assist_task_by_type, update_assist_task_status,
};
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::{AiAnalyzeRequest, AssistService, AssistServiceError, ManualTicketRequest};
use chrono::Utc;

/// 构造“已有进行中任务”冲突错误。
pub(super) fn build_active_task_conflict_error(
    task_type: AssistTaskType,
    active_task: &AssistTask,
) -> AppError {
    AppError::conflict_with_code(
        "ASSIST_TASK_ALREADY_RUNNING",
        format!(
            "已有进行中的{}任务，请等待当前任务完成后再提交: task_id={}",
            match task_type {
                AssistTaskType::Ai => "AI 辅助",
                AssistTaskType::Manual => "人工提单",
            },
            active_task.task_id
        ),
    )
}

/// 提交前检查同类型进行中任务，并尝试释放陈旧 AI 任务占用。
pub(super) async fn resolve_active_task_before_submit(
    task_type: AssistTaskType,
) -> Result<Option<AssistTask>, AppError> {
    let mut active_task = find_active_assist_task_by_type(task_type).await?;

    if let Some(task) = active_task.as_ref() {
        debug!(
            "提交辅助任务前发现进行中任务，先尝试同步远端状态: task_id={}, task_type={}",
            task.task_id, task_type
        );
        super::remote::try_sync_assist_task_result(task).await;
        active_task = find_active_assist_task_by_type(task_type).await?;
    }

    if task_type == AssistTaskType::Ai
        && let Some(task) = active_task.as_ref()
    {
        let wait_seconds = (Utc::now() - task.created_at).num_seconds();
        if wait_seconds >= STALE_AI_TASK_RELEASE_SECONDS {
            let error_message = format!(
                "AI 任务长时间未返回结果，已自动释放占用，请重新提交: task_id={}",
                task.task_id
            );
            warn!(
                "检测到陈旧 AI 任务，自动释放占用: task_id={}, wait_seconds={}",
                task.task_id, wait_seconds
            );
            update_assist_task_status(&task.task_id, AssistTaskStatus::Error, Some(error_message))
                .await?;
            active_task = find_active_assist_task_by_type(task_type).await?;
        }
    }

    Ok(active_task)
}

/// 后台派发人工提单请求。
pub(super) async fn spawn_manual_ticket_dispatch(
    task_id: String,
    target_rule: AssistTargetRule,
    log_data: String,
    current_rule: Option<String>,
    extra_note: Option<String>,
    setting: Setting,
) {
    let base_url = setting.assist.base_url.clone();
    if base_url.is_empty() {
        let error_message =
            "人工提单推送地址未配置，请在 config.toml [assist] 中设置 base_url".to_string();
        warn!(
            "人工提单推送地址未配置: task_id={}, task_type=manual",
            task_id
        );
        if let Err(db_err) =
            update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message)).await
        {
            warn!(
                "辅助任务状态更新失败: task_id={}, status=error, error={}",
                task_id, db_err
            );
        }
        return;
    }

    let payload = ManualTicketRequest {
        task_id: task_id.clone(),
        target_rule: target_rule.as_ref().to_string(),
        log_data,
        current_rule,
        extra_note,
        callback_url: super::build_callback_url(&setting),
    };

    info!(
        "推送人工工单: task_id={}, endpoint={}/ticket",
        task_id, base_url
    );

    let service = match AssistService::new() {
        Ok(service) => service,
        Err(err) => {
            let error_message = format!("构建人工工单客户端失败: {}", err);
            warn!("构建人工工单客户端失败: task_id={}, error={}", task_id, err);
            if let Err(db_err) =
                update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message))
                    .await
            {
                warn!(
                    "辅助任务状态更新失败: task_id={}, status=error, error={}",
                    task_id, db_err
                );
            }
            return;
        }
    };

    if let Err(err) = service.submit_manual_ticket(&base_url, &payload).await {
        let error_message = format!("人工工单推送失败: {}", err);
        match err {
            AssistServiceError::ResponseError { status, .. } => warn!(
                "人工工单推送失败: task_id={}, endpoint={}/ticket, status={}, error={}",
                task_id, base_url, status, err
            ),
            _ => warn!(
                "人工工单推送失败: task_id={}, endpoint={}/ticket, error={}",
                task_id, base_url, err
            ),
        }
        if let Err(db_err) =
            update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message)).await
        {
            warn!(
                "辅助任务状态更新失败: task_id={}, status=error, error={}",
                task_id, db_err
            );
        }
        return;
    }

    info!("人工工单推送成功: task_id={}", task_id);
}

/// 后台派发 AI 分析任务。
pub(super) async fn spawn_ai_task_dispatch(
    task_id: String,
    target_rule: AssistTargetRule,
    log_data: String,
    current_rule: Option<String>,
    setting: Setting,
) {
    if let Err(err) = update_assist_task_status(&task_id, AssistTaskStatus::Processing, None).await
    {
        warn!(
            "辅助任务状态更新失败: task_id={}, status=processing, error={}",
            task_id, err
        );
        return;
    }

    let ai_base_url = setting.assist.base_url.clone();
    if ai_base_url.is_empty() {
        let error_message =
            "AI 服务地址未配置，请在 config.toml [assist] 中设置 base_url".to_string();
        if let Err(err) =
            update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message)).await
        {
            warn!(
                "辅助任务状态更新失败: task_id={}, status=error, error={}",
                task_id, err
            );
        }
        return;
    }

    let request = AiAnalyzeRequest {
        task_id: task_id.clone(),
        target_rule: target_rule.as_ref().to_string(),
        log_data,
        current_rule,
        callback_url: super::build_callback_url(&setting),
    };

    if setting.assist.callback_base_url.trim().is_empty()
        && super::is_non_public_callback_host(&setting.web.host)
    {
        warn!(
            "AI 回调地址可能不可被外部访问: task_id={}, callback_url={}, web_host={}, 建议在 [assist] 中显式配置 callback_base_url",
            task_id, request.callback_url, setting.web.host
        );
    }

    info!(
        "调用 AI 服务: task_id={}, endpoint={}/analyze, target_rule={}, callback_url={}, log_data_len={}, log_data_preview={}, current_rule_present={}, current_rule_len={}, current_rule_preview={}",
        task_id,
        ai_base_url,
        request.target_rule,
        request.callback_url,
        request.log_data.len(),
        super::preview_text(&request.log_data, 120),
        request
            .current_rule
            .as_ref()
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        request
            .current_rule
            .as_ref()
            .map(|value| value.len())
            .unwrap_or(0),
        request
            .current_rule
            .as_deref()
            .map(|value| super::preview_text(value, 120))
            .unwrap_or_else(|| "-".to_string())
    );

    let service = match AssistService::new() {
        Ok(service) => service,
        Err(err) => {
            let error_message = format!("构建 AI 客户端失败: {}", err);
            warn!(
                "调用 AI 服务失败: task_id={}, endpoint={}/analyze, error={}",
                task_id, ai_base_url, err
            );
            let _ =
                update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message))
                    .await;
            return;
        }
    };

    if let Err(err) = service.submit_ai_task(&ai_base_url, &request).await {
        let error_message = format!("调用 AI 服务失败: {}", err);
        match err {
            AssistServiceError::ResponseError { status, .. } => warn!(
                "调用 AI 服务失败: task_id={}, endpoint={}/analyze, status={}, error={}",
                task_id, ai_base_url, status, error_message
            ),
            _ => warn!(
                "调用 AI 服务失败: task_id={}, endpoint={}/analyze, error={}",
                task_id, ai_base_url, error_message
            ),
        }

        if let Err(db_err) =
            update_assist_task_status(&task_id, AssistTaskStatus::Error, Some(error_message)).await
        {
            warn!(
                "辅助任务状态更新失败: task_id={}, status=error, error={}",
                task_id, db_err
            );
        }
        return;
    }

    info!("AI 服务已接受任务: task_id={}", task_id);
}
