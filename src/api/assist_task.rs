//! AI 辅助任务 API。
//!
//! 提供辅助任务的提交、查询、取消和结果回写入口。

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    AssistListQuery, AssistReplyRequest, AssistSubmitRequest, assist_cancel_logic,
    assist_get_logic, assist_list_logic, assist_reply_logic, assist_submit_logic,
};

/// 辅助任务路径参数。
#[derive(serde::Deserialize)]
pub struct AssistTaskIdPath {
    pub task_id: String,
}

#[post("/api/assist")]
/// 提交辅助任务（AI 分析或人工提单）。
pub async fn assist_submit(req: web::Json<AssistSubmitRequest>) -> Result<HttpResponse, AppError> {
    let resp = assist_submit_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/assist")]
/// 分页查询辅助任务列表。
pub async fn assist_list(query: web::Query<AssistListQuery>) -> Result<HttpResponse, AppError> {
    let resp = assist_list_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[get("/api/assist/{task_id}")]
/// 查询单个辅助任务详情及结果。
pub async fn assist_get(path: web::Path<AssistTaskIdPath>) -> Result<HttpResponse, AppError> {
    let resp = assist_get_logic(path.task_id.clone()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/assist/{task_id}/cancel")]
/// 取消等待中的辅助任务。
pub async fn assist_cancel(path: web::Path<AssistTaskIdPath>) -> Result<HttpResponse, AppError> {
    assist_cancel_logic(path.task_id.clone()).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": true })))
}

#[post("/api/assist/reply")]
/// 写回辅助任务结果。
pub async fn assist_reply(req: web::Json<AssistReplyRequest>) -> Result<HttpResponse, AppError> {
    assist_reply_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(serde_json::json!({ "success": true })))
}
