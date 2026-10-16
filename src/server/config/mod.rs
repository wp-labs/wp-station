//! 配置管理业务逻辑层。
//!
//! 统一承接 parse/source/sink/topology/connectors 等配置读写，
//! 并在入口处按 `system` 分发到对应目录。

mod read;
mod templates;
mod write;

use crate::db::RuleType;
use crate::server::RepoLayout;
use crate::utils::{
    SystemKind,
    display::{
        connector_file_sort_order, fallback_connector_display, fallback_sink_display,
        sink_file_sort_order,
    },
    layout_for_system,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;

pub use self::read::{get_config_files_logic, get_config_logic};
pub use self::templates::{
    ConfigTemplateFieldItem, ConfigTemplateItem, ConfigTemplateListResponse, ConfigTemplateQuery,
    RenderConfigTemplateRequest, RenderConfigTemplateResponse, get_config_templates_logic,
    render_config_template_logic,
};
pub use self::write::{create_config_file_logic, delete_config_file_logic, save_config_logic};

// ============ 请求参数结构体 ============

/// 配置文件列表查询参数。
#[derive(Deserialize)]
pub struct ConfigFilesQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub keyword: Option<String>,
}

/// 配置内容查询参数。
#[derive(Deserialize)]
pub struct ConfigQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: Option<String>,
}

/// 保存配置请求。
#[derive(Deserialize)]
pub struct SaveConfigRequest {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
    pub content: String,
}

/// 创建配置文件请求。
#[derive(Deserialize)]
pub struct CreateConfigFileRequest {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
    pub display_name: Option<String>,
}

/// 删除配置文件查询参数。
#[derive(Deserialize)]
pub struct DeleteConfigFileQuery {
    pub system: SystemKind,
    pub rule_type: RuleType,
    pub file: String,
}

// ============ 响应结构体 ============

/// 配置文件列表项。
#[derive(Serialize)]
pub struct ConfigFileItem {
    pub file: String,
    pub display_name: Option<String>,
    pub sort_order: Option<usize>,
    pub file_size: Option<i32>,
    pub last_modified: Option<String>,
}

/// 配置文件列表响应。
#[derive(Serialize)]
pub struct ConfigFilesResponse {
    pub items: Vec<ConfigFileItem>,
    pub default_file: Option<String>,
}

/// 单个配置文件内容响应。
#[derive(Serialize)]
pub struct ConfigItem {
    #[serde(rename = "rule_type")]
    pub rule_type: RuleType,
    pub file: String,
    pub display_name: Option<String>,
    pub content: Option<String>,
    pub last_modified: Option<String>,
}

/// 仅返回成功标志的通用响应。
#[derive(Serialize)]
pub struct SimpleResult {
    pub success: bool,
}

// ============ 业务逻辑函数 ============

/// 根据系统解析当前请求对应的双仓库布局。
fn repo_layout(system: SystemKind) -> RepoLayout {
    layout_for_system(system).as_repo_layout()
}

/// 推导配置文件兜底展示名，避免前端直接展示文件名。
fn fallback_display_name(rule_type: RuleType, file_name: &str) -> Option<String> {
    match rule_type {
        RuleType::Sink => fallback_sink_display(file_name).map(|label| label.to_string()),
        RuleType::SourceConnect | RuleType::SinkConnect => {
            fallback_connector_display(file_name).map(|label| label.to_string())
        }
        _ => None,
    }
}

/// 计算配置文件排序权重，保证列表展示顺序稳定。
fn file_sort_order(rule_type: RuleType, file_name: &str) -> Option<usize> {
    match rule_type {
        RuleType::Sink => sink_file_sort_order(file_name),
        RuleType::SourceConnect | RuleType::SinkConnect => connector_file_sort_order(file_name),
        _ => None,
    }
}

/// 返回默认选中的配置文件。
fn default_file_for_rule_type(rule_type: RuleType, items: &[ConfigFileItem]) -> Option<String> {
    match rule_type {
        RuleType::Parse | RuleType::Source => items.first().map(|item| item.file.clone()),
        _ => None,
    }
}

/// 将文件系统时间转换为统一的 RFC3339 字符串。
fn system_time_to_rfc3339(time: SystemTime) -> String {
    let datetime: DateTime<Utc> = time.into();
    datetime.to_rfc3339()
}
