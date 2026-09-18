//! HTTP API 模块总入口。
//!
//! 统一声明并 re-export 所有路由处理函数。

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
pub mod user;

pub use assist_task::{assist_cancel, assist_get, assist_list, assist_reply, assist_submit};
pub use config::{
    create_config_file, delete_config_file, get_config, get_config_files, get_config_templates,
    render_config_template, save_config,
};
pub use debug::{
    debug_examples, debug_knowledge_query, debug_knowledge_status, debug_parse, debug_transform,
    debug_wfusion_rule_editor_parse, oml_format, toml_format, wfg_format, wfl_format, wfs_format,
    wpl_format,
};
pub use device::{
    create_device, delete_device, list_devices, list_online_devices, refresh_device_status,
    update_device,
};
pub use knowledge_query::{get_db_list, query};
pub use meta::{get_features_config, get_version, hello};
pub use overview::{get_integration_rule_overview, get_integration_runtime_overview};
pub use project::{
    confirm_project_archive_import, export_project_archive, import_project_archive,
    import_project_from_files,
};
pub use release::{
    create_release, get_release_detail, get_release_diff, list_releases, publish_release,
    get_release_restore, restore_release, retry_release, rollback_release, validate_release,
};
pub use rules::{
    create_rule_file, delete_rule_file, get_knowdb_config, get_rule_content, get_rule_files,
    save_knowdb_config, save_knowledge_rule, save_rule, validate_rule,
};
pub use sandbox::{
    create_sandbox_run, get_latest_sandbox_run, get_sandbox_run, get_sandbox_stage_logs,
    list_sandbox_history, stop_sandbox_run,
};
pub use user::{
    change_user_password, create_user, delete_user, list_users, login, reset_user_password,
    update_user, update_user_status,
};
