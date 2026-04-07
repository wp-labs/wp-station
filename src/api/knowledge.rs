// 知识库 API - HTTP 请求处理层

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{KnowdbQuery, KnowledgeDbListQuery, get_db_list_logic, query_logic};

/// 知识库：获取知识库列表
#[get("/api/db_list")]
pub async fn get_db_list(
    _list_query: web::Query<KnowledgeDbListQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询知识库配置列表
    let resp = get_db_list_logic().await?;

    Ok(HttpResponse::Ok().json(resp))
}

/// 知识库：执行 SQL 查询
#[post("/api/db")]
pub async fn query(req: web::Json<KnowdbQuery>) -> Result<HttpResponse, AppError> {
    let resp = query_logic(req.into_inner().sql).await?;
    Ok(HttpResponse::Ok().json(resp))
}
