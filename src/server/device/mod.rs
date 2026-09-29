//! 设备管理业务逻辑层。
//!
//! 双系统改造后，设备对象成为 `system` 的主要承载点：
//! 健康检查、在线状态刷新、发布目标选择都从这里开始分流。

mod health;
mod read;
mod write;

use crate::db::device::Device;
use crate::utils::SystemKind;
use crate::utils::pagination::{PageQuery, PageResponse};
use serde::{Deserialize, Serialize};

pub use self::read::{list_devices_logic, list_online_devices_logic, refresh_device_status_logic};
pub use self::write::{create_device_logic, delete_device_logic, update_device_logic};

// ============ 请求参数结构体 ============

/// 设备列表查询参数。
#[derive(Deserialize)]
pub struct DeviceListQuery {
    pub system: Option<SystemKind>,
    /// 关键字，匹配设备名 / IP / 备注
    pub keyword: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

/// 创建设备请求。
#[derive(Deserialize, Serialize)]
pub struct CreateDeviceRequest {
    pub system: SystemKind,
    /// 设备展示名；为空时回退为 IP
    pub name: Option<String>,
    pub ip: String,
    pub port: i32,
    /// 设备访问令牌，仅用于设备连接。
    pub token: String,
    pub remark: Option<String>,
}

/// 更新设备请求。
#[derive(Deserialize, Serialize)]
pub struct UpdateDeviceRequest {
    pub id: i32,
    pub system: SystemKind,
    /// 设备展示名；为空时回退为 IP
    pub name: Option<String>,
    pub ip: String,
    pub port: i32,
    /// 设备访问令牌，仅用于设备连接。
    pub token: String,
    pub remark: Option<String>,
}

// ============ 响应结构体 ============

/// 设备分页列表响应。
pub type DeviceListResponse = PageResponse<Device>;

/// 创建设备成功响应。
#[derive(Serialize)]
pub struct DeviceCreated {
    pub id: i32,
}

/// 更新设备后的结果摘要。
#[derive(Serialize)]
pub struct DeviceUpdateResult {
    pub success: bool,
    pub is_online: bool,
    pub message: Option<String>,
}

/// 手动刷新设备状态后的结果。
#[derive(Serialize)]
pub struct DeviceRefreshResult {
    pub device: Device,
    pub health_error: Option<String>,
}

// ============ 业务逻辑函数 ============
