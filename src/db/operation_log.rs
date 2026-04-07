// 操作日志数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::DbResult;
use chrono::{DateTime, Utc};
use sea_orm::{Condition, EntityTrait, QueryOrder, QuerySelect, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use wp_station_migrations::entity::operation_log::{ActiveModel, Column, Entity, Model};

pub type OperationLog = Model;

/// 写入操作日志的输入结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewOperationLog {
    /// 操作人用户名
    pub operator: String,
    /// 操作类型: create / update / delete / publish
    pub operation: String,
    /// 操作对象描述
    pub target: Option<String>,
    /// 操作描述（页面展示）
    pub description: Option<String>,
    /// 操作详细内容（不在页面展示，供审计用）
    pub content: Option<String>,
    /// 状态: success / error
    pub status: String,
}

/// 分页查询操作日志，支持按操作人、操作类型、时间范围筛选
pub async fn find_logs_page(
    operator: Option<&str>,
    operation: Option<&str>,
    start_date: Option<DateTime<Utc>>,
    end_date: Option<DateTime<Utc>>,
    page: i64,
    page_size: i64,
) -> DbResult<(Vec<OperationLog>, i64)> {
    debug!(
        "分页查询操作日志: operator={:?}, operation={:?}, page={}, page_size={}",
        operator, operation, page, page_size
    );

    let pool = get_pool();
    let db = pool.inner();

    let offset = (page - 1) * page_size;
    let mut cond = Condition::all();

    if let Some(op) = operator
        && !op.is_empty()
    {
        let pattern = format!("%{}%", op);
        cond = cond.add(Column::Operator.like(&pattern));
    }

    if let Some(op_type) = operation
        && !op_type.is_empty()
    {
        cond = cond.add(Column::Operation.eq(op_type));
    }

    if let Some(start) = start_date {
        cond = cond.add(Column::UpdatedAt.gte(start));
    }

    if let Some(end) = end_date {
        cond = cond.add(Column::UpdatedAt.lte(end));
    }

    let base_query = Entity::find().filter(cond);
    let total = base_query.clone().count(db).await?;
    let items = base_query
        .order_by_desc(Column::UpdatedAt)
        .limit(page_size as u64)
        .offset(offset as u64)
        .all(db)
        .await?;

    debug!(
        "分页查询操作日志成功: count={}, total={}",
        items.len(),
        total
    );
    Ok((items, total as i64))
}

/// 写入一条操作日志。
/// 该函数只负责持久化，不承担业务层审计语义拼装。
pub async fn create_operation_log(input: NewOperationLog) -> DbResult<OperationLog> {
    let pool = get_pool();
    let db = pool.inner();

    let active_model = ActiveModel {
        operator: Set(input.operator),
        operation: Set(input.operation),
        target: Set(input.target),
        description: Set(input.description),
        content: Set(input.content),
        status: Set(input.status),
        updated_at: Set(Utc::now()),
        ..Default::default()
    };

    let model = Entity::insert(active_model).exec_with_returning(db).await?;

    Ok(model)
}
