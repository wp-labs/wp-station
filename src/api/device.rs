//! 设备管理 API。
//!
//! 提供设备列表、创建、更新、删除和手动刷新状态入口。
//! 双系统改造后，和发布相关的设备查询都要求显式区分 `system`。

use actix_web::{HttpResponse, delete, get, post, put, web};

use crate::error::AppError;
use crate::server::{
    CreateDeviceRequest, DeviceListQuery, UpdateDeviceRequest, create_device_logic,
    delete_device_logic, list_devices_logic, list_online_devices_logic,
    refresh_device_status_logic, update_device_logic,
};

#[get("/api/devices")]
/// 设备管理：获取设备列表。
pub async fn list_devices(query: web::Query<DeviceListQuery>) -> Result<HttpResponse, AppError> {
    let resp = list_devices_logic(query.system, query.keyword.clone(), query.page.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/devices/online")]
/// 设备管理：获取在线设备列表。
pub async fn list_online_devices(
    query: web::Query<DeviceListQuery>,
) -> Result<HttpResponse, AppError> {
    // 在线设备列表必须限定 system，避免混出不同系统的发布目标。
    let system = query
        .system
        .ok_or_else(|| AppError::validation("缺少 system 参数"))?;
    let resp = list_online_devices_logic(system).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/devices")]
/// 设备管理：创建设备。
pub async fn create_device(req: web::Json<CreateDeviceRequest>) -> Result<HttpResponse, AppError> {
    let resp = create_device_logic(req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[put("/api/devices")]
/// 设备管理：更新设备。
pub async fn update_device(req: web::Json<UpdateDeviceRequest>) -> Result<HttpResponse, AppError> {
    let resp = update_device_logic(req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[delete("/api/devices/{id}")]
/// 设备管理：删除设备。
pub async fn delete_device(path: web::Path<i32>) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    delete_device_logic(id).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/devices/{id}/refresh")]
/// 设备管理：手动刷新设备状态。
pub async fn refresh_device_status(path: web::Path<i32>) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    // 刷新入口统一，具体健康检查由 server / utils 层按 system 分发。
    let device = refresh_device_status_logic(id).await?;

    Ok(HttpResponse::Ok().json(device))
}
