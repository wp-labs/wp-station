//! 服务端业务模块总入口。
//!
//! 统一声明各领域模块，并对外 re-export 业务层公开类型与逻辑函数，
//! 让 `api`、启动装配和测试代码可以按稳定入口引用。

pub mod app;
pub mod assist_task;
pub mod config;
pub mod debug;
pub mod device;
pub mod knowledge_query;
pub mod meta;
pub mod overview;
pub mod project;
pub mod release;
pub mod rules;
pub mod sandbox;
pub mod setting;
pub mod sync;
pub mod user;

pub use app::start;
pub use assist_task::{
    AssistListQuery, AssistListResponse, AssistReplyRequest, AssistSubmitRequest,
    AssistSubmitResponse, AssistTaskDetail, assist_cancel_logic, assist_get_logic,
    assist_list_logic, assist_reply_logic, assist_submit_logic,
};
pub use config::{
    ConfigFilesQuery, ConfigQuery, ConfigTemplateFieldItem, ConfigTemplateItem,
    ConfigTemplateListResponse, ConfigTemplateQuery, CreateConfigFileRequest,
    DeleteConfigFileQuery, RenderConfigTemplateRequest, RenderConfigTemplateResponse,
    SaveConfigRequest, create_config_file_logic, delete_config_file_logic, get_config_files_logic,
    get_config_logic, get_config_templates_logic, render_config_template_logic, save_config_logic,
};
pub use debug::{
    DebugExample, DebugFormatKind, DebugKnowledgeQueryRequest, DebugKnowledgeStatusQuery,
    DebugParseRequest, DebugTransformRequest, DebugWfusionRuleEditorParseRequest, SharedRecord,
    debug_knowledge_query_logic, debug_knowledge_status_logic, debug_parse_logic,
    debug_transform_logic, debug_wfusion_rule_editor_parse_logic, format_code_logic,
    load_debug_examples, oml_format_logic, toml_format_logic, wfg_format_logic, wfl_format_logic,
    wfs_format_logic, wpl_format_logic,
};
pub use device::{
    CreateDeviceRequest, DeviceCreated, DeviceListQuery, DeviceRefreshResult, DeviceUpdateResult,
    UpdateDeviceRequest, create_device_logic, delete_device_logic, list_devices_logic,
    list_online_devices_logic, refresh_device_status_logic, update_device_logic,
};
pub use knowledge_query::{KnowdbQuery, KnowledgeDbListQuery, get_db_list_logic, query_logic};
pub use meta::{
    FeaturesConfigResponse, VersionResponse, get_features_config_logic, get_version_logic,
    hello_logic,
};
pub use overview::{
    IntegrationRuleItemResponse, IntegrationRuleLogTypeResponse, IntegrationRuleOverviewResponse,
    IntegrationRuntimeItemResponse, IntegrationRuntimeOverviewResponse,
    get_integration_rule_overview_logic, get_integration_runtime_overview_logic,
};
pub use project::{
    ProjectArchiveConfirmRequest, ProjectArchiveExport, ProjectArchivePreviewResponse,
    confirm_project_archive_import_logic, export_project_archive_logic,
    import_project_from_files_logic, preview_project_archive_logic,
};
pub use release::runner::spawn_release_task_runner;
pub use release::restore_runner::spawn_restore_task_runner;
pub use release::{
    CreateReleaseRequest, ReleaseActionRequest, ReleaseListQuery, ReleaseRestoreRequest,
    ReleaseTargetActionRequest, create_release_logic, create_restore_job_logic,
    get_release_detail_logic, get_release_diff_logic, get_restore_job_logic, list_releases_logic,
    publish_release_logic, refresh_draft_release_logic, retry_release_logic,
    rollback_release_logic, validate_release_logic,
};
pub use rules::{
    CreateRuleFileRequest, DeleteRuleFileQuery, KnowdbConfigResponse, RuleContentQuery,
    RuleFileItem, RuleFilesQuery, RuleFilesResponse, SaveKnowdbConfigRequest,
    SaveKnowledgeRuleRequest, SaveRuleRequest, ValidateRuleRequest, create_rule_file_logic,
    delete_rule_file_logic, get_knowdb_config_logic, get_rule_content_logic, get_rule_files_logic,
    save_knowdb_config_logic, save_knowledge_rule_logic, save_rule_logic, validate_rule_logic,
};
pub use sandbox::{
    CreateSandboxRunRequest, CreateSandboxRunResponse, FileOverride, RunOptions,
    SandboxHistoryItem, SandboxHistoryResponse, SandboxLatestResponse, SandboxRun, SandboxStage,
    SandboxStageLogResponse, SandboxState, StageResult, StageStatus, TaskStatus,
    create_sandbox_run_logic, get_latest_sandbox_run_logic, get_sandbox_run_logic,
    get_stage_logs_logic, list_sandbox_history_logic, stop_sandbox_run_logic,
};
pub use setting::{
    AssistConf, DatabaseConf, DatabaseKind, FeaturesConf, LogConf, RepoLayout, RepoStartupStrategy,
    Setting, WebConf,
};
pub use sync::push_and_tag_release;
pub use user::{
    ChangePasswordRequest, CreateUserRequest, LoginRequest, LoginResponse, ResetPasswordRequest,
    ResetPasswordResponse, UpdateUserRequest, UpdateUserStatusRequest, UserCreated, UserListQuery,
    change_password_logic, create_user_logic, delete_user_logic, list_users_logic, login_logic,
    reset_password_logic, update_user_logic, update_user_status_logic,
};
