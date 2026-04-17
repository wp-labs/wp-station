// 设备管理业务逻辑层

use crate::db::device::{Device, NewDevice};
use crate::db::{
    DeviceStatus, create_device as db_create_device, delete_device as db_delete_device,
    find_all_devices, find_device_by_id, find_devices_page, update_device as db_update_device,
    update_device_runtime_state, update_device_status,
};
use crate::error::AppError;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::WarpParseService;
use crate::utils::check_device_health;
use crate::utils::pagination::{PageQuery, PageResponse};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct DeviceListQuery {
    /// 关键字，匹配设备名 / IP / 备注
    pub keyword: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Deserialize, Serialize)]
pub struct CreateDeviceRequest {
    /// 设备展示名；为空时回退为 IP
    pub name: Option<String>,
    pub ip: String,
    pub port: i32,
    /// 设备访问令牌，仅用于连接，不写入操作日志
    pub token: String,
    pub remark: Option<String>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateDeviceRequest {
    pub id: i32,
    /// 设备展示名；为空时回退为 IP
    pub name: Option<String>,
    pub ip: String,
    pub port: i32,
    /// 设备访问令牌，仅用于连接，不写入操作日志
    pub token: String,
    pub remark: Option<String>,
}

// ============ 响应结构体 ============

pub type DeviceListResponse = PageResponse<Device>;

#[derive(Serialize)]
pub struct DeviceCreated {
    pub id: i32,
}

#[derive(Serialize)]
pub struct DeviceUpdateResult {
    pub success: bool,
    pub is_online: bool,
    pub message: Option<String>,
}

const CREATE_DEVICE_CONNECT_TIMEOUT: Duration = Duration::from_secs(3);

// ============ 业务逻辑函数 ============

/// 获取设备列表（支持关键字搜索 + 分页）
pub async fn list_devices_logic(
    keyword: Option<String>,
    page: PageQuery,
) -> Result<DeviceListResponse, AppError> {
    debug!("获取设备列表: keyword={:?}", keyword);

    let (page, page_size) = page.normalize_default();

    let (items, total) = find_devices_page(keyword.as_deref(), page, page_size).await?;

    debug!(
        "获取设备列表成功: 共 {} 条, page={}, page_size={}",
        total, page, page_size
    );

    Ok(DeviceListResponse::from_db(items, total, page, page_size))
}

/// 获取在线设备列表（status == active，供发布弹窗使用）
pub async fn list_online_devices_logic() -> Result<Vec<Device>, AppError> {
    debug!("获取在线设备列表");

    let all = find_all_devices().await?;
    let online: Vec<Device> = all
        .into_iter()
        .filter(|device| device.status == DeviceStatus::Active.as_ref())
        .collect();

    debug!("获取在线设备列表成功: count={}", online.len());
    Ok(online)
}

/// 创建新设备。先入库，再做 3 秒连通性校验；校验失败时保留记录并返回错误。
pub async fn create_device_logic(req: CreateDeviceRequest) -> Result<DeviceCreated, AppError> {
    info!("创建设备: ip={}, port={}", req.ip, req.port);

    let device_name = req.name.clone().unwrap_or_else(|| req.ip.clone());
    let ip = req.ip.clone();
    let port = req.port;
    let remark = req.remark.clone();

    let result = async move {
        let new_device = NewDevice {
            name: req.name.clone(),
            ip: req.ip.clone(),
            port: req.port,
            remark: req.remark.clone(),
            token: req.token.clone(),
            status: Some(DeviceStatus::Unknown),
        };

        let id = db_create_device(new_device)
            .await
            .map_err(AppError::internal)?;
        info!("设备记录创建成功: id={}", id);

        validate_device_reachable_after_create(id, &req).await?;

        info!("创建设备完成: id={}", id);
        Ok::<_, AppError>(DeviceCreated { id })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::Device,
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name(device_name)
            .with_field("ip", ip)
            .with_field("port", port.to_string())
            .with_field("remark", remark.unwrap_or_else(|| "-".to_string())),
        &result,
    )
    .await;

    result
}

async fn validate_device_reachable_after_create(
    device_id: i32,
    req: &CreateDeviceRequest,
) -> Result<(), AppError> {
    let setting = Setting::load();
    let service = WarpParseService::with_timeout(CREATE_DEVICE_CONNECT_TIMEOUT)
        .map_err(AppError::internal)?;
    let now = Utc::now();
    let device = Device {
        id: 0,
        name: req.name.clone(),
        ip: req.ip.clone(),
        port: req.port,
        remark: req.remark.clone(),
        status: DeviceStatus::Unknown.as_ref().to_string(),
        token: req.token.clone(),
        client_version: None,
        config_version: None,
        last_release_id: None,
        last_seen_at: None,
        created_at: now,
        updated_at: now,
    };

    match service.check_online(&device, &setting.warparse).await {
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

/// 更新已有设备配置
pub async fn update_device_logic(req: UpdateDeviceRequest) -> Result<DeviceUpdateResult, AppError> {
    info!("更新设备: id={}, ip={}, port={}", req.id, req.ip, req.port);

    let device_id = req.id;
    let device_name = req.name.clone().unwrap_or_else(|| req.ip.clone());
    let ip = req.ip.clone();
    let port = req.port;
    let remark = req.remark.clone();
    let token = req.token.clone();

    let result = async move {
        let device = NewDevice {
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

    write_operation_log_for_result(
        OperationLogBiz::Device,
        OperationLogAction::Update,
        OperationLogParams::new()
            .with_target_name(device_name)
            .with_target_id(device_id.to_string())
            .with_field("ip", ip)
            .with_field("port", port.to_string())
            .with_field("remark", remark.unwrap_or_else(|| "-".to_string())),
        &result,
    )
    .await;

    // 如果更新成功，立即执行健康检查
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

/// 手动刷新设备在线状态
pub async fn refresh_device_status_logic(device_id: i32) -> Result<Device, AppError> {
    info!("手动刷新设备状态: id={}", device_id);

    // 确认设备存在
    let existing = find_device_by_id(device_id)
        .await?
        .ok_or_else(|| AppError::NotFound("设备不存在".to_string()))?;

    // 触发一次 WarpParse 健康检查
    check_device_health(device_id).await;

    // 返回最新状态（若查询失败则回退到旧值）
    let refreshed = find_device_by_id(device_id).await?.unwrap_or(existing);
    Ok(refreshed)
}

/// 删除指定 ID 的设备（软删除）
pub async fn delete_device_logic(id: i32) -> Result<(), AppError> {
    info!("删除设备: id={}", id);

    let result = async {
        db_delete_device(id).await?;

        info!("删除设备成功: id={}", id);
        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::Device,
        OperationLogAction::Delete,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("delete_mode", "soft"),
        &result,
    )
    .await;

    result
}
