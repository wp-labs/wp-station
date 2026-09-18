//! 设备查询相关业务。

use crate::db::device::Device;
use crate::db::{DeviceStatus, find_device_by_id, find_devices_by_system, find_devices_page};
use crate::error::AppError;
use crate::utils::SystemKind;
use crate::utils::check_device_health_with_detail;
use crate::utils::pagination::PageQuery;

use super::{DeviceListResponse, DeviceRefreshResult};

/// 获取设备列表（支持关键字搜索 + 分页）。
pub async fn list_devices_logic(
    system: Option<SystemKind>,
    keyword: Option<String>,
    page: PageQuery,
) -> Result<DeviceListResponse, AppError> {
    debug!("获取设备列表: keyword={:?}", keyword);

    let (page, page_size) = page.normalize_default();
    let (items, total) = find_devices_page(system, keyword.as_deref(), page, page_size).await?;

    debug!(
        "获取设备列表成功: 共 {} 条, page={}, page_size={}",
        total, page, page_size
    );

    Ok(DeviceListResponse::from_db(items, total, page, page_size))
}

/// 获取在线设备列表（`status == active`，供发布弹窗使用）。
pub async fn list_online_devices_logic(system: SystemKind) -> Result<Vec<Device>, AppError> {
    debug!("获取在线设备列表");

    let all = find_devices_by_system(system).await?;
    let online: Vec<Device> = all
        .into_iter()
        .filter(|device| device.status == DeviceStatus::Active.as_ref())
        .collect();

    debug!("获取在线设备列表成功: count={}", online.len());
    Ok(online)
}

/// 手动刷新设备在线状态。
///
/// 统一复用 utils 层健康检查入口，再返回最新设备记录。
pub async fn refresh_device_status_logic(device_id: i32) -> Result<DeviceRefreshResult, AppError> {
    info!("手动刷新设备状态: id={}", device_id);

    let existing = find_device_by_id(device_id)
        .await?
        .ok_or_else(|| AppError::NotFound("设备不存在".to_string()))?;

    let health = check_device_health_with_detail(device_id).await;

    let refreshed = find_device_by_id(device_id).await?.unwrap_or(existing);
    Ok(DeviceRefreshResult {
        device: refreshed,
        health_error: health.error_message,
    })
}
