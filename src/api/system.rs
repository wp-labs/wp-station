// 系统 API - HTTP 请求处理层

use crate::error::AppError;
use crate::server::{get_version_logic, hello_logic};
use actix_web::{HttpResponse, get};

/// 系统：健康检查接口
#[get("/api/hello")]
pub async fn hello() -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(hello_logic()))
}

/// 系统：获取服务版本信息
#[get("/api/version")]
pub async fn get_version() -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(get_version_logic()))
}
