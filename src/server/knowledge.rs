// 知识库业务逻辑层

use crate::db::find_all_knowledge_configs;
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::knowledge::load_knowledge;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use wp_knowledge::facade::query as query_all;

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct KnowledgeDbListQuery {}

#[derive(Serialize, Deserialize)]
pub struct KnowdbQuery {
    /// 调试页面提交的 SQL 语句
    pub sql: String,
}

// ============ 业务逻辑函数 ============

/// 查询知识库（数据库）列表
pub async fn get_db_list_logic() -> Result<Vec<String>, AppError> {
    let configs = find_all_knowledge_configs().await?;
    let names: Vec<String> = configs.into_iter().map(|c| c.file_name).collect();
    debug!("查询知识库列表成功: count={}", names.len());

    Ok(names)
}

/// 执行知识库 SQL 查询
pub async fn query_logic(sql: String) -> Result<Value, AppError> {
    let sql = sql.trim().to_string();
    if sql.is_empty() {
        return Err(AppError::validation("SQL 不能为空"));
    }

    debug!("执行知识库 SQL 查询");

    // 确保知识库已加载
    let setting = Setting::load();
    let project_dir = &setting.project_root;

    // 加载知识库（如果已加载则跳过）
    if let Err(e) = load_knowledge(project_dir) {
        warn!("加载知识库失败: {}", e);
        // 继续尝试查询，可能知识库已经加载过了
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
