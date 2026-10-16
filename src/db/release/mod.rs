//! 发布记录数据访问层。
//!
//! 只负责发布主表的查询与状态更新，不负责设备目标调度或外部发布调用。
//! 双系统改造后，所有发布查询都会围绕 `system` 增加过滤。

pub mod group;
pub mod target;

use std::collections::HashMap;

use crate::constants::release::{GROUP_ALL, GROUP_DRAFT, GROUP_INFRA, GROUP_MODELS};
use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{Condition, DatabaseConnection, QueryOrder, QuerySelect, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::{
    release::{ActiveModel, Column, Entity, Model},
    release_restore_job,
    release_target::{self, Column as TargetColumn, Entity as TargetEntity},
};

pub use self::group::ReleaseGroup;
pub use self::target::{
    NewReleaseTarget, ReleaseTarget, ReleaseTargetStatus, ReleaseTargetUpdate,
    create_release_targets, find_device_previous_success_version, find_due_release_targets,
    find_release_targets_by_release, update_release_target,
};
use crate::utils::SystemKind;

/// 发布记录实体模型。
pub type Release = Model;

/// 发布状态枚举。
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

/// 创建发布记录时使用的入库模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRelease {
    /// 发布所属系统，后续校验、diff、发布执行都依赖该字段分发。
    pub system: SystemKind,
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
    system: Option<SystemKind>,
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

    let restore_target_ids = release_restore_job::Entity::find()
        .select_only()
        .column(release_restore_job::Column::TargetReleaseId)
        .into_tuple::<i32>()
        .all(db)
        .await?;

    // 普通发布列表不展示还原任务的 INIT 技术目标；筛选值统一规范化，
    // 避免大小写或首尾空格差异导致状态明明存在却查不到。
    let normalized_status = status.map(str::trim).filter(|status| !status.is_empty());
    let mut condition = if let Some(status) = normalized_status {
        let status = status.to_ascii_uppercase();
        Condition::all().add(Column::Status.eq(status))
    } else {
        Condition::all().add(Column::Status.ne(ReleaseStatus::INIT.as_ref()))
    };
    if !restore_target_ids.is_empty() {
        condition = condition.add(Column::Id.is_not_in(restore_target_ids));
    }

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

    if let Some(system) = system {
        condition = condition.add(Column::System.eq(system.as_ref()));
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

/// 查询指定系统的全部发布记录，包括 INIT 占位记录。
pub async fn find_releases_by_system(system: SystemKind) -> DbResult<Vec<Release>> {
    let pool = get_pool();
    let db = pool.inner();
    let restore_target_ids = release_restore_job::Entity::find()
        .select_only()
        .column(release_restore_job::Column::TargetReleaseId)
        .into_tuple::<i32>()
        .all(db)
        .await?;
    let mut query = Entity::find().filter(Column::System.eq(system.as_ref()));
    if !restore_target_ids.is_empty() {
        query = query.filter(Column::Id.is_not_in(restore_target_ids));
    }
    Ok(query.order_by_desc(Column::CreatedAt).all(db).await?)
}

/// 查询仍在执行的普通发布记录，供发布调度器恢复全量发布阶段。
pub async fn find_running_release_ids() -> DbResult<Vec<i32>> {
    let pool = get_pool();
    let db = pool.inner();
    let restore_target_ids = release_restore_job::Entity::find()
        .select_only()
        .column(release_restore_job::Column::TargetReleaseId)
        .into_tuple::<i32>()
        .all(db)
        .await?;

    let mut query = Entity::find()
        .select_only()
        .column(Column::Id)
        .filter(Column::Status.eq(ReleaseStatus::RUNNING.as_ref()));
    if !restore_target_ids.is_empty() {
        query = query.filter(Column::Id.is_not_in(restore_target_ids));
    }

    Ok(query.into_tuple::<i32>().all(db).await?)
}

/// 查找当前系统唯一草稿发布记录（WAIT 状态）。
pub async fn find_latest_draft_release(system: SystemKind) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let release = Entity::find()
        .filter(Column::System.eq(system.as_ref()))
        .filter(Column::Status.eq(ReleaseStatus::WAIT.as_ref()))
        .order_by_desc(Column::UpdatedAt)
        .order_by_desc(Column::CreatedAt)
        .one(db)
        .await?;

    Ok(release)
}

/// 将同系统下额外的 WAIT 草稿记录归档为 INIT，确保外部只看到一条草稿。
pub async fn archive_extra_draft_releases(system: SystemKind, keep_id: i32) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let drafts = Entity::find()
        .filter(Column::System.eq(system.as_ref()))
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

/// 创建发布记录，并把 `system` 一起持久化到主表。
pub async fn create_release(release: NewRelease) -> DbResult<i32> {
    info!("创建发布记录: version={}", release.version);

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let status = release.status.unwrap_or_default().as_ref().to_string();
    let active_model = ActiveModel {
        system: Set(release.system.as_ref().to_string()),
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

/// 更新发布记录状态，仅在发布成功时写入发布时间。
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

    let should_set_published_at = status == ReleaseStatus::PASS
        && model.release_group != GROUP_DRAFT
        && model.published_at.is_none();

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

/// 更新发布备注（沿用 `pipeline` 字段）。
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

/// 将指定发布记录刷新为草稿状态，供后续继续编辑和发布。
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

/// 查找最近一次已发布的版本（可排除指定 ID）。
pub async fn find_latest_passed_release(exclude_id: Option<i32>) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();
    let restore_target_ids = release_restore_job::Entity::find()
        .select_only()
        .column(release_restore_job::Column::TargetReleaseId)
        .into_tuple::<i32>()
        .all(db)
        .await?;

    let mut query = Entity::find().filter(Column::Status.eq(ReleaseStatus::PASS.as_ref()));
    if !restore_target_ids.is_empty() {
        query = query.filter(Column::Id.is_not_in(restore_target_ids));
    }
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

/// 查找同系统、同发布组最近一次所有目标都已成功完成的发布记录。
pub async fn find_latest_successful_release_by_group(
    system: SystemKind,
    group: &str,
) -> DbResult<Option<Release>> {
    find_successful_release_by_group(system, group, None).await
}

/// 查找指定发布之前，同系统同发布组最近一次所有目标都已成功完成的记录。
pub async fn find_previous_successful_release_by_group(
    system: SystemKind,
    group: &str,
    before: &Release,
) -> DbResult<Option<Release>> {
    find_successful_release_by_group(system, group, Some(before)).await
}

async fn find_successful_release_by_group(
    system: SystemKind,
    group: &str,
    before: Option<&Release>,
) -> DbResult<Option<Release>> {
    let pool = get_pool();
    let db = pool.inner();

    let group_condition = match group {
        GROUP_MODELS => Condition::any()
            .add(Column::ReleaseGroup.eq(GROUP_MODELS))
            .add(Column::ReleaseGroup.eq(GROUP_ALL)),
        GROUP_INFRA => Condition::any()
            .add(Column::ReleaseGroup.eq(GROUP_INFRA))
            .add(Column::ReleaseGroup.eq(GROUP_ALL)),
        other => Condition::all().add(Column::ReleaseGroup.eq(other)),
    };
    let mut query = Entity::find()
        .filter(Column::System.eq(system.as_ref()))
        .filter(group_condition)
        .filter(Column::Status.is_in([
            ReleaseStatus::PASS.as_ref(),
            ReleaseStatus::PARTIAL_FAIL.as_ref(),
        ]));
    if let Some(before) = before {
        let earlier_than_release = Condition::any()
            .add(Column::CreatedAt.lt(before.created_at))
            .add(
                Condition::all()
                    .add(Column::CreatedAt.eq(before.created_at))
                    .add(Column::Id.lt(before.id)),
            );
        query = query.filter(earlier_than_release);
    }
    // 还原目标记录虽然在普通列表中隐藏，但还原成功后代表当前已提升的仓库版本，
    // 必须作为后续发布和详情 diff 的成功基线参与查询。
    let candidates = query
        .order_by_desc(Column::CreatedAt)
        .order_by_desc(Column::Id);
    let candidates = candidates.all(db).await?;

    for candidate in candidates {
        if release_group_targets_succeeded(&candidate, group, db).await? {
            return Ok(Some(candidate));
        }
    }

    Ok(None)
}

async fn release_group_targets_succeeded(
    release: &Release,
    group: &str,
    db: &DatabaseConnection,
) -> DbResult<bool> {
    let targets = TargetEntity::find()
        .filter(TargetColumn::ReleaseId.eq(release.id))
        .filter(TargetColumn::ReleaseGroup.eq(group))
        .filter(TargetColumn::Operation.eq("publish"))
        .order_by_asc(TargetColumn::AttemptNo)
        .order_by_asc(TargetColumn::Id)
        .all(db)
        .await?;

    let mut latest_by_device: HashMap<i32, release_target::Model> = HashMap::new();
    for target in targets {
        latest_by_device.insert(target.device_id, target);
    }

    if latest_by_device.is_empty() {
        // 旧版成功记录可能没有逐设备 target 明细，保留主记录 PASS 的成功语义。
        return Ok(release.status == ReleaseStatus::PASS.as_ref());
    }

    Ok(latest_by_device
        .values()
        .all(|target: &release_target::Model| {
            target.status == ReleaseTargetStatus::SUCCESS.as_ref()
                || target.status == ReleaseTargetStatus::ROLLED_BACK.as_ref()
        }))
}

impl NewRelease {
    /// 构造一条默认发布记录。
    pub fn new(system: SystemKind, version: String, release_group: ReleaseGroup) -> Self {
        Self {
            system,
            version,
            release_group: release_group.as_ref().to_string(),
            pipeline: None,
            created_by: None,
            stages: None,
            status: None,
        }
    }
}
