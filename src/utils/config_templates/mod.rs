//! 配置模板扫描与渲染工具。
//!
//! 模板直接从运行时 infra 仓库的 `connectors` 目录读取，再按业务规则转换成
//! source / sink 可插入的拓扑配置片段。

mod render;
mod scan;

use crate::db::RuleType;

pub use self::render::{display_name_from_file, render_config_template, template_id_from_file};
pub use self::scan::{list_config_templates, list_config_templates_from_layout};

/// 模板字段定义。
#[derive(Debug, Clone)]
pub struct ConfigTemplateField {
    pub name: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub advanced: bool,
}

/// 模板扫描结果定义。
#[derive(Debug, Clone)]
pub struct ConfigTemplateDef {
    pub scope: RuleType,
    pub template_file: String,
    pub connect: String,
    pub connector_type: String,
    pub default_enabled: Option<bool>,
    pub fields: Vec<ConfigTemplateField>,
}

/// 模板渲染结果。
#[derive(Debug, Clone)]
pub struct RenderedConfigTemplate {
    pub scope: RuleType,
    pub template_id: String,
    pub template_file: String,
    pub display_name: String,
    pub connect: String,
    pub connector_type: String,
    pub instance_name: String,
    pub required_fields: Vec<String>,
    pub inserted_fields: Vec<String>,
    pub omitted_fields: Vec<String>,
    pub warnings: Vec<String>,
    pub snippet: String,
    pub content: String,
}
