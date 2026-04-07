// 发布记录数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{Condition, QueryOrder, QuerySelect, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::release::{ActiveModel, Column, Entity, Model};

pub type Release = Model;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Display,
    EnumString,
    AsRefStr,
    Default,
)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
#[allow(non_camel_case_types)]
pub enum ReleaseStatus {
    #[default]
    WAIT,
    PASS,
    FAIL,
    INIT,
    RUNNING,
    PARTIAL_FAIL,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRelease {
    pub version: String,
    pub pipeline: Option<String>,
    pub created_by: Option<String>,
    pub stages: Option<String>,
    pub status: Option<ReleaseStatus>,
}

// ============ 数据库操作函数 ============

/// 查找所有发布记录（分页）
pub async fn find_all_releases(
    page: i64,
    page_size: i64,
    pipeline: Option<&str>,
    version: Option<&str>,
    created_by: Option<&str>,
    status: Option<&str>,
) -> DbResult<(Vec<Release>, i64)> {
    debug!("查询发布记录: page={}, page_size={}", page, page_size);

    let pool = get_pool();
    let db = pool.inner();

    let offset = (page - 1) * page_size;

    // 状态过滤：有显式 status 时按 status 等值过滤；否则默认排除 INIT
    let mut condition = if let Some(status) = status {
        if !status.is_empty() {
            Condition::all().add(Column::Status.eq(status))
        } else {
            Condition::all().add(Column::Status.ne(ReleaseStatus::INIT.as_ref()))
        }
    } else {
        Condition::all().add(Column::Status.ne(ReleaseStatus::INIT.as_ref()))
    };

    if let Some(pipeline) = pipeline
        && !pipeline.is_empty()
    {
        condition = condition.add(Column::Pipeline.contains(pipeline));
    }

    if let Some(version) = version
        && !version.is_empty()
    {
        condition = condition.add(Column::Version.contains(version));
    }

    if let Some(created_by) = created_by
        && !created_by.is_empty()
    {
        condition = condition.add(Column::CreatedBy.contains(created_by));
    }

    let base_query = Entity::find().filter(condition);

    let releases = base_query
        .clone()
        .order_by_desc(Column::CreatedAt)
        .limit(page_size as u64)
        .offset(offset as u64)
        .all(db)
        .await?;

    let total = base_query.count(db).await?;

    debug!(
        "查询发布记录成功: count={}, total={}",
        releases.len(),
        total
    );
    Ok((releases, total as i64))
}

/// 根据 ID 查找发布记录
pub async fn find_release_by_id(id: i32) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let release = Entity::find_by_id(id).one(db).await?;

    Ok(release)
}

/// 创建发布记录
pub async fn create_release(release: NewRelease) -> DbResult<i32> {
    info!("创建发布记录: version={}", release.version);

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let status = release.status.unwrap_or_default().as_ref().to_string();
    let active_model = ActiveModel {
        version: Set(release.version),
        pipeline: Set(release.pipeline),
        created_by: Set(release.created_by),
        stages: Set(release.stages),
        status: Set(status),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;

    info!("发布记录创建成功: id={}", id);
    Ok(id)
}

/// 创建初始化发布记录（仅保留 user_name）
pub async fn init_release(user_name: &str) -> DbResult<i32> {
    info!("初始化发布记录: user={}", user_name);

    let draft = NewRelease {
        version: "V1.0.0".to_string(),
        pipeline: Some("草稿".to_string()),
        created_by: Some(user_name.to_string()),
        stages: Some("-".to_string()),
        status: Some(ReleaseStatus::WAIT),
    };

    let id = create_release(draft).await?;
    info!("初始化发布记录完成: id={}", id);
    Ok(id)
}

/// 更新发布记录状态
pub async fn update_release_status(
    id: i32,
    status: ReleaseStatus,
    error_message: Option<&str>,
    stages: Option<&str>,
) -> DbResult<()> {
    info!("更新发布记录状态: id={}, status={}", id, status.as_ref());

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布记录"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.as_ref().to_string());
    active_model.error_message = Set(error_message.map(|s| s.to_string()));
    if let Some(stages) = stages {
        active_model.stages = Set(Some(stages.to_string()));
    }
    active_model.updated_at = Set(Utc::now());

    if status == ReleaseStatus::PASS {
        active_model.published_at = Set(Some(Utc::now()));
    }

    active_model.update(db).await?;

    info!("发布记录状态更新成功");
    Ok(())
}

/// 更新发布备注（沿用 pipeline 字段）
pub async fn update_release_pipeline(id: i32, pipeline: Option<&str>) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布记录"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.pipeline = Set(pipeline.map(|value| value.to_string()));
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 查找草稿发布记录（状态为 WAIT）
pub async fn find_draft_release() -> DbResult<Option<Release>> {
    debug!("查找草稿发布记录");

    let pool = get_pool();
    let db = pool.inner();

    let release = Entity::find()
        .filter(Column::Status.eq(ReleaseStatus::WAIT.as_ref()))
        .order_by_desc(Column::UpdatedAt)
        .one(db)
        .await?;

    if release.is_some() {
        debug!("找到草稿发布记录");
    } else {
        debug!("未找到草稿发布记录");
    }

    Ok(release)
}

/// 更新发布记录的时间戳
pub async fn update_release_timestamp(id: i32) -> DbResult<()> {
    info!("更新发布记录时间戳: id={}", id);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布记录"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    info!("发布记录时间戳更新成功");
    Ok(())
}

/// 查找最近一次已发布的版本（可排除指定 ID）
pub async fn find_latest_passed_release(exclude_id: Option<i32>) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let mut query = Entity::find().filter(Column::Status.eq(ReleaseStatus::PASS.as_ref()));
    if let Some(id) = exclude_id {
        query = query.filter(Column::Id.ne(id));
    }

    let release = query
        .order_by_desc(Column::PublishedAt)
        .order_by_desc(Column::UpdatedAt)
        .one(db)
        .await?;

    Ok(release)
}
