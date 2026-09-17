//! 规则文件读写相关业务。
//!
//! 包括：
//! - 规则列表
//! - 规则内容读取
//! - 规则文件创建与删除
//! - 普通规则保存

use crate::constants::project::{
    FILE_KNOWDB, FILE_WFUSION_GLOBAL_RULE, FILE_WPL_PARSE, FILE_WPL_SAMPLE,
};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::refresh_draft_release_logic;
use crate::server::sync::{sync_delete_to_gitea, sync_to_gitea};
use crate::utils::display::fallback_sink_display;
use crate::utils::pagination::MemoryPaginate;
use crate::utils::{
    SystemKind, delete_knowledge_from_project, delete_rule_from_project, list_knowledge_dirs,
    list_rule_files, read_knowledge_files, read_rule_content, read_wpl_sample_content,
    touch_knowledge_in_project, touch_rule_in_project, write_rule_content,
    write_wpl_sample_content,
};

use super::{
    KnowledgeRuleContentResponse, RuleContentResponse, RuleFileItem, RuleFilesMeta, RuleFilesQuery,
    RuleFilesResponse, WplSubFile, build_rule_files_response, format_wpl_virtual_file, repo_layout,
    split_wpl_virtual_file, system_time_to_rfc3339,
};

fn ensure_rule_type_supported(system: SystemKind, rule_type: RuleType) -> Result<(), AppError> {
    if matches!(system, SystemKind::Wfusion) && matches!(rule_type, RuleType::Knowledge) {
        return Err(AppError::validation("wfusion 不支持 knowledge 配置"));
    }

    if matches!(system, SystemKind::Wparse)
        && matches!(
            rule_type,
            RuleType::Windows | RuleType::Schema | RuleType::Rule | RuleType::Scenarios
        )
    {
        return Err(AppError::validation("wparse 不支持该规则类型"));
    }

    Ok(())
}

/// 获取规则文件列表。
pub async fn get_rule_files_logic(query: RuleFilesQuery) -> Result<RuleFilesResponse, AppError> {
    let RuleFilesQuery {
        system,
        rule_type,
        keyword,
        page,
    } = query;
    ensure_rule_type_supported(system, rule_type)?;

    let keyword = keyword.unwrap_or_default();
    let (page, page_size) = page.normalize(50);
    let layout = repo_layout(system);

    let files = if matches!(rule_type, RuleType::Knowledge) {
        let files = list_knowledge_dirs(&layout)?;
        build_rule_files_response(files, &keyword)
    } else {
        let files = list_rule_files(&layout, rule_type)?;
        build_rule_files_response(files, &keyword)
    };

    let items: Vec<RuleFileItem> = files
        .into_iter()
        .map(|file| {
            let display_name = if matches!(rule_type, RuleType::Sink) {
                fallback_sink_display(&file).map(|label| label.to_string())
            } else {
                None
            };
            RuleFileItem { file, display_name }
        })
        .collect();

    let paged = items.paginate(page, page_size);
    Ok(RuleFilesResponse {
        items: paged.items,
        total: paged.total,
        page: paged.page,
        page_size: paged.page_size,
        meta: RuleFilesMeta {
            wpl_parse_file: FILE_WPL_PARSE.to_string(),
            wpl_sample_file: FILE_WPL_SAMPLE.to_string(),
            knowledge_config_file: FILE_KNOWDB.to_string(),
        },
    })
}

/// 获取规则内容。
pub async fn get_rule_content_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: Option<String>,
) -> Result<serde_json::Value, AppError> {
    ensure_rule_type_supported(system, rule_type)?;
    let layout = repo_layout(system);

    if matches!(rule_type, RuleType::Knowledge) {
        let file = file.ok_or_else(|| AppError::validation("knowledge 类型必须指定 file"))?;

        return if let Some(config) = read_knowledge_files(&layout, &file)? {
            let resp = KnowledgeRuleContentResponse {
                rule_type,
                file: file.clone(),
                config: config.config_content,
                create_sql: config.create_sql,
                insert_sql: config.insert_sql,
                data: config.data_content,
            };
            serde_json::to_value(resp).map_err(AppError::internal)
        } else {
            Err(AppError::not_found("知识库配置"))
        };
    }

    if let Some(file) = file {
        if matches!(rule_type, RuleType::Wpl) {
            let (base_name, sub_file) = split_wpl_virtual_file(&file);
            let base_name = base_name.trim();
            if base_name.is_empty() {
                return Err(AppError::validation("wpl 文件名不能为空"));
            }
            let result = match sub_file {
                WplSubFile::Parse => read_rule_content(&layout, rule_type, base_name)?,
                WplSubFile::Sample => read_wpl_sample_content(&layout, base_name)?,
            };
            if let Some((content, modified)) = result {
                let resp = RuleContentResponse {
                    rule_type,
                    file: format_wpl_virtual_file(base_name, sub_file),
                    content: Some(content),
                    last_modified: Some(system_time_to_rfc3339(modified)),
                };
                return serde_json::to_value(resp).map_err(AppError::internal);
            }
            return Err(AppError::not_found("规则配置"));
        }

        if let Some((content, modified)) = read_rule_content(&layout, rule_type, &file)? {
            let resp = RuleContentResponse {
                rule_type,
                file: file.clone(),
                content: Some(content),
                last_modified: Some(system_time_to_rfc3339(modified)),
            };
            serde_json::to_value(resp).map_err(AppError::internal)
        } else {
            Err(AppError::not_found("规则配置"))
        }
    } else {
        debug!("查询所有规则配置: rule_type={}", rule_type.as_ref());
        let files = list_rule_files(&layout, rule_type)?;

        let mut items = Vec::new();
        for file in files {
            if let Some((content, modified)) = read_rule_content(&layout, rule_type, &file)? {
                items.push(RuleContentResponse {
                    rule_type,
                    file,
                    content: Some(content),
                    last_modified: Some(system_time_to_rfc3339(modified)),
                });
            }
        }

        serde_json::to_value(items).map_err(AppError::internal)
    }
}

/// 创建规则文件。
pub async fn create_rule_file_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
) -> Result<(), AppError> {
    info!("创建规则文件: rule_type={:?}, file={}", rule_type, file);
    ensure_rule_type_supported(system, rule_type)?;

    let normalized_file = if matches!(rule_type, RuleType::Wpl) {
        let (base_name, _) = split_wpl_virtual_file(&file);
        let trimmed = base_name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::validation("wpl 文件名不能为空"));
        }
        trimmed
    } else {
        file.clone()
    };

    async move {
        let layout = repo_layout(system);

        if matches!(rule_type, RuleType::Knowledge) {
            let created_path = touch_knowledge_in_project(&layout, &normalized_file)?;
            info!(
                "知识库规则文件创建成功: file={}, path={}",
                normalized_file, created_path
            );
            let draft_note = format!("新增知识库: {}", normalized_file);
            let _ = refresh_draft_release_logic(system, Some(&draft_note)).await;
            return Ok::<_, AppError>(());
        }

        let created_path = touch_rule_in_project(&layout, rule_type, &normalized_file)?;
        info!(
            "规则文件创建成功: rule_type={:?}, file={}, path={}",
            rule_type, normalized_file, created_path
        );
        let draft_note = format!("新增规则文件: {} - {}", rule_type.as_ref(), normalized_file);
        let _ = refresh_draft_release_logic(system, Some(&draft_note)).await;

        Ok::<_, AppError>(())
    }
    .await
}

/// 删除规则文件。
pub async fn delete_rule_file_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
) -> Result<(), AppError> {
    info!("删除规则文件: rule_type={:?}, file={}", rule_type, file);
    ensure_rule_type_supported(system, rule_type)?;

    if matches!(system, SystemKind::Wfusion)
        && matches!(rule_type, RuleType::Rule)
        && file
            .trim()
            .trim_matches('/')
            .rsplit('/')
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case(FILE_WFUSION_GLOBAL_RULE))
    {
        return Err(AppError::validation("WFusion 全局规则文件不允许删除"));
    }

    let normalized_file = if matches!(rule_type, RuleType::Wpl) {
        let (base_name, _) = split_wpl_virtual_file(&file);
        let trimmed = base_name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::validation("wpl 文件名不能为空"));
        }
        trimmed
    } else {
        file.clone()
    };

    async move {
        let layout = repo_layout(system);

        if matches!(rule_type, RuleType::Knowledge) {
            let deleted_path = delete_knowledge_from_project(&layout, &normalized_file)?;
            info!(
                "知识库规则文件删除成功: file={}, path={}",
                normalized_file, deleted_path
            );

            sync_delete_to_gitea(system, rule_type, &normalized_file).await?;
            let draft_note = format!("删除知识库: {}", normalized_file);
            let _ = refresh_draft_release_logic(system, Some(&draft_note)).await;

            return Ok::<_, AppError>(());
        }

        let deleted_path = delete_rule_from_project(&layout, rule_type, &normalized_file)?;
        info!(
            "规则文件删除成功: rule_type={:?}, file={}, path={}",
            rule_type, normalized_file, deleted_path
        );

        sync_delete_to_gitea(system, rule_type, &normalized_file).await?;
        let draft_note = format!("删除规则文件: {} - {}", rule_type.as_ref(), normalized_file);
        let _ = refresh_draft_release_logic(system, Some(&draft_note)).await;

        Ok::<_, AppError>(())
    }
    .await
}

/// 保存普通规则内容。
pub async fn save_rule_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
    content: Option<String>,
) -> Result<(), AppError> {
    info!("保存规则配置: rule_type={:?}, file={}", rule_type, file);
    ensure_rule_type_supported(system, rule_type)?;

    let (target_file, wpl_sub_file) = if matches!(rule_type, RuleType::Wpl) {
        let (base_name, sub_file) = split_wpl_virtual_file(&file);
        let trimmed = base_name.trim().to_string();
        if trimmed.is_empty() {
            return Err(AppError::validation("wpl 文件名不能为空"));
        }
        (trimmed, Some(sub_file))
    } else {
        (file.clone(), None)
    };

    let content = content.ok_or_else(|| AppError::validation("content 不能为空"))?;
    let layout = repo_layout(system);

    let target_file_cloned = target_file.clone();
    async move {
        let written_path = if matches!(wpl_sub_file, Some(WplSubFile::Sample)) {
            write_wpl_sample_content(&layout, &target_file_cloned, &content)?
        } else {
            write_rule_content(&layout, rule_type, &target_file_cloned, &content)?
        };

        info!(
            "规则配置保存成功: rule_type={:?}, file={}, path={}",
            rule_type, target_file_cloned, written_path
        );

        let commit_message = format!("规则改动: {} - {}", rule_type.as_ref(), target_file_cloned);
        sync_to_gitea(
            &commit_message,
            system,
            crate::db::ReleaseGroup::from_rule_type(rule_type),
        )
        .await?;
        let _ = refresh_draft_release_logic(system, Some(&commit_message)).await;

        Ok::<_, AppError>(())
    }
    .await
}
