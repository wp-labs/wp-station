// 健康检查工具 - 使用 WarpParseService 检测设备在线状态

use crate::db::{
    DeviceStatus, find_device_by_id, update_device_runtime_state, update_device_status,
};
use crate::server::Setting;
use crate::utils::WarpParseService;
use chrono::Utc;

/// 检查设备健康状态
pub async fn check_device_health(device_id: i32) -> bool {
    // 查询设备信息
    let device = match find_device_by_id(device_id).await {
        Ok(Some(dev)) => dev,
        Ok(None) => {
            warn!("设备不存在: id={}", device_id);
            return false;
        }
        Err(err) => {
            warn!("查询设备失败: id={}, error={}", device_id, err);
            return false;
        }
    };

    let setting = Setting::load();
    let service = WarpParseService::default();

    // 检查在线状态
    match service.check_online(&device, &setting.warparse).await {
        Ok(status) => {
            let is_online = status.is_online;

            // 更新设备状态
            let device_status = if is_online {
                DeviceStatus::Active
            } else {
                DeviceStatus::Inactive
            };
            let _ = update_device_status(device_id, device_status).await;

            // 如果在线，更新运行时状态
            if is_online {
                let _ = update_device_runtime_state(
                    device_id,
                    status.client_version.as_deref(),
                    status.config_version.as_deref(),
                    None,
                    Some(Utc::now()),
                )
                .await;

                info!(
                    "设备在线: id={}, version={:?}, config_version={:?}",
                    device_id, status.client_version, status.config_version
                );
            } else {
                info!("设备离线: id={}", device_id);
            }

            is_online
        }
        Err(err) => {
            warn!("健康检查失败: device_id={}, error={}", device_id, err);
            let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
            false
        }
    }
}
