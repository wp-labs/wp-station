// 性能测试数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{QueryOrder, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use wp_station_migrations::entity::performance::{ActiveModel, Column, Entity, Model, result};

pub type PerformanceTask = Model;
pub type PerformanceResult = result::Model;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPerformanceTask {
    pub task_id: String,
    pub sample_data: Option<String>,
    pub config_content: Option<String>,
    pub created_by: Option<String>,
}

// ============ 数据库操作函数 ============

/// 创建性能测试任务
pub async fn create_performance_task(task: NewPerformanceTask) -> DbResult<i32> {
    info!("创建性能测试任务: task_id={}", task.task_id);

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let active_model = ActiveModel {
        task_id: Set(task.task_id),
        sample_data: Set(task.sample_data),
        config_content: Set(task.config_content),
        created_by: Set(task.created_by),
        status: Set("running".to_string()),
        start_time: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;

    info!("性能测试任务创建成功: id={}", id);
    Ok(id)
}

/// 根据任务 ID 查找性能测试任务
pub async fn find_performance_task_by_id(task_id: &str) -> DbResult<Option<PerformanceTask>> {
    debug!("查询性能测试任务: task_id={}", task_id);

    let pool = get_pool();
    let db = pool.inner();

    let task = Entity::find()
        .filter(Column::TaskId.eq(task_id))
        .one(db)
        .await?;

    if task.is_some() {
        debug!("找到性能测试任务");
    } else {
        debug!("未找到性能测试任务");
    }

    Ok(task)
}

/// 更新性能测试任务状态
pub async fn update_performance_task_status(task_id: &str, status: &str) -> DbResult<()> {
    info!(
        "更新性能测试任务状态: task_id={}, status={}",
        task_id, status
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::TaskId.eq(task_id))
        .one(db)
        .await?
        .ok_or(DbError::not_found("性能测试任务"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.to_string());

    if status == "completed" || status == "failed" {
        active_model.end_time = Set(Some(Utc::now()));
    }

    active_model.update(db).await?;

    info!("性能测试任务状态更新成功");
    Ok(())
}

/// 添加性能测试结果
pub async fn add_performance_result(
    task_id: i32,
    sink_name: &str,
    lines: i64,
    qps: i32,
    status: &str,
) -> DbResult<i32> {
    info!(
        "添加性能测试结果: task_id={}, sink_name={}, lines={}, qps={}",
        task_id, sink_name, lines, qps
    );

    let pool = get_pool();
    let db = pool.inner();

    let active_model = result::ActiveModel {
        task_id: Set(task_id),
        sink_name: Set(sink_name.to_string()),
        lines: Set(Some(lines)),
        qps: Set(Some(qps)),
        status: Set(Some(status.to_string())),
        ..Default::default()
    };

    let res = result::Entity::insert(active_model).exec(db).await?;
    let id = res.last_insert_id;

    info!("性能测试结果添加成功: id={}", id);
    Ok(id)
}

/// 获取性能测试结果列表
pub async fn get_performance_results(task_id: i32) -> DbResult<Vec<PerformanceResult>> {
    debug!("获取性能测试结果: task_id={}", task_id);

    let pool = get_pool();
    let db = pool.inner();

    let results = result::Entity::find()
        .filter(result::Column::TaskId.eq(task_id))
        .order_by_asc(result::Column::Id)
        .all(db)
        .await?;

    debug!("获取到 {} 个性能测试结果", results.len());
    Ok(results)
}
