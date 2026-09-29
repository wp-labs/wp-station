//! 设备数据访问层。
//!
//! 只负责设备表 CRUD 和状态字段更新，不处理健康检查或发布调用。
//! 双系统改造后，`system` 字段在这里完成持久化与过滤。

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::{DateTime, Utc};
use sea_orm::{Condition, QueryOrder, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_station_migrations::entity::device::{ActiveModel, Column, Entity, Model};

use crate::utils::SystemKind;

/// 设备实体模型。
pub type Device = Model;

/// 设备状态枚举
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
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
/// 设备在线状态枚举。
pub enum DeviceStatus {
    #[default]
    Unknown,
    Active,
    Inactive,
    Deleted,
}

/// 创建设备时使用的入库模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewDevice {
    /// 设备所属系统，决定健康检查和发布调用方式。
    pub system: SystemKind,
    pub name: Option<String>,
    pub ip: String,
    pub port: i32,
    pub remark: Option<String>,
    pub token: String,
    pub status: Option<DeviceStatus>,
}

/// 部分更新设备配置（支持字段级别的可选更新）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDevice {
    /// 允许在编辑时切换系统归属，后续健康检查和发布均按新 system 生效。
    pub system: Option<SystemKind>,
    pub name: Option<Option<String>>,
    pub ip: Option<String>,
    pub port: Option<i32>,
    pub remark: Option<Option<String>>,
}

// ============ 数据库操作函数 ============

/// 查找所有设备
pub async fn find_all_devices() -> DbResult<Vec<Device>> {
    debug!("查询所有设备");

    let pool = get_pool();
    let db = pool.inner();

    let devices = Entity::find()
        .filter(Column::Status.ne(DeviceStatus::Deleted.as_ref()))
        .order_by_desc(Column::CreatedAt)
        .all(db)
        .await?;

    debug!("查询所有设备成功: count={}", devices.len());
    Ok(devices)
}

/// 按系统查询设备，用于发布弹窗和系统维度的设备管理。
pub async fn find_devices_by_system(system: SystemKind) -> DbResult<Vec<Device>> {
    let pool = get_pool();
    let db = pool.inner();

    Entity::find()
        .filter(Column::System.eq(system.as_ref()))
        .filter(Column::Status.ne(DeviceStatus::Deleted.as_ref()))
        .order_by_desc(Column::CreatedAt)
        .all(db)
        .await
        .map_err(Into::into)
}

/// 分页查询设备列表，支持按关键字搜索（匹配 name/ip/remark）
pub async fn find_devices_page(
    system: Option<SystemKind>,
    keyword: Option<&str>,
    page: i64,
    page_size: i64,
) -> DbResult<(Vec<Device>, i64)> {
    debug!(
        "分页查询设备: keyword={:?}, page={}, page_size={}",
        keyword, page, page_size
    );

    use sea_orm::QuerySelect;

    let pool = get_pool();
    let db = pool.inner();

    let offset = (page - 1) * page_size;

    // 基础条件：过滤已删除的设备
    let mut cond = Condition::all().add(Column::Status.ne(DeviceStatus::Deleted.as_ref()));
    if let Some(system) = system {
        cond = cond.add(Column::System.eq(system.as_ref()));
    }

    // 若提供关键字，则按 name/ip/remark 模糊匹配
    if let Some(kw) = keyword
        && !kw.is_empty()
    {
        let pattern = format!("%{}%", kw);
        cond = cond.add(
            Condition::any()
                .add(Column::Name.like(&pattern))
                .add(Column::Ip.like(&pattern))
                .add(Column::Remark.like(&pattern)),
        );
    }

    let base_query = Entity::find().filter(cond);

    let total = base_query.clone().count(db).await?;

    let items = base_query
        .order_by_desc(Column::CreatedAt)
        .limit(page_size as u64)
        .offset(offset as u64)
        .all(db)
        .await?;

    debug!("分页查询设备成功: count={}, total={}", items.len(), total);
    Ok((items, total as i64))
}

/// 根据 ID 查找设备
pub async fn find_device_by_id(id: i32) -> DbResult<Option<Device>> {
    let pool = get_pool();
    let db = pool.inner();

    let device = Entity::find_by_id(id)
        .filter(Column::Status.ne(DeviceStatus::Deleted.as_ref()))
        .one(db)
        .await?;

    Ok(device)
}

/// 根据 ID 列表批量查询设备
pub async fn find_devices_by_ids(ids: &[i32]) -> DbResult<Vec<Device>> {
    if ids.is_empty() {
        return Ok(vec![]);
    }

    let pool = get_pool();
    let db = pool.inner();

    let devices = Entity::find()
        .filter(Column::Id.is_in(ids.to_vec()))
        .filter(Column::Status.ne(DeviceStatus::Deleted.as_ref()))
        .all(db)
        .await?;

    Ok(devices)
}

/// 创建设备
pub async fn create_device(new_device: NewDevice) -> DbResult<i32> {
    info!("创建设备: ip={}, port={}", new_device.ip, new_device.port);

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let active_model = ActiveModel {
        system: Set(new_device.system.as_ref().to_string()),
        name: Set(new_device.name),
        ip: Set(new_device.ip),
        port: Set(new_device.port),
        remark: Set(new_device.remark),
        token: Set(new_device.token),
        status: Set(new_device.status.unwrap_or_default().as_ref().to_string()),
        client_version: Set(None),
        config_version: Set(None),
        health_error: Set(None),
        last_release_id: Set(None),
        last_seen_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;

    info!("设备创建成功: id={}", id);
    Ok(id)
}

/// 更新设备
pub async fn update_device(id: i32, device: NewDevice) -> DbResult<()> {
    info!(
        "更新设备: id={}, ip={}, port={}",
        id, device.ip, device.port
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("设备"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.system = Set(device.system.as_ref().to_string());
    active_model.name = Set(device.name);
    active_model.ip = Set(device.ip);
    active_model.port = Set(device.port);
    active_model.remark = Set(device.remark);
    active_model.token = Set(device.token);
    active_model.health_error = Set(None);
    active_model.updated_at = Set(Utc::now());

    active_model.update(db).await?;

    info!("设备更新成功: id={}", id);
    Ok(())
}

/// 软删除设备，保留历史记录供发布和审计链路引用。
pub async fn delete_device(id: i32) -> DbResult<()> {
    info!("删除设备: id={}", id);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("设备"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(DeviceStatus::Deleted.as_ref().to_string());
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    info!("设备删除成功: id={}", id);
    Ok(())
}

/// 仅更新设备在线状态字段。
pub async fn update_device_status(id: i32, status: DeviceStatus) -> DbResult<()> {
    info!("更新设备状态: id={}, status={}", id, status.as_ref());

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("设备"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.as_ref().to_string());
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    info!("设备状态更新成功: id={}", id);
    Ok(())
}

/// 更新设备最近一次健康检查错误；在线或刷新成功时传 `None` 清空。
pub async fn update_device_health_error(id: i32, error_message: Option<&str>) -> DbResult<()> {
    debug!(
        "更新设备健康检查错误: id={}, has_error={}",
        id,
        error_message.is_some()
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("设备"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.health_error = Set(error_message.map(|value| value.to_string()));
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 更新设备的运行态信息（客户端版本、配置版本、最近上线时间等）
pub async fn update_device_runtime_state(
    id: i32,
    client_version: Option<&str>,
    config_version: Option<&str>,
    last_release_id: Option<i32>,
    last_seen_at: Option<DateTime<Utc>>,
) -> DbResult<()> {
    debug!(
        "更新设备运行状态: id={}, client_version={:?}, config_version={:?}, last_release_id={:?}",
        id, client_version, config_version, last_release_id
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or(DbError::not_found("设备"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.client_version = Set(client_version.map(|s| s.to_string()));
    active_model.config_version = Set(config_version.map(|s| s.to_string()));
    active_model.last_release_id = Set(last_release_id);
    active_model.last_seen_at = Set(last_seen_at);
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    debug!("设备运行状态更新成功: id={}", id);
    Ok(())
}
