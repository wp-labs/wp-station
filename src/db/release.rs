// 发布记录数据库操作 - 纯函数式

use crate::db::{ReleaseGroup, get_pool};
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
    pub release_group: String,
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
        .order_by_desc(Column::UpdatedAt)
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

/// 查找当前唯一草稿发布记录（WAIT 状态）。
pub async fn find_latest_draft_release() -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let release = Entity::find()
        .filter(Column::Status.eq(ReleaseStatus::WAIT.as_ref()))
        .order_by_desc(Column::UpdatedAt)
        .order_by_desc(Column::CreatedAt)
        .one(db)
        .await?;

    Ok(release)
}

/// 将额外的 WAIT 草稿记录归档为 INIT，确保外部只看到一条草稿。
pub async fn archive_extra_draft_releases(keep_id: i32) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let drafts = Entity::find()
        .filter(Column::Status.eq(ReleaseStatus::WAIT.as_ref()))
        .filter(Column::Id.ne(keep_id))
        .all(db)
        .await?;

    for draft in drafts {
        let mut active_model: ActiveModel = draft.into();
        active_model.status = Set(ReleaseStatus::INIT.as_ref().to_string());
        active_model.updated_at = Set(Utc::now());
        active_model.update(db).await?;
    }

    Ok(())
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
        release_group: Set(release.release_group),
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

    let should_set_published_at = matches!(
        status,
        ReleaseStatus::PASS | ReleaseStatus::FAIL | ReleaseStatus::PARTIAL_FAIL
    ) && model.release_group != "draft";

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.as_ref().to_string());
    active_model.error_message = Set(error_message.map(|s| s.to_string()));
    if let Some(stages) = stages {
        active_model.stages = Set(Some(stages.to_string()));
    }
    active_model.updated_at = Set(Utc::now());

    if should_set_published_at {
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

/// 更新发布记录的发布范围聚合结果。
pub async fn update_release_group(id: i32, release_group: &str) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布记录"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.release_group = Set(release_group.to_string());
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 将指定发布记录刷新为草稿状态。
pub async fn touch_release_as_draft(
    id: i32,
    version: &str,
    release_group: &str,
    stages: Option<&str>,
) -> DbResult<Release> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布记录"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.version = Set(version.to_string());
    active_model.release_group = Set(release_group.to_string());
    active_model.pipeline = Set(None);
    active_model.status = Set(ReleaseStatus::WAIT.as_ref().to_string());
    active_model.error_message = Set(None);
    active_model.published_at = Set(None);
    if let Some(stages) = stages {
        active_model.stages = Set(Some(stages.to_string()));
    }
    active_model.updated_at = Set(Utc::now());

    active_model.update(db).await.map_err(Into::into)
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

/// 查找同发布组最近一次已发布的版本（可排除指定 ID）
pub async fn find_latest_passed_release_by_group(
    group: &str,
    exclude_id: Option<i32>,
) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let group_condition = match group {
        "models" => Condition::any()
            .add(Column::ReleaseGroup.eq("models"))
            .add(Column::ReleaseGroup.eq("all")),
        "infra" => Condition::any()
            .add(Column::ReleaseGroup.eq("infra"))
            .add(Column::ReleaseGroup.eq("all")),
        other => Condition::all().add(Column::ReleaseGroup.eq(other)),
    };

    let mut query = Entity::find()
        .filter(group_condition)
        .filter(Column::Status.is_in([
            ReleaseStatus::PASS.as_ref(),
            ReleaseStatus::PARTIAL_FAIL.as_ref(),
            ReleaseStatus::FAIL.as_ref(),
        ]));
    if let Some(id) = exclude_id {
        query = query.filter(Column::Id.ne(id));
    }

    let release = query
        .order_by_desc(Column::PublishedAt)
        .order_by_desc(Column::UpdatedAt)
        .order_by_desc(Column::CreatedAt)
        .one(db)
        .await?;

    Ok(release)
}

impl NewRelease {
    pub fn new(version: String, release_group: ReleaseGroup) -> Self {
        Self {
            version,
            release_group: release_group.as_ref().to_string(),
            pipeline: None,
            created_by: None,
            stages: None,
            status: None,
        }
    }
}
