//! 发布还原任务数据访问层。

use chrono::{Duration, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::{release, release_restore_job};

use crate::db::get_pool;
use crate::error::DbResult;
use crate::utils::SystemKind;

pub type ReleaseRestoreJob = release_restore_job::Model;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum RestoreJobStatus {
    Queued,
    Running,
    Pass,
    Fail,
    PartialFail,
    RollbackFailed,
    Cancelled,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum RestorePhase {
    Queued,
    Preparing,
    PrepareFailed,
    TagReady,
    ModelsRunning,
    ModelsSuccess,
    ModelsFailed,
    InfraRunning,
    InfraSuccess,
    InfraFailed,
    RollbackRunning,
    RollbackFailed,
    Promoting,
    PromotePending,
    Completed,
}

pub struct NewRestoreJob {
    pub system: SystemKind,
    pub source_release_id: i32,
    pub source_version: String,
    /// 对用户展示的还原发布版本，保持与来源版本一致。
    pub release_version: String,
    /// 仅用于任务表唯一性和内部编排，不作为业务版本展示。
    pub target_version: String,
    pub release_group: String,
    pub selected_device_ids: Vec<i32>,
    pub note: Option<String>,
    pub created_by: Option<String>,
}

/// 在同一事务中创建 INIT 目标发布记录和还原任务。
pub async fn create_restore_job(input: NewRestoreJob) -> DbResult<ReleaseRestoreJob> {
    let pool = get_pool();
    let db = pool.inner();
    let now = Utc::now();
    let selected_device_ids = serde_json::to_string(&input.selected_device_ids)
        .map_err(|error| sea_orm::DbErr::Custom(error.to_string()))?;

    let job = db
        .transaction::<_, ReleaseRestoreJob, sea_orm::DbErr>(|txn| {
            Box::pin(async move {
                let target = release::ActiveModel {
                    system: Set(input.system.as_ref().to_string()),
                    version: Set(input.release_version),
                    release_group: Set(input.release_group.clone()),
                    status: Set("INIT".to_string()),
                    pipeline: Set(input.note),
                    created_by: Set(input.created_by),
                    stages: Set(None),
                    error_message: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    published_at: Set(None),
                    ..Default::default()
                }
                .insert(txn)
                .await?;

                release_restore_job::ActiveModel {
                    system: Set(input.system.as_ref().to_string()),
                    source_release_id: Set(input.source_release_id),
                    target_release_id: Set(target.id),
                    source_version: Set(input.source_version),
                    target_version: Set(input.target_version),
                    status: Set(RestoreJobStatus::Queued.as_ref().to_string()),
                    phase: Set(RestorePhase::Queued.as_ref().to_string()),
                    selected_device_ids: Set(selected_device_ids),
                    models_previous_head: Set(None),
                    infra_previous_head: Set(None),
                    models_candidate_commit: Set(None),
                    infra_candidate_commit: Set(None),
                    models_promoted: Set(false),
                    infra_promoted: Set(false),
                    error_code: Set(None),
                    error_message: Set(None),
                    lock_owner: Set(None),
                    lock_expires_at: Set(None),
                    started_at: Set(None),
                    completed_at: Set(None),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(txn)
                .await
            })
        })
        .await
        .map_err(|error| match error {
            sea_orm::TransactionError::Connection(error)
            | sea_orm::TransactionError::Transaction(error) => error,
        })?;

    Ok(job)
}

pub async fn find_restore_job_by_id(id: i32) -> DbResult<Option<ReleaseRestoreJob>> {
    Ok(release_restore_job::Entity::find_by_id(id)
        .one(get_pool().inner())
        .await?)
}

pub async fn find_restore_job_by_target_release(
    target_release_id: i32,
) -> DbResult<Option<ReleaseRestoreJob>> {
    Ok(release_restore_job::Entity::find()
        .filter(release_restore_job::Column::TargetReleaseId.eq(target_release_id))
        .one(get_pool().inner())
        .await?)
}

pub async fn find_runnable_restore_jobs() -> DbResult<Vec<ReleaseRestoreJob>> {
    Ok(release_restore_job::Entity::find()
        .filter(release_restore_job::Column::Status.is_in([
            RestoreJobStatus::Queued.as_ref(),
            RestoreJobStatus::Running.as_ref(),
        ]))
        .order_by_asc(release_restore_job::Column::CreatedAt)
        .all(get_pool().inner())
        .await?)
}

/// 抢占一个还原任务的短租约，避免多个 Station 实例同时推进同一任务。
pub async fn claim_restore_job(
    id: i32,
    owner: &str,
    lease_seconds: i64,
) -> DbResult<Option<ReleaseRestoreJob>> {
    let now = Utc::now();
    let lease_expires_at = now + Duration::seconds(lease_seconds.max(1));
    let result = release_restore_job::Entity::update_many()
        .filter(release_restore_job::Column::Id.eq(id))
        .filter(release_restore_job::Column::Status.is_in([
            RestoreJobStatus::Queued.as_ref(),
            RestoreJobStatus::Running.as_ref(),
        ]))
        .filter(
            Condition::any()
                .add(release_restore_job::Column::LockExpiresAt.is_null())
                .add(release_restore_job::Column::LockExpiresAt.lt(now)),
        )
        .col_expr(
            release_restore_job::Column::LockOwner,
            Expr::value(owner.to_string()),
        )
        .col_expr(
            release_restore_job::Column::LockExpiresAt,
            Expr::value(lease_expires_at),
        )
        .col_expr(release_restore_job::Column::UpdatedAt, Expr::value(now))
        .exec(get_pool().inner())
        .await?;
    if result.rows_affected == 0 {
        return Ok(None);
    }
    Ok(release_restore_job::Entity::find_by_id(id)
        .one(get_pool().inner())
        .await?)
}

/// 释放还原任务租约；任务状态已经持久化，异常时下一轮可重新抢占。
pub async fn release_restore_job_lock(id: i32, owner: &str) -> DbResult<()> {
    release_restore_job::Entity::update_many()
        .filter(release_restore_job::Column::Id.eq(id))
        .filter(release_restore_job::Column::LockOwner.eq(owner))
        .col_expr(
            release_restore_job::Column::LockOwner,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            release_restore_job::Column::LockExpiresAt,
            Expr::value(Option::<chrono::DateTime<Utc>>::None),
        )
        .col_expr(
            release_restore_job::Column::UpdatedAt,
            Expr::value(Utc::now()),
        )
        .exec(get_pool().inner())
        .await?;
    Ok(())
}

pub async fn find_restore_jobs_by_source(
    source_release_id: i32,
) -> DbResult<Vec<ReleaseRestoreJob>> {
    Ok(release_restore_job::Entity::find()
        .filter(release_restore_job::Column::SourceReleaseId.eq(source_release_id))
        .order_by_desc(release_restore_job::Column::CreatedAt)
        .all(get_pool().inner())
        .await?)
}

pub async fn find_active_restore_job(system: SystemKind) -> DbResult<Option<ReleaseRestoreJob>> {
    Ok(release_restore_job::Entity::find()
        .filter(release_restore_job::Column::System.eq(system.as_ref()))
        .filter(release_restore_job::Column::Status.is_in([
            RestoreJobStatus::Queued.as_ref(),
            RestoreJobStatus::Running.as_ref(),
        ]))
        .order_by_desc(release_restore_job::Column::CreatedAt)
        .one(get_pool().inner())
        .await?)
}

#[derive(Default)]
pub struct RestoreJobUpdate<'a> {
    pub status: Option<RestoreJobStatus>,
    pub phase: Option<RestorePhase>,
    pub models_previous_head: Option<&'a str>,
    pub infra_previous_head: Option<&'a str>,
    pub models_candidate_commit: Option<&'a str>,
    pub infra_candidate_commit: Option<&'a str>,
    pub models_promoted: Option<bool>,
    pub infra_promoted: Option<bool>,
    pub error_code: Option<Option<&'a str>>,
    pub error_message: Option<Option<&'a str>>,
    pub mark_started: bool,
    pub mark_completed: bool,
}

pub async fn update_restore_job(
    id: i32,
    changes: RestoreJobUpdate<'_>,
) -> DbResult<ReleaseRestoreJob> {
    let pool = get_pool();
    let db = pool.inner();
    let model = release_restore_job::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| crate::error::DbError::not_found("还原任务"))?;
    let mut active: release_restore_job::ActiveModel = model.into();
    if let Some(status) = changes.status {
        active.status = Set(status.as_ref().to_string());
    }
    if let Some(phase) = changes.phase {
        active.phase = Set(phase.as_ref().to_string());
    }
    if let Some(value) = changes.models_previous_head {
        active.models_previous_head = Set(Some(value.to_string()));
    }
    if let Some(value) = changes.infra_previous_head {
        active.infra_previous_head = Set(Some(value.to_string()));
    }
    if let Some(value) = changes.models_candidate_commit {
        active.models_candidate_commit = Set(Some(value.to_string()));
    }
    if let Some(value) = changes.infra_candidate_commit {
        active.infra_candidate_commit = Set(Some(value.to_string()));
    }
    if let Some(value) = changes.models_promoted {
        active.models_promoted = Set(value);
    }
    if let Some(value) = changes.infra_promoted {
        active.infra_promoted = Set(value);
    }
    if let Some(value) = changes.error_code {
        active.error_code = Set(value.map(str::to_string));
    }
    if let Some(value) = changes.error_message {
        active.error_message = Set(value.map(str::to_string));
    }
    let now = Utc::now();
    if changes.mark_started {
        active.started_at = Set(Some(now));
    }
    if changes.mark_completed {
        active.completed_at = Set(Some(now));
    }
    active.updated_at = Set(now);
    Ok(active.update(db).await?)
}
