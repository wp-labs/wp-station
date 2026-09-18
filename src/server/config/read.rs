//! 配置查询相关业务。

use crate::db::RuleType;
use crate::error::AppError;
use crate::utils::{SystemKind, list_rule_files, read_rule_content};

use super::{
    ConfigFileItem, ConfigFilesResponse, ConfigItem, default_file_for_rule_type,
    fallback_display_name, file_sort_order, repo_layout, system_time_to_rfc3339,
};

/// 获取配置文件列表。
pub async fn get_config_files_logic(
    system: SystemKind,
    rule_type: RuleType,
    keyword: Option<String>,
) -> Result<ConfigFilesResponse, AppError> {
    let layout = repo_layout(system);
    let files = list_rule_files(&layout, rule_type)?;
    let should_filter_default_sink = matches!(rule_type, RuleType::Sink);

    let files: Vec<_> = files
        .into_iter()
        .filter(|file| !(should_filter_default_sink && file == "defaults.toml"))
        .collect();

    let keyword = keyword.unwrap_or_default();
    let keyword = keyword.trim();
    let mut items = Vec::new();
    for file in files {
        let display_name = fallback_display_name(rule_type, &file);
        if !keyword.is_empty()
            && !file.contains(keyword)
            && !display_name
                .as_deref()
                .map(|name| name.contains(keyword))
                .unwrap_or(false)
        {
            continue;
        }

        let (file_size, last_modified) =
            if let Some((content, modified)) = read_rule_content(&layout, rule_type, &file)? {
                (
                    Some(content.len() as i32),
                    Some(system_time_to_rfc3339(modified)),
                )
            } else {
                (None, None)
            };

        let sort_order = file_sort_order(rule_type, &file);
        items.push(ConfigFileItem {
            file,
            display_name,
            sort_order,
            file_size,
            last_modified,
        });
    }

    items.sort_by(|a, b| {
        let a_order = a.sort_order.unwrap_or(usize::MAX);
        let b_order = b.sort_order.unwrap_or(usize::MAX);
        a_order
            .cmp(&b_order)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.display_name.cmp(&b.display_name))
    });

    let default_file = default_file_for_rule_type(rule_type, &items);
    Ok(ConfigFilesResponse {
        items,
        default_file,
    })
}

/// 获取单个或多个配置文件内容。
pub async fn get_config_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let layout = repo_layout(system);

    if let Some(file) = &file {
        return if let Some((content, modified)) = read_rule_content(&layout, rule_type, file)? {
            let display_name = fallback_display_name(rule_type, file);
            let item = ConfigItem {
                rule_type,
                file: file.clone(),
                display_name,
                content: Some(content),
                last_modified: Some(system_time_to_rfc3339(modified)),
            };
            serde_json::to_value(item).map_err(AppError::internal)
        } else if matches!(rule_type, RuleType::Source | RuleType::Parse) {
            serde_json::to_value(ConfigItem {
                rule_type,
                file: file.clone(),
                display_name: None,
                content: None,
                last_modified: None,
            })
            .map_err(AppError::internal)
        } else {
            Err(AppError::NotFound("配置文件不存在".to_string()))
        };
    }

    let files = list_rule_files(&layout, rule_type)?;
    let mut items = Vec::new();
    for file in files {
        if let Some((content, modified)) = read_rule_content(&layout, rule_type, &file)? {
            items.push(ConfigItem {
                rule_type,
                display_name: fallback_display_name(rule_type, &file),
                file,
                content: Some(content),
                last_modified: Some(system_time_to_rfc3339(modified)),
            });
        }
    }

    serde_json::to_value(items).map_err(AppError::internal)
}
