//! 项目导入导出 API。
//!
//! 目录导入、归档预检、归档确认导入、归档导出都统一走这里，
//! 但真正的目录拆分和系统分发逻辑放在 server 层。

use actix_web::{HttpRequest, HttpResponse, get, http::header, post, web};
use futures_util::StreamExt;
use urlencoding::decode;

use crate::constants::api::MAX_ARCHIVE_BYTES;
use crate::error::AppError;
use crate::server::project::{
    ProjectArchiveConfirmRequest, ProjectImportRequest, confirm_project_archive_import_logic,
    export_project_archive_logic, import_project_from_files_logic, preview_project_archive_logic,
};

/// 从上传请求头中提取归档文件名。
fn archive_file_name(req: &HttpRequest) -> Option<String> {
    req.headers()
        .get("x-file-name")
        .and_then(|value| value.to_str().ok())
        .and_then(|raw| decode(raw).ok())
        .map(|cow| cow.trim().to_string())
        .filter(|name| !name.is_empty())
}

/// 从 query string 中解析 `system`。
///
/// 导入归档和导出接口采用原始流上传/下载，因此这里不走常规 JSON DTO。
fn system_from_request_query(req: &HttpRequest) -> Result<crate::utils::SystemKind, AppError> {
    req.query_string()
        .split('&')
        .find_map(|part| part.strip_prefix("system="))
        .ok_or_else(|| AppError::validation("缺少 system 参数"))?
        .parse()
        .map_err(|_| AppError::validation("system 参数无效"))
}

#[post("/api/project/import")]
/// 项目管理：按目录导入项目。
pub async fn import_project_from_files(
    req: web::Json<ProjectImportRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = import_project_from_files_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/project/import/archive")]
/// 项目管理：上传归档并执行预检。
pub async fn import_project_archive(
    http_req: HttpRequest,
    mut payload: web::Payload,
) -> Result<HttpResponse, AppError> {
    let system = system_from_request_query(&http_req)?;
    let file_name = archive_file_name(&http_req)
        .ok_or_else(|| AppError::validation("缺少上传文件名，请设置 X-File-Name"))?;
    let mut bytes = web::BytesMut::new();

    while let Some(chunk) = payload.next().await {
        let chunk = chunk.map_err(|e| AppError::validation(format!("读取上传内容失败: {}", e)))?;
        if bytes.len() + chunk.len() > MAX_ARCHIVE_BYTES {
            return Err(AppError::validation("上传文件超过 200MB 限制"));
        }
        bytes.extend_from_slice(&chunk);
    }

    let resp = preview_project_archive_logic(system, &file_name, bytes.freeze().to_vec()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/project/import/archive/confirm")]
/// 项目管理：确认归档导入。
pub async fn confirm_project_archive_import(
    req: web::Json<ProjectArchiveConfirmRequest>,
) -> Result<HttpResponse, AppError> {
    let req = req.into_inner();
    let resp = confirm_project_archive_import_logic(req.system, &req.import_id).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/project/export/archive")]
/// 项目管理：导出当前系统归档。
pub async fn export_project_archive(http_req: HttpRequest) -> Result<HttpResponse, AppError> {
    let system = system_from_request_query(&http_req)?;
    let archive = export_project_archive_logic(system).await?;
    Ok(HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/gzip"))
        .insert_header((
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", archive.file_name),
        ))
        .body(archive.bytes))
}
