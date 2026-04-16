// 数据库模块

pub mod assist_task;
pub mod default_rules_loader;
pub mod device;
pub mod operation_log;
pub mod performance;
pub mod pool_manager;
pub mod release;
pub mod release_target;
pub mod rule_type;
pub mod sandbox;
pub mod user;

// 导出类型定义
pub use assist_task::{
    AssistTargetRule, AssistTask, AssistTaskStatus, AssistTaskType, NewAssistTask,
};
pub use device::{Device, DeviceStatus, NewDevice, UpdateDevice};
pub use operation_log::{NewOperationLog, OperationLog};
pub use performance::{NewPerformanceTask, PerformanceResult, PerformanceTask};
pub use release::{NewRelease, Release, ReleaseStatus};
pub use release_target::{
    NewReleaseTarget, ReleaseTarget, ReleaseTargetStatus, ReleaseTargetUpdate,
    create_release_targets, find_device_previous_success_version, find_due_release_targets,
    find_release_targets_by_release, update_release_target,
};
pub use rule_type::RuleType;
pub use sandbox::{
    count_sandbox_runs_by_release, delete_sandbox_run_record, find_latest_sandbox_run,
    find_sandbox_run_by_task_id, insert_sandbox_run_record, list_sandbox_runs_by_release,
    update_sandbox_run_record,
};
pub use user::{NewUser, UpdateUser, User};

// 导出 device 函数
pub use device::{
    create_device, delete_device, find_all_devices, find_device_by_id, find_devices_by_ids,
    find_devices_page, update_device, update_device_runtime_state, update_device_status,
};

// 导出 release 函数
pub use release::{
    create_release, find_all_releases, find_draft_release, find_latest_passed_release,
    find_release_by_id, init_release, update_release_pipeline, update_release_status,
    update_release_timestamp,
};

// 导出 performance 函数
pub use performance::{
    create_performance_task, find_performance_task_by_id, get_performance_results,
};

// 导出 user 函数
pub use user::{
    change_user_password, create_user, delete_user, find_user_by_id, find_user_by_username,
    find_users_page, reset_user_password, update_user, update_user_status,
};

// 导出 operation_log 函数
pub use operation_log::{create_operation_log, find_logs_page};

// 导出 assist_task 函数
pub use assist_task::{
    create_assist_task, find_assist_task_by_id, list_assist_tasks, update_assist_task_reply,
    update_assist_task_status,
};

// 导出默认配置加载函数
pub use default_rules_loader::init_default_configs_to_project;

// 导出连接池管理函数
pub use pool_manager::{DbPool, get_pool, init_pool, is_pool_initialized, try_get_pool};

// 重新导出 sql_query 和 sql_knowdb_list 从 util::knowledge
pub use crate::utils::{sql_knowdb_list, sql_query};
