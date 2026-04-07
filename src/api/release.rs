// 发布管理 API - HTTP 请求处理层

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    CreateReleaseRequest, ReleaseActionRequest, ReleaseListQuery, ReleaseTargetActionRequest,
    create_release_logic, get_release_detail_logic, get_release_diff_logic, list_releases_logic,
    publish_release_logic, retry_release_logic, rollback_release_logic, validate_release_logic,
};

#[derive(serde::Deserialize)]
pub struct ReleaseDetailPath {
    pub id: i32,
}

#[derive(serde::Deserialize)]
pub struct ReleaseActionPath {
    pub id: i32,
}

/// 发布管理：获取发布版本列表
#[get("/api/releases")]
pub async fn list_releases(query: web::Query<ReleaseListQuery>) -> Result<HttpResponse, AppError> {
    let resp = list_releases_logic(query.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：获取发布版本详情
#[get("/api/releases/{id}")]
pub async fn get_release_detail(
    path: web::Path<ReleaseDetailPath>,
) -> Result<HttpResponse, AppError> {
    let resp = get_release_detail_logic(path.id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：创建发布版本
#[post("/api/releases")]
pub async fn create_release(
    req: web::Json<CreateReleaseRequest>,
) -> Result<HttpResponse, AppError> {
    let resp =
        create_release_logic(req.version.clone(), req.pipeline.clone(), req.note.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：校验发布版本
#[post("/api/releases/{id}/validate")]
pub async fn validate_release(
    path: web::Path<ReleaseActionPath>,
    _req: web::Json<ReleaseActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = validate_release_logic(path.id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：执行发布（多台设备）
#[post("/api/releases/{id}/publish")]
pub async fn publish_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseActionRequest>,
) -> Result<HttpResponse, AppError> {
    let device_ids = req.device_ids.clone().unwrap_or_default();
    let note = req.note.clone();

    let resp = publish_release_logic(path.id, device_ids, note).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：获取版本差异（git diff）
#[get("/api/releases/{id}/diff")]
pub async fn get_release_diff(
    path: web::Path<ReleaseDetailPath>,
) -> Result<HttpResponse, AppError> {
    let resp = get_release_diff_logic(path.id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：重试失败的设备
#[post("/api/releases/{id}/retry")]
pub async fn retry_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseTargetActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = retry_release_logic(path.id, req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 发布管理：回滚到上一版本
#[post("/api/releases/{id}/rollback")]
pub async fn rollback_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseTargetActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = rollback_release_logic(path.id, req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}
