// 操作日志 API - HTTP 请求处理层

use actix_web::{HttpResponse, get, web};

use crate::error::AppError;
use crate::server::{LogListQuery, list_logs_logic};

/// 操作日志：获取分页列表（支持操作人/操作类型/时间范围筛选）
#[get("/api/operation-logs")]
pub async fn list_operation_logs(
    query: web::Query<LogListQuery>,
) -> Result<HttpResponse, AppError> {
    let resp = list_logs_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}
