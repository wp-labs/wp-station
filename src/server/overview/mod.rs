//! 接入概览业务逻辑层。
//!
//! 聚合规则侧和运行时侧摘要，供接入概览页面直接消费。

use serde::Serialize;

use crate::error::AppError;
use crate::server::Setting;
use crate::utils::{
    SystemKind, layout_for_system, load_integration_rule_overview_from_layout,
    load_integration_runtime_overview_from_layout,
};

/// 接入概览运行时单项响应体。
#[derive(Serialize)]
pub struct IntegrationRuntimeItemResponse {
    pub key: String,
    pub title: String,
    pub connect: String,
    pub type_key: String,
    pub type_label: String,
    pub detail: String,
}

/// 接入概览运行时摘要响应体。
#[derive(Serialize)]
pub struct IntegrationRuntimeOverviewResponse {
    pub sources: Vec<IntegrationRuntimeItemResponse>,
    pub sinks: Vec<IntegrationRuntimeItemResponse>,
    pub supported_source_type_count: usize,
    pub supported_sink_type_count: usize,
}

/// 规则侧单个日志类型响应体。
#[derive(Serialize)]
pub struct IntegrationRuleLogTypeResponse {
    pub key: String,
    pub log_type_name: String,
    pub rule_keys: Vec<String>,
}

/// 规则侧平铺文件项响应体。
#[derive(Serialize)]
pub struct IntegrationRuleFlatItemResponse {
    pub key: String,
    pub name: String,
    pub rule_names: Vec<String>,
}

/// 规则侧单个设备类型响应体。
#[derive(Serialize)]
pub struct IntegrationRuleItemResponse {
    pub key: String,
    pub device_type: String,
    pub log_types: Vec<IntegrationRuleLogTypeResponse>,
}

/// 规则侧概览响应体。
#[derive(Serialize)]
pub struct IntegrationRuleOverviewResponse {
    pub system: SystemKind,
    pub items: Vec<IntegrationRuleItemResponse>,
    pub window_structures: Vec<IntegrationRuleFlatItemResponse>,
    pub association_rules: Vec<IntegrationRuleFlatItemResponse>,
    pub window_structure_count: usize,
    pub association_rule_count: usize,
}

/// 返回接入概览页面所需的输入源与输出源运行时摘要。
pub fn get_integration_runtime_overview_logic(
    system: SystemKind,
) -> Result<IntegrationRuntimeOverviewResponse, AppError> {
    let _setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let overview = load_integration_runtime_overview_from_layout(&layout)?;

    Ok(IntegrationRuntimeOverviewResponse {
        sources: overview
            .sources
            .into_iter()
            .map(|item| IntegrationRuntimeItemResponse {
                key: item.key,
                title: item.title,
                connect: item.connect,
                type_key: item.type_key,
                type_label: item.type_label,
                detail: item.detail,
            })
            .collect(),
        sinks: overview
            .sinks
            .into_iter()
            .map(|item| IntegrationRuntimeItemResponse {
                key: item.key,
                title: item.title,
                connect: item.connect,
                type_key: item.type_key,
                type_label: item.type_label,
                detail: item.detail,
            })
            .collect(),
        supported_source_type_count: overview.supported_source_type_count,
        supported_sink_type_count: overview.supported_sink_type_count,
    })
}

/// 返回接入概览页面所需的规则侧设备类型与日志类型摘要。
pub fn get_integration_rule_overview_logic(
    system: SystemKind,
) -> Result<IntegrationRuleOverviewResponse, AppError> {
    let _setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let overview = load_integration_rule_overview_from_layout(system, &layout)?;

    Ok(IntegrationRuleOverviewResponse {
        system: overview.system,
        items: overview
            .items
            .into_iter()
            .map(|item| IntegrationRuleItemResponse {
                key: item.key,
                device_type: item.device_type,
                log_types: item
                    .log_types
                    .into_iter()
                    .map(|log_type| IntegrationRuleLogTypeResponse {
                        key: log_type.key,
                        log_type_name: log_type.log_type_name,
                        rule_keys: log_type.rule_keys,
                    })
                    .collect(),
            })
            .collect(),
        window_structures: overview
            .window_structures
            .into_iter()
            .map(|item| IntegrationRuleFlatItemResponse {
                key: item.key,
                name: item.name,
                rule_names: item.rule_names,
            })
            .collect(),
        association_rules: overview
            .association_rules
            .into_iter()
            .map(|item| IntegrationRuleFlatItemResponse {
                key: item.key,
                name: item.name,
                rule_names: item.rule_names,
            })
            .collect(),
        window_structure_count: overview.window_structure_count,
        association_rule_count: overview.association_rule_count,
    })
}
