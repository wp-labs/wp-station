//! 知识库 SQL 查询辅助。

use anyhow::Result;
use tracing::debug;
use wp_knowledge::facade;
use wp_knowledge::mem::RowData;
use wp_model_core::model::DataField;

/// 初始化知识库字段定义。
///
/// 当前仍为占位实现，后续可补充为扫描 `models/knowledge` 下全部定义文件。
pub fn db_init() -> Result<Vec<DataField>> {
    Ok(vec![])
}

/// 执行 SQL 并返回第一行字段列表，供调试页快速展示使用。
pub async fn sql_query(sql: &str) -> Result<Vec<DataField>> {
    let rows: Vec<RowData> = sql_query_rows(sql).await?;
    Ok(rows.into_iter().next().unwrap_or_default())
}

/// 直接通过 `wp-knowledge` provider 执行 SQL，支持真实数据库配置。
pub async fn sql_query_rows(sql: &str) -> Result<Vec<RowData>> {
    sql_query_rows_for(None, sql).await
}

/// 通过指定的命名 provider 执行 SQL；未指定时使用当前默认 provider。
pub async fn sql_query_rows_for(provider_name: Option<&str>, sql: &str) -> Result<Vec<RowData>> {
    let rows: Vec<RowData> = match provider_name {
        Some(provider_name) => facade::query_async_for(provider_name, sql).await,
        None => facade::query_async(sql).await,
    }
    .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    debug!(
        "知识库工具执行 SQL 查询完成: provider={}, rows={}",
        provider_name.unwrap_or("default"),
        rows.len()
    );
    Ok(rows)
}

/// 查询当前 provider 可见的数据表列表。
pub async fn sql_knowdb_list() -> Result<Vec<String>> {
    let sql = r#"SELECT GROUP_CONCAT(name, ', ') as name FROM sqlite_master WHERE type='table'"#;
    let result: RowData = sql_query(sql).await?;
    debug!("知识库工具查询数据表列表完成");
    match result.first() {
        Some(value) => {
            let list = format!("{}", value.get_value());
            let items: Vec<String> = list
                .split(',')
                .map(|s: &str| s.trim().to_string())
                .collect();
            debug!("知识库工具查询数据表列表成功: count={}", items.len());
            Ok(items)
        }
        None => Ok(vec![]),
    }
}
