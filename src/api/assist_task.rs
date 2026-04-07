// AI 辅助任务 API - HTTP 请求处理层

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    AssistListQuery, AssistReplyRequest, AssistSubmitRequest, assist_cancel_logic,
    assist_get_logic, assist_list_logic, assist_reply_logic, assist_submit_logic,
};

#[derive(serde::Deserialize)]
pub struct AssistTaskIdPath {
    pub task_id: String,
}

/// 提交辅助任务（AI 分析或人工提单）
#[post("/api/assist")]
pub async fn assist_submit(req: web::Json<AssistSubmitRequest>) -> Result<HttpResponse, AppError> {
    let resp = assist_submit_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 分页查询辅助任务列表
#[get("/api/assist")]
pub async fn assist_list(query: web::Query<AssistListQuery>) -> Result<HttpResponse, AppError> {
    let resp = assist_list_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 查询单个辅助任务详情及结果（前端轮询此接口）
#[get("/api/assist/{task_id}")]
pub async fn assist_get(path: web::Path<AssistTaskIdPath>) -> Result<HttpResponse, AppError> {
    let resp = assist_get_logic(path.task_id.clone()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 取消等待中的辅助任务
#[post("/api/assist/{task_id}/cancel")]
pub async fn assist_cancel(path: web::Path<AssistTaskIdPath>) -> Result<HttpResponse, AppError> {
    assist_cancel_logic(path.task_id.clone()).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": true })))
}

/// 写回辅助任务结果（AI 服务或人工支持平台调用此接口）
#[post("/api/assist/reply")]
pub async fn assist_reply(req: web::Json<AssistReplyRequest>) -> Result<HttpResponse, AppError> {
    assist_reply_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": true })))
}
