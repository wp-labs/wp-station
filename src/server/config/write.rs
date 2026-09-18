//! 配置变更相关业务。

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::refresh_draft_release_logic;
use crate::server::sync::{sync_delete_to_gitea, sync_to_gitea};
use crate::utils::{
    SystemKind, delete_rule_from_project, touch_rule_in_project, write_rule_content,
};

use super::{SimpleResult, repo_layout};

async fn sync_config_change(
    system: SystemKind,
    rule_type: RuleType,
    commit_message: &str,
) -> Result<(), AppError> {
    sync_to_gitea(
        commit_message,
        system,
        crate::db::ReleaseGroup::from_rule_type(rule_type),
    )
    .await
}

async fn refresh_impacted_drafts(
    system: SystemKind,
    rule_type: RuleType,
    note: &str,
) -> Result<(), AppError> {
    let _ = rule_type;
    let _ = refresh_draft_release_logic(system, Some(note)).await;
    Ok(())
}

/// 保存配置文件内容。
///
/// 保存成功后继续执行 Gitea 同步和草稿刷新。
pub async fn save_config_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
    content: String,
) -> Result<SimpleResult, AppError> {
    info!(
        "保存配置文件: rule_type={}, file={}, size={}",
        rule_type.as_ref(),
        file,
        content.len()
    );

    let layout = repo_layout(system);

    async move {
        let written_path = write_rule_content(&layout, rule_type, &file, &content)?;

        info!(
            "配置写入项目目录成功: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            written_path
        );

        let commit_message = format!("配置改动: {} - {}", rule_type.as_ref(), file);
        sync_config_change(system, rule_type, &commit_message).await?;
        refresh_impacted_drafts(system, rule_type, &commit_message).await?;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await
}

/// 创建新的配置文件。
pub async fn create_config_file_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
    display_name: Option<String>,
) -> Result<SimpleResult, AppError> {
    info!(
        "创建配置文件: rule_type={}, file={}, display_name={}",
        rule_type.as_ref(),
        file,
        display_name.as_deref().unwrap_or("-")
    );

    async move {
        let layout = repo_layout(system);
        let created_path = touch_rule_in_project(&layout, rule_type, &file)?;
        info!(
            "配置文件已创建到项目目录: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            created_path
        );

        let commit_message = format!("新增配置文件: {} - {}", rule_type.as_ref(), file);
        sync_config_change(system, rule_type, &commit_message).await?;
        refresh_impacted_drafts(system, rule_type, &commit_message).await?;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await
}

/// 删除配置文件。
pub async fn delete_config_file_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
) -> Result<SimpleResult, AppError> {
    info!(
        "删除配置文件: rule_type={}, file={}",
        rule_type.as_ref(),
        file
    );

    async move {
        let layout = repo_layout(system);
        let deleted_path = delete_rule_from_project(&layout, rule_type, &file)?;
        info!(
            "配置文件项目目录删除成功: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            deleted_path
        );

        sync_delete_to_gitea(system, rule_type, &file).await?;
        let draft_note = format!("删除配置文件: {} - {}", rule_type.as_ref(), file);
        refresh_impacted_drafts(system, rule_type, &draft_note).await?;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await
}
