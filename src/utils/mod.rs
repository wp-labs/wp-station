//! 工具模块。
//!
//! 包含外部服务客户端、格式化器、项目管理、沙盒运行时、知识库等可复用组件。
//! 各子模块职责独立，通过本文件统一声明并选择性 re-export 对外类型。

pub mod assist_service;
pub mod common;
pub mod health_check;
pub mod knowledge;
pub mod oml;
pub mod pagination;
pub mod project;
pub mod project_check;
pub mod sandbox;
pub mod warparse_service;
pub mod wpl;

pub use assist_service::{
    AiAnalyzeRequest, AssistResultData, AssistResultResponse, AssistService, AssistServiceError,
    ManualTicketRequest,
};
pub use common::format_beijing_time;
pub use health_check::check_device_health;
pub use knowledge::{
    configured_provider_name, is_knowledge_loaded, load_knowledge, load_sqlite_knowledge,
    reload_knowledge, reload_sqlite_knowledge, sql_knowdb_list, sql_query, sql_query_rows,
    unload_knowledge,
};
pub use oml::OmlFormatter;
pub use pagination::{MemoryPaginate, PageQuery, PageResponse};
pub use project::{
    ProjectSnapshot, delete_knowledge_from_project, delete_rule_from_project, list_knowledge_dirs,
    list_rule_files, load_project_snapshot, read_knowdb_config, read_knowledge_files,
    read_rule_content, read_wpl_sample_content, resolve_project_root, touch_knowledge_in_project,
    touch_rule_in_project, write_knowdb_config, write_knowledge_files, write_rule_content,
    write_wpl_sample_content,
};
pub use warparse_service::{
    DeployCheckResult, DeployResult, OnlineStatus, ServiceError, WarpParseService,
};
pub use wpl::{ParsedField, WplFormatter, warp_check_record};
