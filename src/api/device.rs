// 设备管理 API - HTTP 请求处理层

use actix_web::{HttpResponse, delete, get, post, put, web};

use crate::error::AppError;
use crate::server::{
    CreateDeviceRequest, DeviceListQuery, UpdateDeviceRequest, create_device_logic,
    delete_device_logic, list_devices_logic, list_online_devices_logic,
    refresh_device_status_logic, update_device_logic,
};

/// 设备管理：获取设备列表（支持关键字分页）
#[get("/api/devices")]
pub async fn list_devices(query: web::Query<DeviceListQuery>) -> Result<HttpResponse, AppError> {
    let resp = list_devices_logic(query.keyword.clone(), query.page.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 设备管理：获取在线设备列表（供发布弹窗使用）
#[get("/api/devices/online")]
pub async fn list_online_devices() -> Result<HttpResponse, AppError> {
    let resp = list_online_devices_logic().await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 设备管理：创建设备
#[post("/api/devices")]
pub async fn create_device(req: web::Json<CreateDeviceRequest>) -> Result<HttpResponse, AppError> {
    let resp = create_device_logic(req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 设备管理：更新设备
#[put("/api/devices")]
pub async fn update_device(req: web::Json<UpdateDeviceRequest>) -> Result<HttpResponse, AppError> {
    let resp = update_device_logic(req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 设备管理：删除设备（软删除）
#[delete("/api/devices/{id}")]
pub async fn delete_device(path: web::Path<i32>) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    delete_device_logic(id).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// 设备管理：手动刷新设备状态
#[post("/api/devices/{id}/refresh")]
pub async fn refresh_device_status(path: web::Path<i32>) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();

    let device = refresh_device_status_logic(id).await?;

    Ok(HttpResponse::Ok().json(device))
}
