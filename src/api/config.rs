//! 配置管理 API。
//!
//! 负责 `wparse` / `wfusion` 两套系统下的配置读写入口。
//! 当前前端仍复用一套页面，因此接口层统一要求显式传入 `system`。

use actix_web::{HttpResponse, delete, get, post, web};

use crate::error::AppError;
use crate::server::{
    ConfigFilesQuery, ConfigQuery, ConfigTemplateQuery, CreateConfigFileRequest,
    DeleteConfigFileQuery, RenderConfigTemplateRequest, SaveConfigRequest,
    create_config_file_logic, delete_config_file_logic, get_config_files_logic, get_config_logic,
    get_config_templates_logic, render_config_template_logic, save_config_logic,
};

#[get("/api/config/files")]
/// 配置管理：获取配置文件列表。
pub async fn get_config_files(
    query: web::Query<ConfigFilesQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询配置文件列表（Source/Sink/Parse 等）
    let resp = get_config_files_logic(query.system, query.rule_type, query.keyword.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/config/templates")]
/// 配置管理：获取来源 / 输出模板列表。
pub async fn get_config_templates(
    query: web::Query<ConfigTemplateQuery>,
) -> Result<HttpResponse, AppError> {
    let resp = get_config_templates_logic(query.scope).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/config")]
/// 配置管理：获取配置内容。
pub async fn get_config(query: web::Query<ConfigQuery>) -> Result<HttpResponse, AppError> {
    // 查询配置文件内容
    let resp = get_config_logic(query.system, query.rule_type, query.file.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/config/templates/render")]
/// 配置管理：渲染来源 / 输出配置模板片段。
pub async fn render_config_template(
    req: web::Json<RenderConfigTemplateRequest>,
) -> Result<HttpResponse, AppError> {
    let resp =
        render_config_template_logic(req.scope, req.template_id.clone(), req.content.clone())
            .await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/config")]
/// 配置管理：保存配置内容。
pub async fn save_config(req: web::Json<SaveConfigRequest>) -> Result<HttpResponse, AppError> {
    let resp = save_config_logic(
        req.system,
        req.rule_type,
        req.file.clone(),
        req.content.clone(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/config/files")]
/// 配置管理：创建配置文件。
pub async fn create_config_file(
    req: web::Json<CreateConfigFileRequest>,
) -> Result<HttpResponse, AppError> {
    // 创建空文件只是入口动作，真正的目录定位由 server 层按 system 分发。
    let resp = create_config_file_logic(
        req.system,
        req.rule_type,
        req.file.clone(),
        req.display_name.clone(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[delete("/api/config/files")]
/// 配置管理：删除配置文件。
pub async fn delete_config_file(
    query: web::Query<DeleteConfigFileQuery>,
) -> Result<HttpResponse, AppError> {
    // 删除成功后仍需由 server 层统一处理同步和草稿刷新。
    let resp = delete_config_file_logic(query.system, query.rule_type, query.file.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}
