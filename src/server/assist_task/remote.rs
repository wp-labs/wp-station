//! 辅助任务远端结果同步逻辑。

use crate::db::{
    AssistTask, AssistTaskStatus, update_assist_task_reply, update_assist_task_status,
};
use crate::server::Setting;
use crate::utils::{AssistResultResponse, AssistService, AssistServiceError};

/// 远端辅助任务状态的本地映射枚举。
#[derive(Debug, Clone, Copy)]
enum RemoteTaskStatus {
    Pending,
    Processing,
    Success,
    Error,
    Cancelled,
}

fn parse_remote_task_status(status: &str) -> Option<RemoteTaskStatus> {
    match status.trim().to_ascii_lowercase().as_str() {
        "pending" | "queued" | "submitted" => Some(RemoteTaskStatus::Pending),
        "processing" | "running" | "in_progress" => Some(RemoteTaskStatus::Processing),
        "done" | "success" | "completed" => Some(RemoteTaskStatus::Success),
        "error" | "failed" | "fail" | "callback_failed" => Some(RemoteTaskStatus::Error),
        "cancelled" | "canceled" => Some(RemoteTaskStatus::Cancelled),
        _ => None,
    }
}

fn parse_remote_result_status(remote_result: &AssistResultResponse) -> Option<RemoteTaskStatus> {
    if let Some(status) = remote_result
        .data
        .as_ref()
        .and_then(|data| data.status.as_deref())
        .and_then(parse_remote_task_status)
    {
        return Some(status);
    }

    if let Some(status) = remote_result
        .data
        .as_ref()
        .and_then(|data| data.callback_status.as_deref())
        .and_then(parse_remote_task_status)
    {
        return Some(status);
    }

    parse_remote_task_status(&remote_result.status)
}

fn remote_result_matches_task(task_id: &str, remote_result: &AssistResultResponse) -> bool {
    remote_result
        .data
        .as_ref()
        .and_then(|data| data.task_id.as_deref())
        .map(|remote_task_id| remote_task_id == task_id)
        .unwrap_or(true)
}

fn build_remote_error_message(task_id: &str, remote_result: &AssistResultResponse) -> String {
    remote_result
        .data
        .as_ref()
        .and_then(|data| {
            data.callback_error
                .clone()
                .or_else(|| data.error.clone())
                .or_else(|| data.error_message.clone())
                .or_else(|| data.execution_log.clone())
                .or_else(|| data.explanation.clone())
        })
        .unwrap_or_else(|| {
            format!(
                "远端辅助任务执行失败: task_id={}, status={}",
                task_id, remote_result.status
            )
        })
}

fn build_remote_status_summary(remote_result: &AssistResultResponse) -> String {
    let data_status = remote_result
        .data
        .as_ref()
        .and_then(|data| data.status.as_deref())
        .unwrap_or("-");
    let callback_status = remote_result
        .data
        .as_ref()
        .and_then(|data| data.callback_status.as_deref())
        .unwrap_or("-");

    format!(
        "status={}, data_status={}, callback_status={}",
        remote_result.status, data_status, callback_status
    )
}

fn is_manual_callback_failure_success_case(remote_result: &AssistResultResponse) -> bool {
    let Some(data) = remote_result.data.as_ref() else {
        return false;
    };

    matches!(
        parse_remote_task_status(&remote_result.status),
        Some(RemoteTaskStatus::Success)
    ) && matches!(
        data.callback_status
            .as_deref()
            .and_then(parse_remote_task_status),
        Some(RemoteTaskStatus::Error)
    )
}

fn remote_result_status_for_log(remote_result: &AssistResultResponse) -> String {
    build_remote_status_summary(remote_result)
}

/// 查询远端辅助平台状态，并尽量把结果同步回本地任务。
pub(super) async fn try_sync_assist_task_result(task: &AssistTask) {
    let setting = Setting::load();
    let assist_base_url = setting.assist.base_url.trim().to_string();
    if assist_base_url.is_empty() {
        return;
    }

    let service = match AssistService::new() {
        Ok(service) => service,
        Err(err) => {
            warn!(
                "构建辅助任务结果查询客户端失败: task_id={}, error={}",
                task.task_id, err
            );
            return;
        }
    };

    let remote_result = match service
        .query_task_result(&assist_base_url, &task.task_id)
        .await
    {
        Ok(result) => result,
        Err(AssistServiceError::ResponseError { status: 404, .. }) => {
            debug!("远端辅助任务结果暂未返回: task_id={}", task.task_id);
            return;
        }
        Err(err) => {
            warn!(
                "查询远端辅助任务结果失败: task_id={}, error={}",
                task.task_id, err
            );
            return;
        }
    };

    if !remote_result_matches_task(&task.task_id, &remote_result) {
        warn!(
            "远端辅助任务结果 task_id 不匹配: expected={}, remote_status={}",
            task.task_id, remote_result.status
        );
        return;
    }

    match parse_remote_result_status(&remote_result) {
        Some(RemoteTaskStatus::Success) => {
            let data = remote_result.data.as_ref();
            if let Err(err) = update_assist_task_reply(
                &task.task_id,
                data.and_then(|value| value.wpl_suggestion.clone()),
                data.and_then(|value| value.oml_suggestion.clone()),
                data.and_then(|value| value.explanation.clone()),
            )
            .await
            {
                warn!(
                    "同步远端辅助任务结果失败: task_id={}, error={}",
                    task.task_id, err
                );
                return;
            }

            if is_manual_callback_failure_success_case(&remote_result) {
                warn!(
                    "远端人工提单结果已保存但回调失败: task_id={}, {}",
                    task.task_id,
                    remote_result_status_for_log(&remote_result)
                );
            } else {
                info!(
                    "同步远端辅助任务结果成功: task_id={}, {}",
                    task.task_id,
                    remote_result_status_for_log(&remote_result)
                );
            }
        }
        Some(RemoteTaskStatus::Error) => {
            let error_message = build_remote_error_message(&task.task_id, &remote_result);
            if let Err(err) = update_assist_task_status(
                &task.task_id,
                AssistTaskStatus::Error,
                Some(error_message),
            )
            .await
            {
                warn!(
                    "同步远端辅助任务失败状态失败: task_id={}, error={}",
                    task.task_id, err
                );
                return;
            }

            info!(
                "同步远端辅助任务失败状态成功: task_id={}, {}",
                task.task_id,
                remote_result_status_for_log(&remote_result)
            );
        }
        Some(RemoteTaskStatus::Cancelled) => {
            if let Err(err) =
                update_assist_task_status(&task.task_id, AssistTaskStatus::Cancelled, None).await
            {
                warn!(
                    "同步远端辅助任务取消状态失败: task_id={}, error={}",
                    task.task_id, err
                );
                return;
            }

            info!(
                "同步远端辅助任务取消状态成功: task_id={}, {}",
                task.task_id,
                remote_result_status_for_log(&remote_result)
            );
        }
        Some(RemoteTaskStatus::Pending | RemoteTaskStatus::Processing) => {
            debug!(
                "远端辅助任务仍在处理中: task_id={}, {}",
                task.task_id,
                remote_result_status_for_log(&remote_result)
            );
        }
        None => {
            warn!(
                "远端辅助任务状态无法识别: task_id={}, {}",
                task.task_id,
                remote_result_status_for_log(&remote_result)
            );
        }
    }
}
