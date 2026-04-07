// 规则配置 API - HTTP 请求处理层

use actix_web::{HttpRequest, HttpResponse, delete, get, post, web};
use urlencoding::decode;

use crate::error::AppError;
use crate::server::{
    CreateRuleFileRequest, DeleteRuleFileQuery, RuleContentQuery, RuleFilesQuery,
    SaveKnowledgeRuleRequest, SaveRuleRequest, ValidateRuleRequest, create_rule_file_logic,
    delete_rule_file_logic, get_rule_content_logic, get_rule_files_logic,
    save_knowledge_rule_logic, save_rule_logic, validate_rule_logic,
};

/// 配置管理-规则配置：获取规则文件列表
#[get("/api/config/rules/files")]
pub async fn get_rule_files(query: web::Query<RuleFilesQuery>) -> Result<HttpResponse, AppError> {
    let resp = get_rule_files_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 配置管理-规则配置：获取规则内容
#[get("/api/config/rules")]
pub async fn get_rule_content(
    query: web::Query<RuleContentQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询规则配置内容
    let resp = get_rule_content_logic(query.rule_type, query.file.clone()).await?;

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

/// 配置管理-规则配置：创建规则文件
#[post("/api/config/rules/files")]
pub async fn create_rule_file(
    _http_req: HttpRequest,
    req: web::Json<CreateRuleFileRequest>,
) -> Result<HttpResponse, AppError> {
    // 创建新的规则文件
    create_rule_file_logic(req.rule_type, req.file.clone()).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// 配置管理-规则配置：删除规则文件
#[delete("/api/config/rules/files")]
pub async fn delete_rule_file(
    http_req: HttpRequest,
    query: web::Query<DeleteRuleFileQuery>,
) -> Result<HttpResponse, AppError> {
    // 删除规则文件
    let operator = operator_from_request(&http_req);
    delete_rule_file_logic(query.rule_type, query.file.clone(), operator).await?;

    Ok(HttpResponse::NoContent().finish())
}

/// 配置管理-规则配置：保存规则内容
#[post("/api/config/rules/save")]
pub async fn save_rule(
    http_req: HttpRequest,
    req: web::Json<SaveRuleRequest>,
) -> Result<HttpResponse, AppError> {
    // 保存规则配置
    let operator = operator_from_request(&http_req);
    save_rule_logic(
        req.rule_type,
        req.file.clone(),
        req.content.clone(),
        operator,
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

/// 配置管理-知识库配置：保存知识库规则
#[post("/api/config/knowledge/save")]
pub async fn save_knowledge_rule(
    http_req: HttpRequest,
    req: web::Json<SaveKnowledgeRuleRequest>,
) -> Result<HttpResponse, AppError> {
    // 保存知识库规则配置（包含 config、create_sql、insert_sql、data）
    let operator = operator_from_request(&http_req);
    save_knowledge_rule_logic(
        req.file.clone(),
        req.config.clone(),
        req.create_sql.clone(),
        req.insert_sql.clone(),
        req.data.clone(),
        operator,
    )
    .await?;

    Ok(HttpResponse::NoContent().finish())
}

/// 配置管理-规则配置：校验规则
#[post("/api/config/rules/validate")]
pub async fn validate_rule(req: web::Json<ValidateRuleRequest>) -> Result<HttpResponse, AppError> {
    // 校验规则配置是否正确
    let resp = validate_rule_logic(req.rule_type, req.file.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}
