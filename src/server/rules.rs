// 规则配置业务逻辑层

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::sync::{handle_draft_release, sync_delete_to_gitea, sync_to_gitea};
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::common::{WPL_PARSE_FILENAME, WPL_SAMPLE_FILENAME, fallback_sink_display};
use crate::utils::knowledge::reload_knowledge;
use crate::utils::pagination::{MemoryPaginate, PageQuery, PageResponse};
use crate::utils::project_check::check_component;
use crate::utils::{
    delete_knowledge_from_project, delete_rule_from_project, list_knowledge_dirs, list_rule_files,
    read_knowdb_config, read_knowledge_files, read_rule_content, read_wpl_sample_content,
    touch_knowledge_in_project, touch_rule_in_project, write_knowdb_config, write_knowledge_files,
    write_rule_content, write_wpl_sample_content,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WplSubFile {
    Parse,
    Sample,
}

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct RuleFilesQuery {
    pub rule_type: RuleType,
    pub keyword: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Deserialize)]
pub struct RuleContentQuery {
    pub rule_type: RuleType,
    pub file: Option<String>,
}

#[derive(Deserialize)]
pub struct CreateRuleFileRequest {
    pub rule_type: RuleType,
    pub file: String,
}

#[derive(Deserialize)]
pub struct DeleteRuleFileQuery {
    pub rule_type: RuleType,
    pub file: String,
}

#[derive(Deserialize)]
pub struct SaveRuleRequest {
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveKnowledgeRuleRequest {
    pub file: String,
    pub config: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveKnowdbConfigRequest {
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct ValidateRuleRequest {
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
}

// ============ 响应结构体定义 ============

#[derive(Serialize)]
pub struct RuleContentResponse {
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
pub struct RuleFileItem {
    pub file: String,
    pub display_name: Option<String>,
}

#[derive(Serialize)]
pub struct KnowledgeRuleContentResponse {
    pub rule_type: RuleType,
    pub file: String,
    pub config: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data: Option<String>,
}

#[derive(Serialize)]
pub struct KnowdbConfigResponse {
    pub file: String,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Serialize)]
pub struct ValidateRuleResponse {
    pub valid: bool,
    pub message: Option<String>,
}

pub type RuleFilesResponse = PageResponse<RuleFileItem>;

// ============ 业务逻辑函数 ============

/// 构建规则文件响应
fn build_rule_files_response(mut files: Vec<String>, keyword: &str) -> Vec<String> {
    files.sort();
    files.dedup();

    let keyword = keyword.trim();

    // 关键字过滤
    if !keyword.is_empty() {
        files.retain(|file| file.contains(keyword));
    }

    files
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}

/// 获取规则文件列表
pub async fn get_rule_files_logic(query: RuleFilesQuery) -> Result<RuleFilesResponse, AppError> {
    let RuleFilesQuery {
        rule_type,
        keyword,
        page,
    } = query;
    let keyword = keyword.unwrap_or_default();
    let (page, page_size) = page.normalize(50);
    let setting = Setting::load();

    // 处理知识库类型
    let mut files = if matches!(rule_type, RuleType::Knowledge) {
        let files = list_knowledge_dirs(&setting.project_root)?;
        build_rule_files_response(files, &keyword)
    } else {
        let files = list_rule_files(&setting.project_root, rule_type)?;
        build_rule_files_response(files, &keyword)
    };

    if matches!(rule_type, RuleType::Wpl) {
        let mut expanded = Vec::new();
        for entry in files.into_iter() {
            // 保留基础文件名，兼容早期接口期望
            expanded.push(entry.clone());

            let (base, _) = split_wpl_virtual_file(&entry);
            if base.trim().is_empty() {
                continue;
            }
            expanded.push(format_wpl_virtual_file(&base, WplSubFile::Parse));
            expanded.push(format_wpl_virtual_file(&base, WplSubFile::Sample));
        }
        let keyword_trim = keyword.trim();
        if !keyword_trim.is_empty() {
            expanded.retain(|file_name| file_name.contains(keyword_trim));
        }
        expanded.sort();
        expanded.dedup();
        files = expanded;
    }

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

    Ok(items.paginate(page, page_size))
}

/// 获取规则内容
pub async fn get_rule_content_logic(
    rule_type: RuleType,
    file: Option<String>,
) -> Result<serde_json::Value, AppError> {
    let setting = Setting::load();

    // 处理知识库类型
    if matches!(rule_type, RuleType::Knowledge) {
        let file = file.ok_or_else(|| AppError::validation("knowledge 类型必须指定 file"))?;

        return if let Some(config) = read_knowledge_files(&setting.project_root, &file)? {
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

    // 处理规则类型
    if let Some(file) = file {
        if matches!(rule_type, RuleType::Wpl) {
            let (base_name, sub_file) = split_wpl_virtual_file(&file);
            let base_name = base_name.trim();
            if base_name.is_empty() {
                return Err(AppError::validation("wpl 文件名不能为空"));
            }
            let result = match sub_file {
                WplSubFile::Parse => {
                    read_rule_content(&setting.project_root, rule_type, base_name)?
                }
                WplSubFile::Sample => read_wpl_sample_content(&setting.project_root, base_name)?,
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

        if let Some((content, modified)) =
            read_rule_content(&setting.project_root, rule_type, &file)?
        {
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
        let files = list_rule_files(&setting.project_root, rule_type)?;

        let mut items = Vec::new();
        for file in files {
            if let Some((content, modified)) =
                read_rule_content(&setting.project_root, rule_type, &file)?
            {
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

/// 创建规则文件
pub async fn create_rule_file_logic(rule_type: RuleType, file: String) -> Result<(), AppError> {
    info!("创建规则文件: rule_type={:?}, file={}", rule_type, file);

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

    let file_for_log = normalized_file.clone();
    let result = async move {
        let setting = Setting::load();

        // 处理知识库类型
        if matches!(rule_type, RuleType::Knowledge) {
            let created_path = touch_knowledge_in_project(&setting.project_root, &normalized_file)?;

            info!(
                "知识库规则文件创建成功: file={}, path={}",
                normalized_file, created_path
            );
            return Ok::<_, AppError>(());
        }

        let created_path =
            touch_rule_in_project(&setting.project_root, rule_type, &normalized_file)?;

        info!(
            "规则文件创建成功: rule_type={:?}, file={}, path={}",
            rule_type, normalized_file, created_path
        );

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        if matches!(rule_type, RuleType::Knowledge) {
            OperationLogBiz::KnowledgeConfig
        } else {
            OperationLogBiz::RuleFile
        },
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name(if matches!(rule_type, RuleType::Knowledge) {
                file_for_log.clone()
            } else {
                format!("{}/{}", rule_type.as_ref(), file_for_log)
            })
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field("content", "empty"),
        &result,
    )
    .await;

    result
}

/// 删除规则文件
pub async fn delete_rule_file_logic(
    rule_type: RuleType,
    file: String,
    operator: Option<String>,
) -> Result<(), AppError> {
    info!("删除规则文件: rule_type={:?}, file={}", rule_type, file);

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

    let operator_cloned = operator.clone();
    let file_for_log = normalized_file.clone();
    let result = async move {
        let setting = Setting::load();

        // 处理知识库类型
        if matches!(rule_type, RuleType::Knowledge) {
            let deleted_path =
                delete_knowledge_from_project(&setting.project_root, &normalized_file)?;

            info!(
                "知识库规则文件删除成功: file={}, path={}",
                normalized_file, deleted_path
            );

            // 同步到 Gitea
            sync_delete_to_gitea(rule_type, &normalized_file).await;

            // 更新草稿发布记录
            handle_draft_release(operator_cloned.as_deref()).await?;

            return Ok::<_, AppError>(());
        }

        let deleted_path =
            delete_rule_from_project(&setting.project_root, rule_type, &normalized_file)?;

        info!(
            "规则文件删除成功: rule_type={:?}, file={}, path={}",
            rule_type, normalized_file, deleted_path
        );

        // 同步到 Gitea
        sync_delete_to_gitea(rule_type, &normalized_file).await;

        // 更新草稿发布记录
        handle_draft_release(operator_cloned.as_deref()).await?;

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        if matches!(rule_type, RuleType::Knowledge) {
            OperationLogBiz::KnowledgeConfig
        } else {
            OperationLogBiz::RuleFile
        },
        OperationLogAction::Delete,
        OperationLogParams::new()
            .with_target_name(if matches!(rule_type, RuleType::Knowledge) {
                file_for_log.clone()
            } else {
                format!("{}/{}", rule_type.as_ref(), file_for_log)
            })
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field("sync", "gitea"),
        &result,
    )
    .await;

    result
}

/// 保存规则配置
pub async fn save_rule_logic(
    rule_type: RuleType,
    file: String,
    content: Option<String>,
    operator: Option<String>,
) -> Result<(), AppError> {
    info!("保存规则配置: rule_type={:?}, file={}", rule_type, file);

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

    let file_for_log = if let Some(sub) = wpl_sub_file {
        format_wpl_virtual_file(&target_file, sub)
    } else {
        target_file.clone()
    };

    let content = content.ok_or_else(|| AppError::validation("content 不能为空"))?;
    let size = content.len() as i32;
    let project_root = Setting::load().project_root;
    let is_update = if matches!(wpl_sub_file, Some(WplSubFile::Sample)) {
        read_wpl_sample_content(&project_root, &target_file)?.is_some()
    } else {
        read_rule_content(&project_root, rule_type, &target_file)?.is_some()
    };

    let operator_cloned = operator.clone();
    let target_file_cloned = target_file.clone();
    let result = async move {
        let written_path = if matches!(wpl_sub_file, Some(WplSubFile::Sample)) {
            write_wpl_sample_content(&project_root, &target_file_cloned, &content)?
        } else {
            write_rule_content(&project_root, rule_type, &target_file_cloned, &content)?
        };

        info!(
            "规则配置保存成功: rule_type={:?}, file={}, path={}",
            rule_type, target_file_cloned, written_path
        );

        // 同步到 Gitea
        let commit_message = format!("规则改动: {} - {}", rule_type.as_ref(), target_file_cloned);
        sync_to_gitea(&commit_message).await;

        // 自动创建或更新草稿发布记录
        handle_draft_release(operator_cloned.as_deref()).await?;

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::RuleFile,
        if is_update {
            OperationLogAction::Update
        } else {
            OperationLogAction::Create
        },
        OperationLogParams::new()
            .with_target_name(format!("{}/{}", rule_type.as_ref(), file_for_log.clone()))
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", &file_for_log)
            .with_field("size", size.to_string())
            .with_field("sync", "project+gitea")
            .with_field("draft_release", "updated"),
        &result,
    )
    .await;

    result
}

/// 保存知识库规则配置
pub async fn save_knowledge_rule_logic(
    file: String,
    config: Option<String>,
    create_sql: Option<String>,
    insert_sql: Option<String>,
    data: Option<String>,
    operator: Option<String>,
) -> Result<(), AppError> {
    info!("保存知识库规则配置: file={}", file);

    let project_root = Setting::load().project_root;
    let is_update = read_knowledge_files(&project_root, &file)?.is_some();
    // 克隆参数用于日志记录
    let file_clone = file.clone();
    let config_clone = config.clone();
    let create_sql_clone = create_sql.clone();
    let insert_sql_clone = insert_sql.clone();
    let data_clone = data.clone();

    let operator_cloned = operator.clone();
    let file_for_log = file_clone.clone();
    let result = async move {
        let table_path = write_knowledge_files(&project_root, &file, create_sql, insert_sql, data)?;
        if let Some(config_content) = config {
            let knowdb_path = write_knowdb_config(&project_root, &config_content)?;
            debug!("全局 knowdb 配置已随知识库保存更新: path={}", knowdb_path);
        }

        info!("知识库规则配置保存成功: file={}, path={}", file, table_path);

        // 知识库文件和 knowdb provider 都可能变化，保存后强制重载运行时。
        reload_knowledge(&project_root).map_err(AppError::internal)?;

        // 同步到 Gitea
        let commit_message = format!("知识库改动: {}", file);
        sync_to_gitea(&commit_message).await;

        // 自动创建或更新草稿发布记录
        handle_draft_release(operator_cloned.as_deref()).await?;

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::KnowledgeConfig,
        if is_update {
            OperationLogAction::Update
        } else {
            OperationLogAction::Create
        },
        OperationLogParams::new()
            .with_target_name(file_for_log)
            .with_field("config", if config_clone.is_some() { "yes" } else { "no" })
            .with_field(
                "create_sql",
                if create_sql_clone.is_some() {
                    "yes"
                } else {
                    "no"
                },
            )
            .with_field(
                "insert_sql",
                if insert_sql_clone.is_some() {
                    "yes"
                } else {
                    "no"
                },
            )
            .with_field("data", if data_clone.is_some() { "yes" } else { "no" })
            .with_field("sync", "project+gitea")
            .with_field("knowledge_reload", "yes"),
        &result,
    )
    .await;

    result
}

/// 获取 knowdb.toml 内容
pub async fn get_knowdb_config_logic() -> Result<KnowdbConfigResponse, AppError> {
    let project_root = Setting::load().project_root;
    let entry = read_knowdb_config(&project_root)?;
    let response = KnowdbConfigResponse {
        file: "knowdb.toml".to_string(),
        content: entry.as_ref().map(|(content, _)| content.clone()),
        last_modified: entry
            .as_ref()
            .map(|(_, modified)| system_time_to_rfc3339(*modified)),
    };
    Ok(response)
}

/// 保存 knowdb.toml（全局知识库配置）
pub async fn save_knowdb_config_logic(
    content: Option<String>,
    operator: Option<String>,
) -> Result<(), AppError> {
    info!("保存 knowdb 配置");
    let operator_cloned = operator.clone();
    let project_root = Setting::load().project_root;

    let result = async move {
        let content = content.unwrap_or_default();
        let written_path = write_knowdb_config(&project_root, &content)?;
        info!("knowdb 配置保存成功: path={}", written_path);

        reload_knowledge(&project_root).map_err(AppError::internal)?;

        let commit_message = "知识库改动: knowdb.toml".to_string();
        sync_to_gitea(&commit_message).await;

        handle_draft_release(operator_cloned.as_deref()).await?;

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::KnowledgeConfig,
        OperationLogAction::Update,
        OperationLogParams::new()
            .with_target_name("knowdb.toml")
            .with_field("config_only", "yes")
            .with_field("sync", "project+gitea")
            .with_field("knowledge_reload", "yes"),
        &result,
    )
    .await;

    result
}

/// 校验规则配置
pub async fn validate_rule_logic(
    rule_type: RuleType,
    file: String,
) -> Result<ValidateRuleResponse, AppError> {
    info!("规则配置校验请求: rule_type={:?}, file={}", rule_type, file);

    // 将 RuleType 映射到 CheckComponent
    let component = rule_type.to_check_component();

    // 执行组件校验
    let result = match check_component(component) {
        Ok(_) => {
            info!("规则配置校验通过: rule_type={:?}", rule_type);
            Ok(ValidateRuleResponse {
                valid: true,
                message: None,
            })
        }
        Err(e) => {
            warn!("规则配置校验失败: rule_type={:?}, error={}", rule_type, e);
            Ok(ValidateRuleResponse {
                valid: false,
                message: Some(e.to_string()),
            })
        }
    };

    write_operation_log_for_result(
        if matches!(rule_type, RuleType::Knowledge) {
            OperationLogBiz::KnowledgeConfig
        } else {
            OperationLogBiz::RuleFile
        },
        OperationLogAction::Validate,
        OperationLogParams::new()
            .with_target_name(format!("{}/{}", rule_type.as_ref(), file))
            .with_field("rule_type", rule_type.as_ref())
            .with_field("file", file),
        &result,
    )
    .await;

    result
}

fn split_wpl_virtual_file(file: &str) -> (String, WplSubFile) {
    let trimmed = file.trim().trim_matches('/');
    if trimmed.is_empty() {
        return (String::new(), WplSubFile::Parse);
    }

    if let Some((base, sub)) = trimmed.split_once('/') {
        let normalized = normalize_wpl_rule_name(base);
        if sub.eq_ignore_ascii_case(WPL_SAMPLE_FILENAME) {
            (normalized, WplSubFile::Sample)
        } else {
            (normalized, WplSubFile::Parse)
        }
    } else {
        (normalize_wpl_rule_name(trimmed), WplSubFile::Parse)
    }
}

fn format_wpl_virtual_file(base: &str, sub_file: WplSubFile) -> String {
    let normalized = normalize_wpl_rule_name(base);
    if normalized.is_empty() {
        return String::new();
    }
    match sub_file {
        WplSubFile::Parse => format!("{}/{}", normalized, WPL_PARSE_FILENAME),
        WplSubFile::Sample => format!("{}/{}", normalized, WPL_SAMPLE_FILENAME),
    }
}

fn normalize_wpl_rule_name(name: &str) -> String {
    let trimmed = name.trim().trim_matches('/');
    if let Some(stripped) = trimmed.strip_suffix(".wpl") {
        stripped.trim_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}
