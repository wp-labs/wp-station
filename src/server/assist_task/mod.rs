//! AI / 人工辅助任务业务逻辑层。
//!
//! AI 与人工提单共用同一套任务流转逻辑，
//! Station 负责存储任务、查询状态和接收结果回写。

mod dispatch;
mod remote;

use crate::db::{
    AssistTargetRule, AssistTask, AssistTaskStatus, AssistTaskType, NewAssistTask,
    create_assist_task, find_assist_task_by_id, list_assist_tasks, update_assist_task_reply,
    update_assist_task_status,
};
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::pagination::{PageQuery, PageResponse};
use chrono::Utc;
use rand::{RngExt, distr::Alphanumeric};
use serde::{Deserialize, Serialize};

// ============ 请求/响应结构体 ============

/// 提交辅助任务请求体。
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

/// 提交辅助任务响应体。
#[derive(Serialize)]
pub struct AssistSubmitResponse {
    pub task_id: String,
    pub status: String,
}

/// 辅助任务详情响应体。
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

/// 辅助任务列表查询参数。
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
    let configured = setting.assist.callback_base_url.trim();
    if !configured.is_empty() {
        return format!("{}/api/assist/reply", configured.trim_end_matches('/'));
    }

    format!(
        "http://{}:{}/api/assist/reply",
        setting.web.host, setting.web.port
    )
}

fn is_non_public_callback_host(host: &str) -> bool {
    matches!(host.trim(), "0.0.0.0" | "127.0.0.1" | "localhost")
}

fn preview_text(value: &str, limit: usize) -> String {
    let mut chars = value.chars();
    let preview: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{}...", preview)
    } else {
        preview
    }
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
    let random_suffix: String = (&mut rand::rng())
        .sample_iter(Alphanumeric)
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

    async {
        if let Some(active_task) = dispatch::resolve_active_task_before_submit(task_type).await? {
            return Err(dispatch::build_active_task_conflict_error(
                task_type,
                &active_task,
            ));
        }

        create_assist_task(new_task).await?;

        let setting = Setting::load();
        match task_type {
            AssistTaskType::Manual => {
                let task_id_clone = task_id.clone();
                let log_data_clone = req.log_data.clone();
                let current_rule_clone = req.current_rule.clone();
                let extra_note_clone = req.extra_note.clone();

                tokio::spawn(async move {
                    dispatch::spawn_manual_ticket_dispatch(
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
                    dispatch::spawn_ai_task_dispatch(
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
    .await
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
        remote::try_sync_assist_task_result(&task).await;
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
    async {
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
    .await
}

/// 写回辅助任务结果（AI 服务回调或人工平台回调均调用此接口）
/// task_id 通过请求体传递，不在 URL 路径中
pub async fn assist_reply_logic(req: AssistReplyRequest) -> Result<(), AppError> {
    find_assist_task_by_id(&req.task_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("辅助任务 {} 不存在", req.task_id)))?;

    let task_id = req.task_id.clone();

    async {
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
    .await
}
