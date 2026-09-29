//! 设备写操作相关业务。

use crate::db::device::NewDevice;
use crate::db::{
    create_device as db_create_device, delete_device as db_delete_device,
    update_device as db_update_device,
};
use crate::error::AppError;
use crate::utils::check_device_health;

use super::{CreateDeviceRequest, DeviceCreated, DeviceUpdateResult, UpdateDeviceRequest};

/// 创建新设备。先入库，再做 3 秒连通性校验；校验失败时保留记录并返回错误。
pub async fn create_device_logic(req: CreateDeviceRequest) -> Result<DeviceCreated, AppError> {
    info!("创建设备: ip={}, port={}", req.ip, req.port);

    async move {
        let new_device = NewDevice {
            system: req.system,
            name: req.name.clone(),
            ip: req.ip.clone(),
            port: req.port,
            remark: req.remark.clone(),
            token: req.token.clone(),
            status: Some(crate::db::DeviceStatus::Unknown),
        };

        let id = db_create_device(new_device)
            .await
            .map_err(AppError::internal)?;
        info!("设备记录创建成功: id={}", id);

        super::health::validate_device_reachable_after_create(id, &req).await?;

        info!("创建设备完成: id={}", id);
        Ok::<_, AppError>(DeviceCreated { id })
    }
    .await
}

/// 更新已有设备配置。
pub async fn update_device_logic(req: UpdateDeviceRequest) -> Result<DeviceUpdateResult, AppError> {
    info!("更新设备: id={}, ip={}, port={}", req.id, req.ip, req.port);

    let device_id = req.id;
    let token = req.token.clone();

    let result = async move {
        let device = NewDevice {
            system: req.system,
            name: req.name,
            ip: req.ip,
            port: req.port,
            remark: req.remark,
            token: req.token,
            status: None,
        };

        db_update_device(req.id, device).await?;
        info!("更新设备成功: id={}", req.id);
        Ok::<_, AppError>(())
    }
    .await;

    if result.is_ok() {
        let is_online = check_device_health(device_id).await;

        let message = if token.is_empty() {
            Some("设备 Token 未配置，无法验证连接".to_string())
        } else if !is_online {
            Some("连接失败，请检查 IP、端口和 Token 是否正确".to_string())
        } else {
            Some("设备连接成功".to_string())
        };

        Ok(DeviceUpdateResult {
            success: true,
            is_online,
            message,
        })
    } else {
        result?;
        unreachable!()
    }
}

/// 删除指定 ID 的设备（软删除）。
pub async fn delete_device_logic(id: i32) -> Result<(), AppError> {
    info!("删除设备: id={}", id);

    async {
        db_delete_device(id).await?;
        info!("删除设备成功: id={}", id);
        Ok::<_, AppError>(())
    }
    .await
}
