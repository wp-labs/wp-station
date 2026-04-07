// 数据库实体定义

pub mod device;
pub mod rule_config;
pub mod knowledge_config;
pub mod release;
pub mod release_target;
pub mod performance;
pub mod user;
pub mod operation_log;
pub mod assist_task;
pub mod sandbox_run;

pub use device::Entity as Device;
pub use rule_config::Entity as RuleConfig;
pub use knowledge_config::Entity as KnowledgeConfig;
pub use release::Entity as Release;
pub use release_target::Entity as ReleaseTarget;
pub use performance::Entity as PerformanceTask;
pub use user::Entity as User;
pub use operation_log::Entity as OperationLog;
pub use assist_task::Entity as AssistTask;
pub use sandbox_run::Entity as SandboxRun;
