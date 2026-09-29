//! 设备健康检查模块。
//!
//! 健康检查入口保持统一，但内部按 `device.system` 分发：
//! - `wparse` -> `WarpParseService`
//! - `wfusion` -> `WfusionService`
//!
//! 最终都回写数据库中的设备运行态信息。

use crate::db::{
    DeviceStatus, find_device_by_id, update_device_health_error, update_device_runtime_state,
    update_device_status,
};
use crate::utils::{SystemKind, WarpParseService, WfusionService};
use chrono::Utc;

/// 设备健康检查结果。
pub struct DeviceHealthCheckResult {
    pub is_online: bool,
    pub error_message: Option<String>,
}

/// 统一设备健康检查入口，并把结果回写到数据库运行态字段。
pub async fn check_device_health(device_id: i32) -> bool {
    check_device_health_with_detail(device_id).await.is_online
}

/// 统一设备健康检查入口，并返回本次探活的离线原因。
pub async fn check_device_health_with_detail(device_id: i32) -> DeviceHealthCheckResult {
    let device = match find_device_by_id(device_id).await {
        Ok(Some(dev)) => dev,
        Ok(None) => {
            warn!("设备不存在: id={}", device_id);
            return DeviceHealthCheckResult {
                is_online: false,
                error_message: Some("设备不存在".to_string()),
            };
        }
        Err(err) => {
            warn!("查询设备失败: id={}, error={}", device_id, err);
            return DeviceHealthCheckResult {
                is_online: false,
                error_message: Some(format!("查询设备失败: {}", err)),
            };
        }
    };

    if device.system == SystemKind::Wfusion.as_ref() {
        let service = match WfusionService::new() {
            Ok(service) => service,
            Err(err) => {
                warn!(
                    "创建 wfusion 客户端失败: device_id={}, error={}",
                    device_id, err
                );
                let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
                let _ = update_device_health_error(device_id, Some(&err.to_string())).await;
                return DeviceHealthCheckResult {
                    is_online: false,
                    error_message: Some(err.to_string()),
                };
            }
        };
        match service.check_health(&device).await {
            Ok(status) => {
                let device_status = if status.is_online {
                    DeviceStatus::Active
                } else {
                    DeviceStatus::Inactive
                };
                let _ = update_device_status(device_id, device_status).await;
                if status.is_online {
                    let _ = update_device_health_error(device_id, None).await;
                    let _ = update_device_runtime_state(
                        device_id,
                        status.client_version.as_deref(),
                        status.config_version.as_deref(),
                        None,
                        Some(Utc::now()),
                    )
                    .await;
                }
                let offline_error = "设备返回 accepting=false".to_string();
                let _ = update_device_health_error(
                    device_id,
                    if status.is_online {
                        None
                    } else {
                        Some(&offline_error)
                    },
                )
                .await;
                return DeviceHealthCheckResult {
                    is_online: status.is_online,
                    error_message: if status.is_online {
                        None
                    } else {
                        Some(offline_error)
                    },
                };
            }
            Err(err) => {
                warn!(
                    "wfusion 健康检查失败: device_id={}, error={}",
                    device_id, err
                );
                let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
                let _ = update_device_health_error(device_id, Some(&err.to_string())).await;
                return DeviceHealthCheckResult {
                    is_online: false,
                    error_message: Some(err.to_string()),
                };
            }
        }
    }

    let service = WarpParseService::default();

    match service.check_online(&device).await {
        Ok(status) => {
            let is_online = status.is_online;

            let device_status = if is_online {
                DeviceStatus::Active
            } else {
                DeviceStatus::Inactive
            };
            let _ = update_device_status(device_id, device_status).await;

            if is_online {
                let _ = update_device_health_error(device_id, None).await;
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

            let offline_error = "设备返回 accepting_commands=false".to_string();
            let _ = update_device_health_error(
                device_id,
                if is_online {
                    None
                } else {
                    Some(&offline_error)
                },
            )
            .await;
            DeviceHealthCheckResult {
                is_online,
                error_message: if is_online { None } else { Some(offline_error) },
            }
        }
        Err(err) => {
            warn!("健康检查失败: device_id={}, error={}", device_id, err);
            let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
            let _ = update_device_health_error(device_id, Some(&err.to_string())).await;
            DeviceHealthCheckResult {
                is_online: false,
                error_message: Some(err.to_string()),
            }
        }
    }
}
