//! 工具模块。
//!
//! 包含外部服务客户端、格式化器、项目管理、沙盒运行时、知识库等可复用组件。
//! 各子模块职责独立，通过本文件统一声明并选择性 re-export 对外类型。
//! 双系统改造相关的系统布局和系统适配器统一收敛在 `system` 目录下。

pub mod assist_service;
pub mod config_templates;
pub mod display;
pub mod health_check;
pub mod integration_overview;
pub mod knowledge;
pub mod oml;
pub mod pagination;
pub mod project_check;
pub mod project_fs;
pub mod sandbox;
pub mod system;
pub mod tree_sitter_assets;
mod tree_sitter_sync_manifest;
pub mod wpl;

pub use assist_service::{
    AiAnalyzeRequest, AssistResultData, AssistResultResponse, AssistService, AssistServiceError,
    ManualTicketRequest,
};
pub use config_templates::{
    ConfigTemplateDef, ConfigTemplateField, RenderedConfigTemplate, display_name_from_file,
    list_config_templates, list_config_templates_from_layout, render_config_template,
    template_id_from_file,
};
pub use display::format_beijing_time;
pub use health_check::{
    DeviceHealthCheckResult, check_device_health, check_device_health_with_detail,
};
pub use integration_overview::{
    IntegrationRuleItem, IntegrationRuleLogType, IntegrationRuleOverview, IntegrationRuntimeItem,
    IntegrationRuntimeOverview, load_integration_rule_overview_from_layout,
    load_integration_runtime_overview_from_layout,
};
pub use knowledge::{
    configured_provider_names, is_knowledge_loaded, load_knowledge, load_sqlite_knowledge,
    reload_knowledge, reload_sqlite_knowledge, should_reload_knowledge_source, sql_knowdb_list,
    sql_query, sql_query_rows, sql_query_rows_for, unload_knowledge,
};
pub use oml::OmlFormatter;
pub use pagination::{MemoryPaginate, PageQuery, PageResponse};
pub use project_fs::{
    ProjectSnapshot, compose_repo_layout_into, delete_knowledge_from_project,
    delete_rule_from_project, init_default_configs_to_infra,
    init_default_configs_to_infra_for_system, init_default_configs_to_models,
    init_default_configs_to_models_for_system,
    list_knowledge_dirs, list_rule_files, load_project_snapshot,
    load_project_snapshot_from_repo_layout, read_knowdb_config, read_knowledge_files,
    read_rule_content, read_wpl_sample_content, resolve_dir_for_rule, resolve_project_root,
    runtime_default_configs_dir, touch_knowledge_in_project, touch_rule_in_project,
    write_knowdb_config, write_knowledge_files, write_rule_content,
    write_rule_content_in_project_dir, write_wpl_sample_content,
};
pub use system::{
    DeployCheckResult, DeployResult, DeviceHealthSnapshot, OnlineStatus, ProjectArea,
    PublishPayload, ServiceError, SystemKind, SystemProjectLayout, WarpParseService,
    WfusionService, all_system_layouts, layout_for_system, repo_name, wfusion_not_implemented,
};
pub use tree_sitter_assets::{
    read_runtime_asset_from_public, sync_tree_sitter_assets_for_dev_start,
};
pub use wpl::{ParsedField, WplFormatter, warp_check_record};
