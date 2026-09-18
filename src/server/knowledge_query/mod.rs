//! 知识库业务逻辑层。
//!
//! 负责知识库目录列表查询，以及调试页使用的 SQL 查询入口。

use crate::error::AppError;
use crate::server::Setting;
use crate::utils::knowledge::load_knowledge;
use crate::utils::list_knowledge_dirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wp_knowledge::facade::query as query_all;

// ============ 请求参数结构体 ============

/// 知识库列表查询参数，占位保留给未来扩展。
#[derive(Deserialize)]
pub struct KnowledgeDbListQuery {}

/// 知识库 SQL 查询请求体。
#[derive(Serialize, Deserialize)]
pub struct KnowdbQuery {
    /// 调试页面提交的 SQL 语句
    pub sql: String,
}

// ============ 业务逻辑函数 ============

/// 查询项目中的知识库目录列表。
pub async fn get_db_list_logic() -> Result<Vec<String>, AppError> {
    let setting = Setting::load();
    let names = list_knowledge_dirs(&setting.wparse_layout())?;
    debug!("查询知识库列表成功: count={}", names.len());

    Ok(names)
}

/// 执行知识库 SQL 查询并返回 JSON 结果。
pub async fn query_logic(sql: String) -> Result<Value, AppError> {
    let sql = sql.trim().to_string();
    if sql.is_empty() {
        return Err(AppError::validation("SQL 不能为空"));
    }

    debug!("执行知识库 SQL 查询");

    let setting = Setting::load();
    let layout = setting.wparse_layout();

    if let Err(e) = load_knowledge(&layout) {
        warn!("加载知识库失败: {}", e);
        // 继续尝试查询，可能全局 runtime 已被其他入口初始化。
    }

    match query_all(&sql) {
        Ok(result) => {
            let value = serde_json::to_value(result)
                .map_err(|e| AppError::internal(format!("序列化查询结果失败: {}", e)))?;
            debug!("知识库 SQL 查询成功");
            Ok(value)
        }
        Err(err) => {
            error!("知识库 SQL 查询失败: error={}", err);
            Err(AppError::validation(format!("查询知识库失败: {}", err)))
        }
    }
}
