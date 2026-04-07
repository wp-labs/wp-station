use std::str::FromStr;

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{
    CreateSandboxRunRequest, SandboxStage, SandboxState, create_sandbox_run_logic,
    get_latest_sandbox_run_logic, get_sandbox_run_logic, get_stage_logs_logic,
    list_sandbox_history_logic, stop_sandbox_run_logic,
};

/// 路径参数：task_id。
#[derive(serde::Deserialize)]
pub struct SandboxTaskPath {
    pub task_id: String,
}

/// 路径参数：task_id + 阶段。
#[derive(serde::Deserialize)]
pub struct SandboxLogPath {
    pub task_id: String,
    pub stage: String,
}

/// 路径参数：release id。
#[derive(serde::Deserialize)]
pub struct SandboxReleasePath {
    pub id: i32,
}

/// 历史记录查询参数。
#[derive(serde::Deserialize)]
pub struct SandboxHistoryQuery {
    pub limit: Option<u32>,
}

/// 默认的历史记录条数。
fn default_history_limit() -> u32 {
    20
}

/// 创建沙盒运行任务
#[post("/api/sandbox/runs")]
pub async fn create_sandbox_run(
    state: web::Data<SandboxState>,
    req: web::Json<CreateSandboxRunRequest>,
) -> Result<HttpResponse, AppError> {
    let resp = create_sandbox_run_logic(state.get_ref().clone(), req.into_inner()).await?;
    Ok(HttpResponse::Accepted().json(resp))
}

/// 查询沙盒任务详情
#[get("/api/sandbox/runs/{task_id}")]
pub async fn get_sandbox_run(
    state: web::Data<SandboxState>,
    path: web::Path<SandboxTaskPath>,
) -> Result<HttpResponse, AppError> {
    let run = get_sandbox_run_logic(state.get_ref().clone(), &path.task_id).await?;
    Ok(HttpResponse::Ok().json(run))
}

/// 停止沙盒任务
#[post("/api/sandbox/runs/{task_id}/stop")]
pub async fn stop_sandbox_run(
    state: web::Data<SandboxState>,
    path: web::Path<SandboxTaskPath>,
) -> Result<HttpResponse, AppError> {
    let run = stop_sandbox_run_logic(state.get_ref().clone(), &path.task_id).await?;
    Ok(HttpResponse::Ok().json(run))
}

/// 获取指定阶段日志
#[get("/api/sandbox/runs/{task_id}/logs/{stage}")]
pub async fn get_sandbox_stage_logs(
    state: web::Data<SandboxState>,
    path: web::Path<SandboxLogPath>,
) -> Result<HttpResponse, AppError> {
    let stage = SandboxStage::from_str(&path.stage)
        .map_err(|_| AppError::validation("无法解析阶段参数"))?;
    let resp = get_stage_logs_logic(state.get_ref().clone(), &path.task_id, stage).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 获取 Release 最近一次沙盒结果
#[get("/api/releases/{id}/sandbox/latest")]
pub async fn get_latest_sandbox_run(
    state: web::Data<SandboxState>,
    path: web::Path<SandboxReleasePath>,
) -> Result<HttpResponse, AppError> {
    let resp = get_latest_sandbox_run_logic(state.get_ref().clone(), path.id).await?;
    Ok(HttpResponse::Ok().json(resp))
}

/// 获取指定发布的沙盒历史记录
#[get("/api/releases/{id}/sandbox/runs")]
pub async fn list_sandbox_history(
    state: web::Data<SandboxState>,
    path: web::Path<SandboxReleasePath>,
    query: web::Query<SandboxHistoryQuery>,
) -> Result<HttpResponse, AppError> {
    let limit = query
        .limit
        .unwrap_or_else(default_history_limit)
        .clamp(1, 100) as u64;
    let resp = list_sandbox_history_logic(state.get_ref().clone(), path.id, limit).await?;
    Ok(HttpResponse::Ok().json(resp))
}
