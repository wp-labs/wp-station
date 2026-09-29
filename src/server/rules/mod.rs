//! 规则配置业务逻辑层。
//!
//! 统一承接 `wpl / oml / knowledge` 等规则读写与校验。
//! 双系统改造后，规则入口显式带 `system`，内部再决定目录和校验实现。

mod files;
mod knowledge;
mod validate;

use crate::constants::project::{FILE_WPL_PARSE, FILE_WPL_SAMPLE};
use crate::db::RuleType;
use crate::server::RepoLayout;
use crate::utils::pagination::PageQuery;
use crate::utils::{SystemKind, layout_for_system};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

pub use self::files::{
    create_rule_file_logic, delete_rule_file_logic, get_rule_content_logic, get_rule_files_logic,
    save_rule_logic,
};
pub use self::knowledge::{
    get_knowdb_config_logic, save_knowdb_config_logic, save_knowledge_rule_logic,
};
pub use self::validate::validate_rule_logic;

/// WPL 规则目录下的特殊子文件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WplSubFile {
    Parse,
    Sample,
}

// ============ 请求参数结构体 ============

/// 规则文件列表查询参数。
#[derive(Deserialize)]
pub struct RuleFilesQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub keyword: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

/// 规则内容查询参数。
#[derive(Deserialize)]
pub struct RuleContentQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: Option<String>,
}

/// 创建规则文件请求。
#[derive(Deserialize)]
pub struct CreateRuleFileRequest {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
}

/// 删除规则文件查询参数。
#[derive(Deserialize)]
pub struct DeleteRuleFileQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
}

/// 保存规则内容请求。
#[derive(Deserialize)]
pub struct SaveRuleRequest {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
}

/// 保存知识库目录请求。
#[derive(Deserialize)]
pub struct SaveKnowledgeRuleRequest {
    pub system: SystemKind,
    pub file: String,
    pub config: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data: Option<String>,
}

/// 保存 knowdb 主配置请求。
#[derive(Deserialize)]
pub struct SaveKnowdbConfigRequest {
    pub system: SystemKind,
    pub content: Option<String>,
}

/// 规则校验请求。
#[derive(Deserialize)]
pub struct ValidateRuleRequest {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
}

// ============ 响应结构体定义 ============

/// 普通规则内容响应。
#[derive(Serialize)]
pub struct RuleContentResponse {
    pub rule_type: RuleType,
    pub file: String,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

/// 规则文件列表项。
#[derive(Serialize)]
pub struct RuleFileItem {
    pub file: String,
    pub display_name: Option<String>,
}

/// 规则文件列表补充元信息。
#[derive(Serialize)]
pub struct RuleFilesMeta {
    pub wpl_parse_file: String,
    pub wpl_sample_file: String,
    pub knowledge_config_file: String,
}

/// 知识库目录内容响应。
#[derive(Serialize)]
pub struct KnowledgeRuleContentResponse {
    pub rule_type: RuleType,
    pub file: String,
    pub config: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data: Option<String>,
}

/// knowdb 主配置响应。
#[derive(Serialize)]
pub struct KnowdbConfigResponse {
    pub file: String,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

/// 规则校验结果响应。
#[derive(Serialize)]
pub struct ValidateRuleResponse {
    pub valid: bool,
    pub message: Option<String>,
    pub details: Vec<String>,
}

/// 规则文件分页响应。
#[derive(Serialize)]
pub struct RuleFilesResponse {
    pub items: Vec<RuleFileItem>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
    pub meta: RuleFilesMeta,
}

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

fn repo_layout(system: SystemKind) -> RepoLayout {
    layout_for_system(system).as_repo_layout()
}

fn split_wpl_virtual_file(file: &str) -> (String, WplSubFile) {
    let trimmed = file.trim().trim_matches('/');
    if trimmed.is_empty() {
        return (String::new(), WplSubFile::Parse);
    }

    if let Some((base, sub)) = trimmed.split_once('/') {
        let normalized = normalize_wpl_rule_name(base);
        if sub.eq_ignore_ascii_case(FILE_WPL_SAMPLE) {
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
        WplSubFile::Parse => format!("{}/{}", normalized, FILE_WPL_PARSE),
        WplSubFile::Sample => format!("{}/{}", normalized, FILE_WPL_SAMPLE),
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
