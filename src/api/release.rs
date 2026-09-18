//! 发布管理 API。
//!
//! 负责发布列表、详情、创建、校验、发布、差异、重试和回滚入口。
//! 双系统改造后，发布相关接口全部围绕显式 `system` 或发布记录上的 `system` 工作。

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    CreateReleaseRequest, ReleaseActionRequest, ReleaseListQuery, ReleaseRestoreRequest,
    ReleaseTargetActionRequest, create_release_logic, create_restore_job_logic,
    get_release_detail_logic, get_release_diff_logic, get_restore_job_logic, list_releases_logic,
    publish_release_logic, retry_release_logic, rollback_release_logic, validate_release_logic,
};
use crate::db::ReleaseGroup;

/// 发布详情路径参数。
#[derive(serde::Deserialize)]
pub struct ReleaseDetailPath {
    pub id: i32,
}

/// 发布动作路径参数。
#[derive(serde::Deserialize)]
pub struct ReleaseActionPath {
    pub id: i32,
}

/// 发布差异查询参数。
#[derive(serde::Deserialize)]
pub struct ReleaseDiffQuery {
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: usize,
}

#[get("/api/releases")]
/// 发布管理：获取发布版本列表。
pub async fn list_releases(query: web::Query<ReleaseListQuery>) -> Result<HttpResponse, AppError> {
    let resp = list_releases_logic(query.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/releases/{id}")]
/// 发布管理：获取发布版本详情。
pub async fn get_release_detail(
    path: web::Path<ReleaseDetailPath>,
) -> Result<HttpResponse, AppError> {
    let resp = get_release_detail_logic(path.id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases")]
/// 发布管理：创建发布版本。
pub async fn create_release(
    req: web::Json<CreateReleaseRequest>,
) -> Result<HttpResponse, AppError> {
    // 新建发布时必须显式绑定 system，避免再依赖默认 wparse。
    let resp = create_release_logic(req.system, req.pipeline.clone(), req.note.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases/{id}/validate")]
/// 发布管理：校验发布版本。
pub async fn validate_release(
    path: web::Path<ReleaseActionPath>,
    _req: web::Json<ReleaseActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = validate_release_logic(path.id).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases/{id}/publish")]
/// 发布管理：执行发布。
pub async fn publish_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseActionRequest>,
) -> Result<HttpResponse, AppError> {
    let full_publish = req.full_publish.unwrap_or(false);
    let release_group = req.release_group.unwrap_or(ReleaseGroup::Models);
    let device_ids = req.device_ids.clone().unwrap_or_default();
    let note = req.note.clone();

    // 具体发布分发由 server 层根据发布记录中的 system 决定。
    let resp = publish_release_logic(path.id, release_group, device_ids, note, full_publish).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/releases/{id}/diff")]
/// 发布管理：获取版本差异。
pub async fn get_release_diff(
    path: web::Path<ReleaseDetailPath>,
    query: web::Query<ReleaseDiffQuery>,
) -> Result<HttpResponse, AppError> {
    let query = query.into_inner();
    let resp = get_release_diff_logic(path.id, query.offset, query.limit).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases/{id}/retry")]
/// 发布管理：重试失败设备。
pub async fn retry_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseTargetActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = retry_release_logic(path.id, req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases/{id}/rollback")]
/// 发布管理：回滚到上一版本。
pub async fn rollback_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseTargetActionRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = rollback_release_logic(path.id, req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/releases/{id}/restore")]
/// 发布管理：基于历史成功版本创建新的还原发布任务。
pub async fn restore_release(
    path: web::Path<ReleaseActionPath>,
    req: web::Json<ReleaseRestoreRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = create_restore_job_logic(path.id, req.into_inner()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/release-restores/{id}")]
/// 发布管理：查询还原任务及设备阶段详情。
pub async fn get_release_restore(
    path: web::Path<ReleaseDetailPath>,
) -> Result<HttpResponse, AppError> {
    let resp = get_restore_job_logic(path.id).await?;
    Ok(HttpResponse::Ok().json(resp))
}
