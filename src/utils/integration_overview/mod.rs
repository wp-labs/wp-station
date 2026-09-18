//! 接入概览运行时摘要提取工具。
//!
//! 负责两类概览数据：
//! - 运行时输入源 / 输出源摘要
//! - WPL 规则中的设备类型 / 日志类型摘要
//!
//! 目录拆分后，运行时扫描与规则解析分别下沉到子模块。

mod detail;
mod rules;
mod runtime;

pub use self::rules::load_integration_rule_overview_from_layout;
pub use self::runtime::load_integration_runtime_overview_from_layout;
use crate::utils::SystemKind;

/// 运行时接入概览单项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuntimeItem {
    pub key: String,
    pub title: String,
    pub connect: String,
    pub type_key: String,
    pub type_label: String,
    pub detail: String,
}

/// 运行时接入概览摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuntimeOverview {
    pub sources: Vec<IntegrationRuntimeItem>,
    pub sinks: Vec<IntegrationRuntimeItem>,
    pub supported_source_type_count: usize,
    pub supported_sink_type_count: usize,
}

/// 规则侧单个日志类型摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuleLogType {
    pub key: String,
    pub log_type_name: String,
    pub rule_keys: Vec<String>,
}

/// 规则侧平铺文件项摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuleFlatItem {
    pub key: String,
    pub name: String,
    pub rule_names: Vec<String>,
}

/// 规则侧单个设备类型摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuleItem {
    pub key: String,
    pub device_type: String,
    pub log_types: Vec<IntegrationRuleLogType>,
}

/// 规则侧接入概览摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationRuleOverview {
    pub system: SystemKind,
    pub items: Vec<IntegrationRuleItem>,
    pub window_structures: Vec<IntegrationRuleFlatItem>,
    pub association_rules: Vec<IntegrationRuleFlatItem>,
    pub window_structure_count: usize,
    pub association_rule_count: usize,
}
