//! 规则管理 API。
//!
//! 负责规则文件、知识库文件和 knowdb 主配置的读写与校验入口。
//! 当前仍由一套 API 承接两套系统，因此请求必须显式传入 `system`。

use actix_web::{HttpResponse, delete, get, post, web};

use crate::error::AppError;
use crate::server::{
    CreateRuleFileRequest, DeleteRuleFileQuery, RuleContentQuery, RuleFilesQuery,
    SaveKnowdbConfigRequest, SaveKnowledgeRuleRequest, SaveRuleRequest, ValidateRuleRequest,
    create_rule_file_logic, delete_rule_file_logic, get_knowdb_config_logic,
    get_rule_content_logic, get_rule_files_logic, save_knowdb_config_logic,
    save_knowledge_rule_logic, save_rule_logic, validate_rule_logic,
};

/// 知识库主配置查询参数。
#[derive(serde::Deserialize)]
pub struct KnowdbQuery {
    pub system: crate::utils::SystemKind,
}

#[get("/api/config/rules/files")]
/// 配置管理：获取规则文件列表。
pub async fn get_rule_files(query: web::Query<RuleFilesQuery>) -> Result<HttpResponse, AppError> {
    let resp = get_rule_files_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/config/rules")]
/// 配置管理：获取规则内容。
pub async fn get_rule_content(
    query: web::Query<RuleContentQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询规则配置内容
    let resp = get_rule_content_logic(query.system, query.rule_type, query.file.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/config/rules/files")]
/// 配置管理：创建规则文件。
pub async fn create_rule_file(
    req: web::Json<CreateRuleFileRequest>,
) -> Result<HttpResponse, AppError> {
    // 规则目录由 server 层根据 system + rule_type 解析。
    create_rule_file_logic(req.system, req.rule_type, req.file.clone()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[delete("/api/config/rules/files")]
/// 配置管理：删除规则文件。
pub async fn delete_rule_file(
    query: web::Query<DeleteRuleFileQuery>,
) -> Result<HttpResponse, AppError> {
    // 删除后还需要由 server 层继续处理 Gitea 同步和草稿刷新。
    delete_rule_file_logic(query.system, query.rule_type, query.file.clone()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/config/rules/save")]
/// 配置管理：保存规则内容。
pub async fn save_rule(req: web::Json<SaveRuleRequest>) -> Result<HttpResponse, AppError> {
    // 保存链路统一走 server 层，避免在 API 层散落目录和同步逻辑。
    save_rule_logic(
        req.system,
        req.rule_type,
        req.file.clone(),
        req.content.clone(),
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/config/knowledge/save")]
/// 配置管理：保存知识库规则。
pub async fn save_knowledge_rule(
    req: web::Json<SaveKnowledgeRuleRequest>,
) -> Result<HttpResponse, AppError> {
    // 知识库目录结构与普通规则不同，因此独立走专门保存逻辑。
    save_knowledge_rule_logic(
        req.system,
        req.file.clone(),
        req.config.clone(),
        req.create_sql.clone(),
        req.insert_sql.clone(),
        req.data.clone(),
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

#[get("/api/config/knowledge/knowdb")]
/// 配置管理：获取 knowdb 配置。
pub async fn get_knowdb_config(query: web::Query<KnowdbQuery>) -> Result<HttpResponse, AppError> {
    let resp = get_knowdb_config_logic(query.system).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/config/knowledge/knowdb")]
/// 配置管理：保存 knowdb 配置。
pub async fn save_knowdb_config(
    req: web::Json<SaveKnowdbConfigRequest>,
) -> Result<HttpResponse, AppError> {
    save_knowdb_config_logic(req.system, req.content.clone()).await?;

    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/config/rules/validate")]
/// 配置管理：校验规则。
pub async fn validate_rule(req: web::Json<ValidateRuleRequest>) -> Result<HttpResponse, AppError> {
    // 校验入口统一，但内部仍按 system 分发到各自实现。
    let resp = validate_rule_logic(
        req.system,
        req.rule_type,
        req.file.clone(),
        req.content.clone(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}
