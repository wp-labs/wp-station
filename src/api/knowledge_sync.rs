//! 知识库数据同步状态 API。

use actix_web::{HttpResponse, get, post, web};
use serde::Deserialize;

use crate::error::AppError;
use crate::server::{KnowledgeSyncState, get_knowledge_sync_status_logic};

#[get("/api/knowledge-sync/status")]
/// 获取资产同步初始化、定时任务和数据状态。
pub async fn get_knowledge_sync_status(
    state: web::Data<KnowledgeSyncState>,
) -> Result<HttpResponse, AppError> {
    let response = get_knowledge_sync_status_logic(&state).await;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

impl HistoryQuery {
    fn bounds(&self) -> Result<(i64, i64), AppError> {
        let limit = self.limit.unwrap_or(20);
        let offset = self.offset.unwrap_or(0);
        if !(1..=100).contains(&limit) || offset < 0 {
            return Err(AppError::validation("历史分页参数无效"));
        }
        Ok((limit, offset))
    }
}

#[get("/api/knowledge-sync/asset-runs")]
/// 查询 pg_cron 中的资产任务历史。
pub async fn list_asset_runs(
    state: web::Data<KnowledgeSyncState>,
    query: web::Query<HistoryQuery>,
) -> Result<HttpResponse, AppError> {
    let (limit, offset) = query.bounds()?;
    Ok(HttpResponse::Ok().json(state.asset_history(limit, offset).await?))
}

#[get("/api/knowledge-sync/geo-runs")]
/// 查询目标库中的 Geo 导入历史。
pub async fn list_geo_runs(
    state: web::Data<KnowledgeSyncState>,
    query: web::Query<HistoryQuery>,
) -> Result<HttpResponse, AppError> {
    let (limit, offset) = query.bounds()?;
    Ok(HttpResponse::Ok().json(state.geo_history(limit, offset).await?))
}

#[get("/api/knowledge-sync/intel-runs")]
/// 查询 Doris → Redis 情报任务历史。
pub async fn list_intel_runs(
    state: web::Data<KnowledgeSyncState>,
    query: web::Query<HistoryQuery>,
) -> Result<HttpResponse, AppError> {
    let (limit, offset) = query.bounds()?;
    Ok(HttpResponse::Ok().json(state.intel_history(limit, offset).await?))
}

#[get("/api/knowledge-sync/mmdb-files")]
/// 列出固定映射目录下可导入的 MMDB 文件。
pub async fn list_mmdb_files(
    state: web::Data<KnowledgeSyncState>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(state.mmdb_files()?))
}

#[derive(Deserialize)]
pub struct StartGeoImportRequest {
    file_name: String,
}

#[post("/api/knowledge-sync/geo-imports")]
/// 创建异步 Geo 导入任务。
pub async fn start_geo_import(
    state: web::Data<KnowledgeSyncState>,
    request: web::Json<StartGeoImportRequest>,
) -> Result<HttpResponse, AppError> {
    let run = state.start_geo_import(&request.file_name).await?;
    Ok(HttpResponse::Accepted().json(run))
}

#[get("/api/knowledge-sync/runs/{id}")]
/// 查询 Geo 导入进度和结果。
pub async fn get_geo_import_run(
    state: web::Data<KnowledgeSyncState>,
    id: web::Path<i64>,
) -> Result<HttpResponse, AppError> {
    Ok(HttpResponse::Ok().json(state.geo_run(id.into_inner()).await?))
}
