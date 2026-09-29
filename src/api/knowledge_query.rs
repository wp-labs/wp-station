//! 知识库 API。
//!
//! 提供知识库目录列表和 SQL 查询入口。

use actix_web::{HttpResponse, get, post, web};

use crate::error::AppError;
use crate::server::{KnowdbQuery, KnowledgeDbListQuery, get_db_list_logic, query_logic};

#[get("/api/db_list")]
/// 知识库：获取知识库列表。
pub async fn get_db_list(
    _list_query: web::Query<KnowledgeDbListQuery>,
) -> Result<HttpResponse, AppError> {
    // 查询知识库配置列表
    let resp = get_db_list_logic().await?;

    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/db")]
/// 知识库：执行 SQL 查询。
pub async fn query(req: web::Json<KnowdbQuery>) -> Result<HttpResponse, AppError> {
    let resp = query_logic(req.into_inner().sql).await?;
    Ok(HttpResponse::Ok().json(resp))
}
