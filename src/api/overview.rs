//! 接入概览 API。
//!
//! 提供规则侧和运行时两类接入概览查询入口。

use actix_web::{HttpResponse, get, web};

use crate::error::AppError;
use crate::server::{get_integration_rule_overview_logic, get_integration_runtime_overview_logic};
use crate::utils::SystemKind;

/// 接入概览查询参数。
#[derive(serde::Deserialize)]
pub struct IntegrationOverviewQuery {
    pub system: SystemKind,
}

#[get("/api/integration-overview/rules")]
/// 接入概览：返回规则侧设备类型与日志类型摘要。
pub async fn get_integration_rule_overview(
    query: web::Query<IntegrationOverviewQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(get_integration_rule_overview_logic(query.system)?))
}

#[get("/api/integration-overview/runtime")]
/// 接入概览：返回运行时输入源与业务输出源摘要。
pub async fn get_integration_runtime_overview(
    query: web::Query<IntegrationOverviewQuery>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(get_integration_runtime_overview_logic(query.system)?))
}
