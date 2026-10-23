//! 设备创建后的连通性校验。

use std::time::Duration;

use chrono::Utc;

use crate::constants::device::CREATE_DEVICE_CONNECT_TIMEOUT_SECONDS;
use crate::db::device::Device;
use crate::db::{DeviceStatus, update_device_runtime_state, update_device_status};
use crate::error::AppError;
use crate::utils::{SystemKind, WarpParseService, WfusionService};

use super::CreateDeviceRequest;

/// 创建设备后的连通性校验。
///
/// `wparse` / `wfusion` 都走真实探活，但调用的远端状态字段不同。
pub(super) async fn validate_device_reachable_after_create(
    device_id: i32,
    req: &CreateDeviceRequest,
) -> Result<(), AppError> {
    let now = Utc::now();
    let device = Device {
        id: 0,
        system: req.system.as_ref().to_string(),
        name: req.name.clone(),
        ip: req.ip.clone(),
        port: req.port,
        remark: req.remark.clone(),
        status: DeviceStatus::Unknown.as_ref().to_string(),
        token: req.token.clone(),
        client_version: None,
        config_version: None,
        health_error: None,
        last_release_id: None,
        last_seen_at: None,
        created_at: now,
        updated_at: now,
    };

    if matches!(req.system, SystemKind::Wfusion) {
        let service = WfusionService::with_timeout(Duration::from_secs(
            CREATE_DEVICE_CONNECT_TIMEOUT_SECONDS,
        ))
        .map_err(AppError::internal)?;
        let status = service.check_health(&device).await?;
        if status.is_online {
            update_device_status(device_id, DeviceStatus::Active)
                .await
                .map_err(AppError::internal)?;
            update_device_runtime_state(
                device_id,
                status.client_version.as_deref(),
                status.config_version.as_deref(),
                None,
                Some(now),
            )
            .await
            .map_err(AppError::internal)?;
            return Ok(());
        }
        let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
        return Err(AppError::validation("wfusion 设备不可达"));
    }

    let service =
        WarpParseService::with_timeout(Duration::from_secs(CREATE_DEVICE_CONNECT_TIMEOUT_SECONDS))
            .map_err(AppError::internal)?;

    match service.check_online(&device).await {
        Ok(status) if status.is_online => {
            update_device_status(device_id, DeviceStatus::Active)
                .await
                .map_err(AppError::internal)?;
            update_device_runtime_state(
                device_id,
                status.client_version.as_deref(),
                status.config_version.as_deref(),
                None,
                Some(now),
            )
            .await
            .map_err(AppError::internal)?;
            info!("新增设备连接验证成功: ip={}, port={}", req.ip, req.port);
            Ok(())
        }
        Ok(_) => {
            let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
            warn!(
                "新增设备连接验证失败: ip={}, port={}, reason=设备离线",
                req.ip, req.port
            );
            Err(AppError::validation(
                "无法连接设备，请检查 IP、端口和 Token 是否正确",
            ))
        }
        Err(err) => {
            let _ = update_device_status(device_id, DeviceStatus::Inactive).await;
            warn!(
                "新增设备连接验证失败: ip={}, port={}, error={}",
                req.ip, req.port, err
            );
            Err(AppError::validation(format!(
                "无法连接设备，请检查 IP、端口和 Token 是否正确（3 秒超时）: {}",
                err
            )))
        }
    }
}
