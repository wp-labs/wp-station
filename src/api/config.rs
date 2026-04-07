// 配置管理 API - HTTP 请求处理层（负责配置管理菜单下的解析配置和连接配置功能）

use actix_web::{HttpRequest, HttpResponse, delete, get, post, web};
use urlencoding::decode;

use crate::error::AppError;
use crate::server::{
    ConfigFilesQuery, ConfigQuery, CreateConfigFileRequest, DeleteConfigFileQuery,
    SaveConfigRequest, create_config_file_logic, delete_config_file_logic, get_config_files_logic,
    get_config_logic, save_config_logic,
};

/// 配置管理：获取配置文件列表
#[get("/api/config/files")]
pub async fn get_config_files(
    query: web::Query<ConfigFilesQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询配置文件列表（Source/Sink/Parse 等）
    let resp = get_config_files_logic(query.rule_type, query.keyword.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 配置管理：获取配置内容
#[get("/api/config")]
pub async fn get_config(query: web::Query<ConfigQuery>) -> Result<HttpResponse, AppError> {
    // 查询配置文件内容
    let resp = get_config_logic(query.rule_type, query.file.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

fn operator_from_request(req: &HttpRequest) -> Option<String> {
    req.headers().get("x-operator").and_then(|value| {
        let raw = value.to_str().ok()?.trim();
        if raw.is_empty() {
            return None;
        }
        decode(raw)
            .ok()
            .map(|cow| cow.trim().to_string())
            .filter(|decoded| !decoded.is_empty())
    })
}

/// 配置管理：保存配置内容
#[post("/api/config")]
pub async fn save_config(
    http_req: HttpRequest,
    req: web::Json<SaveConfigRequest>,
) -> Result<HttpResponse, AppError> {
    // 保存配置文件内容并同步到 Gitea
    let operator = operator_from_request(&http_req);
    let resp = save_config_logic(
        req.rule_type,
        req.file.clone(),
        req.content.clone(),
        operator,
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 配置管理：创建配置文件
#[post("/api/config/files")]
pub async fn create_config_file(
    http_req: HttpRequest,
    req: web::Json<CreateConfigFileRequest>,
) -> Result<HttpResponse, AppError> {
    // 创建新的配置文件
    let operator = operator_from_request(&http_req);
    let resp = create_config_file_logic(
        req.rule_type,
        req.file.clone(),
        req.display_name.clone(),
        operator,
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 配置管理：删除配置文件
#[delete("/api/config/files")]
pub async fn delete_config_file(
    http_req: HttpRequest,
    query: web::Query<DeleteConfigFileQuery>,
) -> Result<HttpResponse, AppError> {
    // 删除配置文件
    let operator = operator_from_request(&http_req);
    let resp = delete_config_file_logic(query.rule_type, query.file.clone(), operator).await?;

    Ok(HttpResponse::Ok().json(resp))
}
