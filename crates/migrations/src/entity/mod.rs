// 数据库实体定义

pub mod device;
pub mod release;
pub mod release_target;
pub mod release_restore_job;
pub mod user;
pub mod assist_task;
pub mod sandbox_run;

pub use device::Entity as Device;
pub use release::Entity as Release;
pub use release_target::Entity as ReleaseTarget;
pub use release_restore_job::Entity as ReleaseRestoreJob;
pub use user::Entity as User;
pub use assist_task::Entity as AssistTask;
pub use sandbox_run::Entity as SandboxRun;
