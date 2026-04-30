// 配置管理业务逻辑层

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::Setting;
use crate::server::sync::{sync_delete_to_gitea, sync_to_gitea};
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, ProjectLayout,
    refresh_draft_release_logic, write_operation_log_for_result,
};
use crate::utils::{
    common::fallback_sink_display, delete_rule_from_project, list_rule_files, read_rule_content,
    touch_rule_in_project, write_rule_content,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct ConfigFilesQuery {
    pub rule_type: RuleType,
    pub keyword: Option<String>,
}

#[derive(Deserialize)]
pub struct ConfigQuery {
    pub rule_type: RuleType,
    pub file: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveConfigRequest {
    pub rule_type: RuleType,
    pub file: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct CreateConfigFileRequest {
    pub rule_type: RuleType,
    pub file: String,
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteConfigFileQuery {
    pub rule_type: RuleType,
    pub file: String,
}

// ============ 响应结构体 ============

#[derive(Serialize)]
pub struct ConfigFileItem {
    pub file: String,
    pub display_name: Option<String>,
    pub file_size: Option<i32>,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
pub struct ConfigFilesResponse {
    pub items: Vec<ConfigFileItem>,
}

#[derive(Serialize)]
pub struct ConfigItem {
    #[serde(rename = "rule_type")]
    pub rule_type: RuleType,
    pub file: String,
    pub display_name: Option<String>,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
pub struct SimpleResult {
    pub success: bool,
}

// ============ 业务逻辑函数 ============

fn project_layout() -> ProjectLayout {
    Setting::load().project_layout()
}

fn fallback_display_name(rule_type: RuleType, file_name: &str) -> Option<String> {
    if matches!(rule_type, RuleType::Sink) {
        return fallback_sink_display(file_name).map(|label| label.to_string());
    }
    None
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

/// 获取配置文件列表
pub async fn get_config_files_logic(
    rule_type: RuleType,
    keyword: Option<String>,
) -> Result<ConfigFilesResponse, AppError> {
    let layout = project_layout();
    let files = list_rule_files(&layout, rule_type)?;
    let should_filter_default_sink = matches!(rule_type, RuleType::Sink);

    // `defaults.toml` 仅作为内部默认配置，不在 sink 文件列表中展示。
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

        items.push(ConfigFileItem {
            file,
            display_name,
            file_size,
            last_modified,
        });
    }

    Ok(ConfigFilesResponse { items })
}

/// 获取单个或多个配置文件内容
pub async fn get_config_logic(
    rule_type: RuleType,
    file: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let layout = project_layout();

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
            // 对于 source / parse 配置，如果不存在则返回 200 + 空内容，方便前端展示空白配置
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

/// 保存配置文件内容
pub async fn save_config_logic(
    rule_type: RuleType,
    file: String,
    content: String,
    _operator: Option<String>,
) -> Result<SimpleResult, AppError> {
    info!(
        "保存配置文件: rule_type={}, file={}, size={}",
        rule_type.as_ref(),
        file,
        content.len()
    );

    let size = content.len() as i32;
    let layout = project_layout();
    let is_update = read_rule_content(&layout, rule_type, &file)?.is_some();

    let file_for_log = file.clone();
    let result = async move {
        let written_path = write_rule_content(&layout, rule_type, &file, &content)?;

        info!(
            "配置写入项目目录成功: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            written_path
        );

        let commit_message = format!("配置改动: {} - {}", rule_type.as_ref(), file);
        sync_to_gitea(
            &commit_message,
            crate::db::ReleaseGroup::from_rule_type(rule_type),
        )
        .await;
        let _ = refresh_draft_release_logic(Some(&commit_message)).await;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::ConfigFile,
        if is_update {
            OperationLogAction::Update
        } else {
            OperationLogAction::Create
        },
        OperationLogParams::new()
            .with_target_name(format!("{}/{}", rule_type.as_ref(), file_for_log))
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field("size", size.to_string())
            .with_field("sync", "project+gitea"),
        &result,
    )
    .await;
    result
}

/// 创建新的配置文件
pub async fn create_config_file_logic(
    rule_type: RuleType,
    file: String,
    display_name: Option<String>,
    _operator: Option<String>,
) -> Result<SimpleResult, AppError> {
    info!(
        "创建配置文件: rule_type={}, file={}, display_name={}",
        rule_type.as_ref(),
        file,
        display_name.as_deref().unwrap_or("-")
    );

    let file_for_log = file.clone();
    let display_name_for_log = display_name.clone();
    let result = async move {
        let layout = project_layout();
        let created_path = touch_rule_in_project(&layout, rule_type, &file)?;
        info!(
            "配置文件已创建到项目目录: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            created_path
        );

        let commit_message = format!("新增配置文件: {} - {}", rule_type.as_ref(), file);
        sync_to_gitea(
            &commit_message,
            crate::db::ReleaseGroup::from_rule_type(rule_type),
        )
        .await;
        let _ = refresh_draft_release_logic(Some(&commit_message)).await;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::ConfigFile,
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name(format!("{}/{}", rule_type.as_ref(), file_for_log.clone()))
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field(
                "display_name",
                display_name_for_log.as_deref().unwrap_or("-"),
            )
            .with_field("content", "empty")
            .with_field("sync", "project+gitea"),
        &result,
    )
    .await;
    result
}

/// 删除配置文件
pub async fn delete_config_file_logic(
    rule_type: RuleType,
    file: String,
    _operator: Option<String>,
) -> Result<SimpleResult, AppError> {
    info!(
        "删除配置文件: rule_type={}, file={}",
        rule_type.as_ref(),
        file
    );

    let file_for_log = file.clone();
    let result = async move {
        let layout = project_layout();
        let deleted_path = delete_rule_from_project(&layout, rule_type, &file)?;
        info!(
            "配置文件项目目录删除成功: rule_type={}, file={}, path={}",
            rule_type.as_ref(),
            file,
            deleted_path
        );

        sync_delete_to_gitea(rule_type, &file).await;
        let draft_note = format!("删除配置文件: {} - {}", rule_type.as_ref(), file);
        let _ = refresh_draft_release_logic(Some(&draft_note)).await;

        Ok::<_, AppError>(SimpleResult { success: true })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::ConfigFile,
        OperationLogAction::Delete,
        OperationLogParams::new()
            .with_target_name(format!("{}/{}", rule_type.as_ref(), file_for_log.clone()))
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field("sync", "project+gitea"),
        &result,
    )
    .await;
    result
}
