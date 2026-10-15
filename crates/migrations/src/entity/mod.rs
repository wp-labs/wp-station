// 数据库实体定义

pub mod assist_task;
pub mod device;
pub mod release;
pub mod release_restore_job;
pub mod release_target;
pub mod sandbox_run;
pub mod user;

pub use assist_task::Entity as AssistTask;
pub use device::Entity as Device;
pub use release::Entity as Release;
pub use release_restore_job::Entity as ReleaseRestoreJob;
pub use release_target::Entity as ReleaseTarget;
pub use sandbox_run::Entity as SandboxRun;
pub use user::Entity as User;
