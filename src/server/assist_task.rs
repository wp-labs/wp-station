// AI 辅助规则编写业务逻辑层
// AI 与人工提单共用同一套逻辑，Station 只负责存储任务和提供 reply 接口

use crate::db::{
    AssistTargetRule, AssistTask, AssistTaskStatus, AssistTaskType, NewAssistTask,
    create_assist_task, find_assist_task_by_id, list_assist_tasks, update_assist_task_reply,
    update_assist_task_status,
};
use crate::error::AppError;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::AssistServiceError;
use crate::utils::pagination::{PageQuery, PageResponse};
use crate::utils::{AiAnalyzeRequest, AssistResultResponse, AssistService, ManualTicketRequest};
use chrono::Utc;
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};

// ============ 请求/响应结构体 ============

#[derive(Deserialize)]
pub struct AssistSubmitRequest {
    /// 任务类型：ai / manual
    pub task_type: String,
    /// 目标规则类型：wpl / oml / both
    pub target_rule: String,
    /// 用户提交的日志数据
    pub log_data: String,
    /// 当前已有的规则内容（供 AI 参考，可为空）
    pub current_rule: Option<String>,
    /// 用户补充说明（仅 manual 类型使用，可为空）
    pub extra_note: Option<String>,
}

#[derive(Serialize)]
pub struct AssistSubmitResponse {
    pub task_id: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct AssistTaskDetail {
    pub task_id: String,
    pub task_type: String,
    pub target_rule: String,
    pub status: String,
    pub wpl_suggestion: Option<String>,
    pub oml_suggestion: Option<String>,
    pub explanation: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    /// 已等待秒数（从创建到现在）
    pub wait_seconds: i64,
}

/// reply 接口请求体，task_id 在 body 中传递
#[derive(Deserialize)]
pub struct AssistReplyRequest {
    pub task_id: String,
    pub wpl_suggestion: Option<String>,
    pub oml_suggestion: Option<String>,
    pub explanation: Option<String>,
}

#[derive(Deserialize)]
pub struct AssistListQuery {
    #[serde(flatten)]
    pub page: PageQuery,
}

pub type AssistListResponse = PageResponse<AssistTaskDetail>;

// ============ 业务逻辑函数 ============

fn parse_task_type(task_type: &str) -> Result<AssistTaskType, AppError> {
    task_type
        .parse::<AssistTaskType>()
        .map_err(|_| AppError::validation(format!("不支持的 task_type: {}", task_type)))
}

fn parse_target_rule(target_rule: &str) -> Result<AssistTargetRule, AppError> {
    target_rule
        .parse::<AssistTargetRule>()
        .map_err(|_| AppError::validation(format!("不支持的 target_rule: {}", target_rule)))
}

fn parse_task_status(status: &str) -> Result<AssistTaskStatus, AppError> {
    status
        .parse::<AssistTaskStatus>()
        .map_err(|_| AppError::internal(format!("未知辅助任务状态: {}", status)))
}

fn build_callback_url(setting: &Setting) -> String {
    format!(
        "http://{}:{}/api/assist/reply",
        setting.web.host, setting.web.port
    )
}

fn build_assist_task_detail(task: AssistTask) -> AssistTaskDetail {
    let wait_seconds = (Utc::now() - task.created_at).num_seconds();

    AssistTaskDetail {
        task_id: task.task_id,
        task_type: task.task_type,
        target_rule: task.target_rule,
        status: task.status,
        wpl_suggestion: task.wpl_suggestion,
        oml_suggestion: task.oml_suggestion,
        explanation: task.explanation,
        error_message: task.error_message,
        created_at: task.created_at.to_rfc3339(),
        updated_at: task.updated_at.to_rfc3339(),
        wait_seconds,
    }
}

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
        "error" | "failed" | "fail" => Some(RemoteTaskStatus::Error),
        "cancelled" | "canceled" => Some(RemoteTaskStatus::Cancelled),
        _ => None,
    }
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
            data.error_message
                .clone()
                .or_else(|| data.explanation.clone())
        })
        .unwrap_or_else(|| {
            format!(
                "远端辅助任务执行失败: task_id={}, status={}",
                task_id, remote_result.status
            )
        })
}

async fn try_sync_assist_task_result(task: &AssistTask) {
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

    match parse_remote_task_status(&remote_result.status) {
        Some(RemoteTaskStatus::Success) => {
            let data = remote_result.data.unwrap_or_default();
            if let Err(err) = update_assist_task_reply(
                &task.task_id,
                data.wpl_suggestion,
                data.oml_suggestion,
                data.explanation,
            )
            .await
            {
                warn!(
                    "同步远端辅助任务结果失败: task_id={}, error={}",
                    task.task_id, err
                );
                return;
            }

            info!("同步远端辅助任务结果成功: task_id={}", task.task_id);
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

            info!("同步远端辅助任务失败状态成功: task_id={}", task.task_id);
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

            info!("同步远端辅助任务取消状态成功: task_id={}", task.task_id);
        }
        Some(RemoteTaskStatus::Pending | RemoteTaskStatus::Processing) => {
            debug!(
                "远端辅助任务仍在处理中: task_id={}, remote_status={}",
                task.task_id, remote_result.status
            );
        }
        None => {
            warn!(
                "远端辅助任务状态无法识别: task_id={}, remote_status={}",
                task.task_id, remote_result.status
            );
        }
    }
}

async fn spawn_manual_ticket_dispatch(
    task_id: String,
    target_rule: AssistTargetRule,
    log_data: String,
    current_rule: Option<String>,
    extra_note: Option<String>,
    setting: Setting,
) {
    let base_url = setting.assist.base_url.clone();
    if base_url.is_empty() {
        warn!(
            "人工提单推送地址未配置: task_id={}, task_type=manual",
            task_id
        );
        return;
    }

    let payload = ManualTicketRequest {
        task_id: task_id.clone(),
        target_rule: target_rule.as_ref().to_string(),
        log_data,
        current_rule,
        extra_note,
        callback_url: build_callback_url(&setting),
    };

    info!(
        "推送人工工单: task_id={}, endpoint={}/ticket",
        task_id, base_url
    );

    let service = match AssistService::new() {
        Ok(service) => service,
        Err(err) => {
            warn!("构建人工工单客户端失败: task_id={}, error={}", task_id, err);
            return;
        }
    };

    if let Err(err) = service.submit_manual_ticket(&base_url, &payload).await {
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
        return;
    }

    info!("人工工单推送成功: task_id={}", task_id);
}

async fn spawn_ai_task_dispatch(
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
        callback_url: build_callback_url(&setting),
    };

    info!(
        "调用 AI 服务: task_id={}, endpoint={}/analyze",
        task_id, ai_base_url
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

/// 提交辅助任务
/// AI 类型：写库后 tokio::spawn 后台调用远端 AI 服务
/// 人工类型：写库后等待远端平台通过 reply 接口写回结果
pub async fn assist_submit_logic(
    req: AssistSubmitRequest,
) -> Result<AssistSubmitResponse, AppError> {
    let task_type = parse_task_type(&req.task_type)?;
    let target_rule = parse_target_rule(&req.target_rule)?;
    info!(
        "提交辅助任务: task_type={}, target_rule={}",
        task_type, target_rule
    );

    // 生成全局唯一 task_id
    let random_suffix: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(6)
        .map(char::from)
        .collect();
    let task_id = format!("assist-{}-{}", Utc::now().timestamp_millis(), random_suffix);

    let new_task = NewAssistTask {
        task_id: task_id.clone(),
        task_type,
        target_rule,
        log_data: req.log_data.clone(),
        current_rule: req.current_rule.clone(),
        extra_note: req.extra_note.clone(),
    };

    let result = async {
        create_assist_task(new_task).await?;

        let setting = Setting::load();
        match task_type {
            AssistTaskType::Manual => {
                let task_id_clone = task_id.clone();
                let log_data_clone = req.log_data.clone();
                let current_rule_clone = req.current_rule.clone();
                let extra_note_clone = req.extra_note.clone();

                tokio::spawn(async move {
                    spawn_manual_ticket_dispatch(
                        task_id_clone,
                        target_rule,
                        log_data_clone,
                        current_rule_clone,
                        extra_note_clone,
                        setting,
                    )
                    .await;
                });
            }
            AssistTaskType::Ai => {
                let task_id_clone = task_id.clone();
                let log_data_clone = req.log_data.clone();
                let current_rule_clone = req.current_rule.clone();

                tokio::spawn(async move {
                    spawn_ai_task_dispatch(
                        task_id_clone,
                        target_rule,
                        log_data_clone,
                        current_rule_clone,
                        setting,
                    )
                    .await;
                });
            }
        }

        Ok::<_, AppError>(AssistSubmitResponse {
            task_id: task_id.clone(),
            status: AssistTaskStatus::Pending.as_ref().to_string(),
        })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::AssistTask,
        OperationLogAction::Submit,
        OperationLogParams::new()
            .with_target_name(format!("{} [{}]", task_id, task_type.as_ref()))
            .with_field("task_id", &task_id)
            .with_field("task_type", task_type.as_ref())
            .with_field("target_rule", target_rule.as_ref()),
        &result,
    )
    .await;
    result
}

/// 查询辅助任务详情及当前状态
pub async fn assist_get_logic(task_id: String) -> Result<AssistTaskDetail, AppError> {
    let mut task = find_assist_task_by_id(&task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("辅助任务 {} 不存在", task_id)))?;

    let task_status = parse_task_status(&task.status)?;
    if matches!(
        task_status,
        AssistTaskStatus::Pending | AssistTaskStatus::Processing
    ) {
        try_sync_assist_task_result(&task).await;
        task = find_assist_task_by_id(&task_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("辅助任务 {} 不存在", task_id)))?;
    }

    Ok(build_assist_task_detail(task))
}

/// 分页查询辅助任务列表
pub async fn assist_list_logic(query: AssistListQuery) -> Result<AssistListResponse, AppError> {
    let (page, page_size) = query.page.normalize_default();

    let (tasks, total) = list_assist_tasks(page as u64, page_size as u64).await?;

    let items = tasks.into_iter().map(build_assist_task_detail).collect();

    Ok(AssistListResponse::from_db(
        items,
        total as i64,
        page,
        page_size,
    ))
}

/// 取消等待中的辅助任务
pub async fn assist_cancel_logic(task_id: String) -> Result<(), AppError> {
    let task = find_assist_task_by_id(&task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("辅助任务 {} 不存在", task_id)))?;

    let task_status = parse_task_status(&task.status)?;
    let task_type = task.task_type.clone();
    let target_rule = task.target_rule.clone();

    let result = async {
        if !matches!(
            task_status,
            AssistTaskStatus::Pending | AssistTaskStatus::Processing
        ) {
            return Err(AppError::Validation(format!(
                "任务状态为 {}，无法取消",
                task.status
            )));
        }

        update_assist_task_status(&task_id, AssistTaskStatus::Cancelled, None).await?;
        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::AssistTask,
        OperationLogAction::Cancel,
        OperationLogParams::new()
            .with_target_name(format!("{} [{}]", task_id, task_type))
            .with_field("task_id", &task_id)
            .with_field("task_type", &task_type)
            .with_field("target_rule", &target_rule),
        &result,
    )
    .await;
    result
}

/// 写回辅助任务结果（AI 服务回调或人工平台回调均调用此接口）
/// task_id 通过请求体传递，不在 URL 路径中
pub async fn assist_reply_logic(req: AssistReplyRequest) -> Result<(), AppError> {
    let task = find_assist_task_by_id(&req.task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("辅助任务 {} 不存在", req.task_id)))?;

    let task_type = task.task_type.clone();
    let target_rule = task.target_rule.clone();
    let task_id = req.task_id.clone();

    let result = async {
        update_assist_task_reply(
            &task_id,
            req.wpl_suggestion,
            req.oml_suggestion,
            req.explanation,
        )
        .await?;

        info!("辅助任务结果写回成功: task_id={}", task_id);
        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::AssistTask,
        OperationLogAction::Reply,
        OperationLogParams::new()
            .with_target_name(format!("{} [{}]", task_id, task_type))
            .with_field("task_id", &task_id)
            .with_field("task_type", &task_type)
            .with_field("target_rule", &target_rule),
        &result,
    )
    .await;
    result
}
