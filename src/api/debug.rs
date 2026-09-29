//! 调试功能 API。
//!
//! 提供解析、转换、知识库查询和格式化入口。

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    DebugKnowledgeQueryRequest, DebugKnowledgeStatusQuery, DebugParseRequest,
    DebugTransformRequest, DebugWfusionRuleEditorParseRequest, SharedRecord,
    debug_knowledge_query_logic, debug_knowledge_status_logic, debug_parse_logic,
    debug_transform_logic, debug_wfusion_rule_editor_parse_logic, load_debug_examples,
    oml_format_logic, toml_format_logic, wfg_format_logic, wfl_format_logic, wfs_format_logic,
    wpl_format_logic,
};
use crate::utils::SystemKind;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct DebugExamplesQuery {
    #[serde(default = "default_debug_examples_system")]
    system: SystemKind,
}

fn default_debug_examples_system() -> SystemKind {
    SystemKind::Wparse
}

#[post("/api/debug/parse")]
/// 模拟调试：解析日志。
pub async fn debug_parse(
    shared_record: web::Data<SharedRecord>,
    req: web::Json<DebugParseRequest>,
) -> Result<HttpResponse, AppError> {
    // 解析日志并返回字段列表
    let resp = debug_parse_logic(
        shared_record.get_ref().clone(),
        req.rules.clone(),
        req.logs.clone(),
    )
    .await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/debug/transform")]
/// 模拟调试：执行 OML 转换。
pub async fn debug_transform(
    shared_record: web::Data<SharedRecord>,
    req: web::Json<DebugTransformRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = debug_transform_logic(shared_record.get_ref().clone(), req.oml.clone()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/debug/knowledge/status")]
/// 模拟调试：查询知识库状态。
pub async fn debug_knowledge_status(
    _query: web::Query<DebugKnowledgeStatusQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询知识库配置状态列表
    let resp = debug_knowledge_status_logic().await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/debug/knowledge/query")]
/// 模拟调试：执行知识库 SQL 查询。
pub async fn debug_knowledge_query(
    req: web::Json<DebugKnowledgeQueryRequest>,
) -> Result<HttpResponse, AppError> {
    // 执行知识库 SQL 查询
    let resp =
        debug_knowledge_query_logic(req.table.clone(), req.source_kind.clone(), req.sql.clone())
            .await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/debug/wpl/format")]
/// 模拟调试：格式化 WPL 代码。
pub async fn wpl_format(req: String) -> HttpResponse {
    match wpl_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "wpl_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "WPL_FORMAT_ERROR",
                "message": "格式化 WPL 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[post("/api/debug/oml/format")]
/// 模拟调试：格式化 OML 代码。
pub async fn oml_format(req: String) -> HttpResponse {
    match oml_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "oml_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "OML_FORMAT_ERROR",
                "message": "格式化 OML 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[post("/api/debug/wfs/format")]
/// 模拟调试：格式化 WFS 代码。
pub async fn wfs_format(req: String) -> HttpResponse {
    match wfs_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "wfs_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "WFS_FORMAT_ERROR",
                "message": "格式化 WFS 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[post("/api/debug/wfl/format")]
/// 模拟调试：格式化 WFL 代码。
pub async fn wfl_format(req: String) -> HttpResponse {
    match wfl_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "wfl_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "WFL_FORMAT_ERROR",
                "message": "格式化 WFL 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[post("/api/debug/wfg/format")]
/// 模拟调试：格式化 WFG 代码。
pub async fn wfg_format(req: String) -> HttpResponse {
    match wfg_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "wfg_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "WFG_FORMAT_ERROR",
                "message": "格式化 WFG 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[post("/api/debug/toml/format")]
/// 模拟调试：格式化 TOML 代码。
pub async fn toml_format(req: String) -> HttpResponse {
    match toml_format_logic(req) {
        Ok(formatted) => HttpResponse::Ok().json(serde_json::json!({
            "toml_code": formatted
        })),
        Err(err) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "error": {
                "code": "TOML_FORMAT_ERROR",
                "message": "格式化 TOML 代码失败",
                "detail": err.to_string()
            }
        })),
    }
}

#[get("/api/debug/examples")]
/// 模拟调试：获取示例列表。
pub async fn debug_examples(
    query: web::Query<DebugExamplesQuery>,
) -> Result<HttpResponse, AppError> {
    let resp = load_debug_examples(query.system)?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/debug/wfusion-editor/parse")]
/// WFusion 规则编辑器：解析 WFS/WFL 并试跑 NDJSON。
pub async fn debug_wfusion_rule_editor_parse(
    req: web::Json<DebugWfusionRuleEditorParseRequest>,
) -> HttpResponse {
    let resp = debug_wfusion_rule_editor_parse_logic(req.into_inner());
    HttpResponse::Ok().json(resp)
}
