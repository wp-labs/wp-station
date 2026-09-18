//! 知识库配置相关业务。
//!
//! 包括知识库目录内容保存，以及 `knowdb.toml` 的读写。

use crate::constants::project::FILE_KNOWDB;
use crate::error::AppError;
use crate::server::refresh_draft_release_logic;
use crate::server::sync::sync_to_gitea;
use crate::utils::knowledge::reload_knowledge;
use crate::utils::{
    SystemKind, read_knowdb_config, wfusion_not_implemented, write_knowdb_config,
    write_knowledge_files,
};

use super::{KnowdbConfigResponse, repo_layout, system_time_to_rfc3339};

/// 保存知识库规则配置。
pub async fn save_knowledge_rule_logic(
    system: SystemKind,
    file: String,
    config: Option<String>,
    create_sql: Option<String>,
    insert_sql: Option<String>,
    data: Option<String>,
) -> Result<(), AppError> {
    info!("保存知识库规则配置: file={}", file);
    if matches!(system, SystemKind::Wfusion) {
        return Err(wfusion_not_implemented("知识库保存"));
    }

    let layout = repo_layout(system);
    async move {
        let table_path = write_knowledge_files(&layout, &file, create_sql, insert_sql, data)?;
        if let Some(config_content) = config {
            let knowdb_path = write_knowdb_config(&layout, &config_content)?;
            debug!("全局 knowdb 配置已随知识库保存更新: path={}", knowdb_path);
        }

        info!("知识库规则配置保存成功: file={}, path={}", file, table_path);

        reload_knowledge(&layout).map_err(AppError::internal)?;

        let commit_message = format!("知识库改动: {}", file);
        sync_to_gitea(&commit_message, system, crate::db::ReleaseGroup::Models).await?;
        let _ = refresh_draft_release_logic(system, Some(&commit_message)).await;

        Ok::<_, AppError>(())
    }
    .await
}

/// 获取 `knowdb.toml` 内容。
pub async fn get_knowdb_config_logic(system: SystemKind) -> Result<KnowdbConfigResponse, AppError> {
    if matches!(system, SystemKind::Wfusion) {
        return Err(wfusion_not_implemented("knowdb 读取"));
    }
    let layout = repo_layout(system);
    let entry = read_knowdb_config(&layout)?;
    let response = KnowdbConfigResponse {
        file: FILE_KNOWDB.to_string(),
        content: entry.as_ref().map(|(content, _)| content.clone()),
        last_modified: entry
            .as_ref()
            .map(|(_, modified)| system_time_to_rfc3339(*modified)),
    };
    Ok(response)
}

/// 保存 `knowdb.toml` 全局配置。
pub async fn save_knowdb_config_logic(
    system: SystemKind,
    content: Option<String>,
) -> Result<(), AppError> {
    info!("保存 knowdb 配置");
    if matches!(system, SystemKind::Wfusion) {
        return Err(wfusion_not_implemented("knowdb 保存"));
    }
    let layout = repo_layout(system);

    async move {
        let content = content.unwrap_or_default();
        let written_path = write_knowdb_config(&layout, &content)?;
        info!("knowdb 配置保存成功: path={}", written_path);

        reload_knowledge(&layout).map_err(AppError::internal)?;

        let commit_message = format!("知识库改动: {}", FILE_KNOWDB);
        sync_to_gitea(&commit_message, system, crate::db::ReleaseGroup::Models).await?;
        let _ = refresh_draft_release_logic(system, Some(&commit_message)).await;

        Ok::<_, AppError>(())
    }
    .await
}
