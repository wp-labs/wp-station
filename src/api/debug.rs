// 调试功能 API - HTTP 请求处理层

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    DebugKnowledgeQueryRequest, DebugKnowledgeStatusQuery, DebugParseRequest,
    DebugPerformanceRunRequest, DebugTransformRequest, SharedRecord, debug_examples_logic,
    debug_knowledge_query_logic, debug_knowledge_status_logic, debug_parse_logic,
    debug_performance_get_logic, debug_performance_run_logic, debug_transform_logic,
    oml_format_logic, wpl_format_logic,
};

#[derive(serde::Deserialize)]
pub struct DebugPerformanceGetPath {
    #[serde(rename = "taskId")]
    pub task_id: String,
}

/// 模拟调试-解析：解析日志
#[post("/api/debug/parse")]
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

/// 模拟调试-转换：使用最近一次解析结果执行 OML 转换
#[post("/api/debug/transform")]
pub async fn debug_transform(
    shared_record: web::Data<SharedRecord>,
    req: web::Json<DebugTransformRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = debug_transform_logic(shared_record.get_ref().clone(), req.oml.clone()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 模拟调试-知识库：查询知识库状态
#[get("/api/debug/knowledge/status")]
pub async fn debug_knowledge_status(
    _query: web::Query<DebugKnowledgeStatusQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询知识库配置状态列表
    let resp = debug_knowledge_status_logic().await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 模拟调试-知识库：执行 SQL 查询
#[post("/api/debug/knowledge/query")]
pub async fn debug_knowledge_query(
    req: web::Json<DebugKnowledgeQueryRequest>,
) -> Result<HttpResponse, AppError> {
    // 执行知识库 SQL 查询
    let resp = debug_knowledge_query_logic(req.table.clone(), req.sql.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 模拟调试-性能测试：启动测试任务
#[post("/api/debug/performance/run")]
pub async fn debug_performance_run(
    req: web::Json<DebugPerformanceRunRequest>,
) -> Result<HttpResponse, AppError> {
    // 创建并启动性能测试任务
    let resp = debug_performance_run_logic(req.sample.clone(), req.config.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 模拟调试-性能测试：查询测试结果
#[get("/api/debug/performance/{taskId}")]
pub async fn debug_performance_get(
    path: web::Path<DebugPerformanceGetPath>,
) -> Result<HttpResponse, AppError> {
    // 查询性能测试任务详情及结果
    let resp = debug_performance_get_logic(path.task_id.clone()).await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 模拟调试-格式化：WPL 代码格式化
#[post("/api/debug/wpl/format")]
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

/// 模拟调试-格式化：OML 代码格式化
#[post("/api/debug/oml/format")]
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

/// 模拟调试：获取示例列表
#[get("/api/debug/examples")]
pub async fn debug_examples() -> HttpResponse {
    let resp = debug_examples_logic();
    HttpResponse::Ok().json(resp)
}
