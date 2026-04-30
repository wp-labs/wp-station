use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::{DateTime, Utc};
use sea_orm::{Condition, QueryOrder, QuerySelect, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::release_target::{ActiveModel, Column, Entity, Model};

pub type ReleaseTarget = Model;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
#[allow(non_camel_case_types)]
pub enum ReleaseTargetStatus {
    QUEUED,
    RUNNING,
    SUCCESS,
    FAIL,
    ROLLBACK_PENDING,
    ROLLBACKING,
    ROLLED_BACK,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewReleaseTarget {
    pub release_id: i32,
    pub device_id: i32,
    pub release_group: String,
    pub status: ReleaseTargetStatus,
    pub stage_trace: Option<String>,
    pub remote_job_id: Option<String>,
    pub rollback_job_id: Option<String>,
    pub current_config_version: Option<String>,
    pub target_config_version: String,
    pub client_version: Option<String>,
    pub error_message: Option<String>,
    pub next_poll_at: Option<DateTime<Utc>>,
    pub poll_attempts: i32,
}

#[derive(Debug, Default)]
pub struct ReleaseTargetUpdate {
    pub status: Option<ReleaseTargetStatus>,
    pub stage_trace: Option<Option<String>>,
    pub remote_job_id: Option<Option<String>>,
    pub rollback_job_id: Option<Option<String>>,
    pub current_config_version: Option<Option<String>>,
    pub target_config_version: Option<String>,
    pub client_version: Option<Option<String>>,
    pub error_message: Option<Option<String>>,
    pub next_poll_at: Option<Option<DateTime<Utc>>>,
    pub poll_attempts: Option<i32>,
    pub completed_at: Option<Option<DateTime<Utc>>>,
}

/// 批量创建 release target 记录
pub async fn create_release_targets(targets: Vec<NewReleaseTarget>) -> DbResult<Vec<i32>> {
    info!("创建 release target 记录: count={}", targets.len());
    let pool = get_pool();
    let db = pool.inner();

    let mut ids = Vec::with_capacity(targets.len());
    for target in targets {
        let now = Utc::now();
        let active_model = ActiveModel {
            release_id: Set(target.release_id),
            device_id: Set(target.device_id),
            release_group: Set(target.release_group),
            status: Set(target.status.as_ref().to_string()),
            stage_trace: Set(target.stage_trace),
            remote_job_id: Set(target.remote_job_id),
            rollback_job_id: Set(target.rollback_job_id),
            current_config_version: Set(target.current_config_version),
            target_config_version: Set(target.target_config_version),
            client_version: Set(target.client_version),
            error_message: Set(target.error_message),
            next_poll_at: Set(target.next_poll_at),
            poll_attempts: Set(target.poll_attempts),
            created_at: Set(now),
            updated_at: Set(now),
            completed_at: Set(None),
            ..Default::default()
        };

        let inserted = Entity::insert(active_model).exec(db).await?;
        ids.push(inserted.last_insert_id);
    }

    Ok(ids)
}

/// 根据发布 ID 查询所有 target
pub async fn find_release_targets_by_release(release_id: i32) -> DbResult<Vec<ReleaseTarget>> {
    debug!("查询 release target: release_id={}", release_id);

    let pool = get_pool();
    let db = pool.inner();

    let targets = Entity::find()
        .filter(Column::ReleaseId.eq(release_id))
        .order_by_asc(Column::Id)
        .all(db)
        .await?;

    Ok(targets)
}

/// 查询需要执行/轮询的 target
pub async fn find_due_release_targets(
    now: DateTime<Utc>,
    limit: u64,
) -> DbResult<Vec<ReleaseTarget>> {
    let pool = get_pool();
    let db = pool.inner();

    let status_list = [
        ReleaseTargetStatus::QUEUED,
        ReleaseTargetStatus::RUNNING,
        ReleaseTargetStatus::ROLLBACK_PENDING,
        ReleaseTargetStatus::ROLLBACKING,
    ]
    .into_iter()
    .map(|s| s.as_ref().to_string())
    .collect::<Vec<_>>();

    let targets = Entity::find()
        .filter(
            Condition::any()
                .add(Column::NextPollAt.lte(now))
                .add(Column::NextPollAt.is_null()),
        )
        .filter(Column::Status.is_in(status_list))
        .order_by_asc(Column::NextPollAt)
        .order_by_asc(Column::Id)
        .limit(limit)
        .all(db)
        .await?;

    Ok(targets)
}

/// 更新 target 的状态及相关字段
pub async fn update_release_target(
    id: i32,
    changes: ReleaseTargetUpdate,
) -> DbResult<ReleaseTarget> {
    debug!("更新 release target: id={}", id);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("发布子任务"))?;

    let mut active_model: ActiveModel = model.into();

    if let Some(status) = changes.status {
        active_model.status = Set(status.as_ref().to_string());
    }
    if let Some(stage_trace) = changes.stage_trace {
        active_model.stage_trace = Set(stage_trace);
    }
    if let Some(remote_job_id) = changes.remote_job_id {
        active_model.remote_job_id = Set(remote_job_id);
    }
    if let Some(rollback_job_id) = changes.rollback_job_id {
        active_model.rollback_job_id = Set(rollback_job_id);
    }
    if let Some(current_config_version) = changes.current_config_version {
        active_model.current_config_version = Set(current_config_version);
    }
    if let Some(target_config_version) = changes.target_config_version {
        active_model.target_config_version = Set(target_config_version);
    }
    if let Some(client_version) = changes.client_version {
        active_model.client_version = Set(client_version);
    }
    if let Some(error_message) = changes.error_message {
        active_model.error_message = Set(error_message);
    }
    if let Some(next_poll_at) = changes.next_poll_at {
        active_model.next_poll_at = Set(next_poll_at);
    }
    if let Some(poll_attempts) = changes.poll_attempts {
        active_model.poll_attempts = Set(poll_attempts);
    }
    if let Some(completed_at) = changes.completed_at {
        active_model.completed_at = Set(completed_at);
    }

    active_model.updated_at = Set(Utc::now());

    let updated = active_model.update(db).await?;
    Ok(updated)
}

/// 查找指定设备的上一个成功发布版本
/// 按完成时间倒序查找，返回第一个 SUCCESS 状态的版本号
pub async fn find_device_previous_success_version(
    device_id: i32,
    group: &str,
) -> DbResult<Option<String>> {
    debug!(
        "查找设备上一个成功版本: device_id={}, release_group={}",
        device_id, group
    );

    let pool = get_pool();
    let db = pool.inner();

    let targets = Entity::find()
        .filter(Column::DeviceId.eq(device_id))
        .filter(Column::ReleaseGroup.eq(group))
        .filter(Column::Status.eq(ReleaseTargetStatus::SUCCESS.as_ref()))
        .order_by_desc(Column::CompletedAt)
        .all(db)
        .await?;

    if let Some(target) = targets.first() {
        debug!(
            "找到设备上一个成功版本: device_id={}, version={}",
            device_id, target.target_config_version
        );
        Ok(Some(target.target_config_version.clone()))
    } else {
        debug!(
            "设备没有成功版本记录，将使用 v1.0.0: device_id={}",
            device_id
        );
        Ok(None)
    }
}
