// AI 辅助任务数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{QueryOrder, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::assist_task::{ActiveModel, Column, Entity, Model};

pub type AssistTask = Model;

/// 辅助任务类型
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AssistTaskType {
    Ai,
    Manual,
}

/// 辅助任务目标规则类型
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AssistTargetRule {
    Wpl,
    Oml,
    Both,
}

/// 辅助任务状态
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum AssistTaskStatus {
    Pending,
    Processing,
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAssistTask {
    pub task_id: String,
    pub task_type: AssistTaskType,
    pub target_rule: AssistTargetRule,
    pub log_data: String,
    pub current_rule: Option<String>,
    pub extra_note: Option<String>,
}

// ============ 数据库操作函数 ============

/// 创建 AI 辅助任务，初始状态为 pending
pub async fn create_assist_task(task: NewAssistTask) -> DbResult<i32> {
    info!(
        "创建辅助任务: task_id={}, type={}",
        task.task_id, task.task_type
    );

    let pool = get_pool();
    let db = pool.inner();
    let now = Utc::now();

    let active_model = ActiveModel {
        task_id: Set(task.task_id),
        task_type: Set(task.task_type.as_ref().to_string()),
        target_rule: Set(task.target_rule.as_ref().to_string()),
        log_data: Set(task.log_data),
        current_rule: Set(task.current_rule),
        extra_note: Set(task.extra_note),
        status: Set(AssistTaskStatus::Pending.as_ref().to_string()),
        wpl_suggestion: Set(None),
        oml_suggestion: Set(None),
        explanation: Set(None),
        error_message: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;
    info!("辅助任务创建成功: id={}", id);
    Ok(id)
}

/// 根据 task_id 查找辅助任务
pub async fn find_assist_task_by_id(task_id: &str) -> DbResult<Option<AssistTask>> {
    debug!("查询辅助任务: task_id={}", task_id);

    let pool = get_pool();
    let db = pool.inner();

    let task = Entity::find()
        .filter(Column::TaskId.eq(task_id))
        .one(db)
        .await?;

    Ok(task)
}

/// 分页查询辅助任务列表（按创建时间倒序）
pub async fn list_assist_tasks(page: u64, page_size: u64) -> DbResult<(Vec<AssistTask>, u64)> {
    debug!("分页查询辅助任务: page={}, page_size={}", page, page_size);

    let pool = get_pool();
    let db = pool.inner();

    let paginator = Entity::find()
        .order_by_desc(Column::CreatedAt)
        .paginate(db, page_size);

    let total = paginator.num_items().await?;
    let items = paginator.fetch_page(page.saturating_sub(1)).await?;

    debug!("查询到 {} 条辅助任务，共 {} 条", items.len(), total);
    Ok((items, total))
}

/// 写回任务结果，将状态更新为 success
pub async fn update_assist_task_reply(
    task_id: &str,
    wpl_suggestion: Option<String>,
    oml_suggestion: Option<String>,
    explanation: Option<String>,
) -> DbResult<()> {
    info!("写回辅助任务结果: task_id={}", task_id);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::TaskId.eq(task_id))
        .one(db)
        .await?
        .ok_or(DbError::not_found("辅助任务"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(AssistTaskStatus::Success.as_ref().to_string());
    active_model.wpl_suggestion = Set(wpl_suggestion);
    active_model.oml_suggestion = Set(oml_suggestion);
    active_model.explanation = Set(explanation);
    active_model.error_message = Set(None);
    active_model.updated_at = Set(Utc::now());

    active_model.update(db).await?;
    info!("辅助任务结果写回成功: task_id={}", task_id);
    Ok(())
}

/// 更新任务状态（用于 processing / error / cancelled）
pub async fn update_assist_task_status(
    task_id: &str,
    status: AssistTaskStatus,
    error_message: Option<String>,
) -> DbResult<()> {
    info!("更新辅助任务状态: task_id={}, status={}", task_id, status);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::TaskId.eq(task_id))
        .one(db)
        .await?
        .ok_or(DbError::not_found("辅助任务"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.as_ref().to_string());
    active_model.error_message = Set(error_message);
    active_model.updated_at = Set(Utc::now());

    active_model.update(db).await?;
    info!("辅助任务状态更新成功");
    Ok(())
}
