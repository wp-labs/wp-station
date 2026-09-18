//! 系统 API。
//!
//! 提供健康检查、版本信息和特性配置等基础入口。

use crate::error::AppError;
use crate::server::{get_features_config_logic, get_version_logic, hello_logic};
use actix_web::{HttpResponse, get};

#[get("/api/hello")]
/// 系统：健康检查接口。
pub async fn hello() -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(hello_logic()))
}

#[get("/api/version")]
/// 系统：获取服务版本信息。
pub async fn get_version() -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(get_version_logic()))
}

#[get("/api/features/config")]
/// 系统：获取特性配置。
pub async fn get_features_config() -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(get_features_config_logic()))
}
