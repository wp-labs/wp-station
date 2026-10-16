//! 系统相关工具。
//!
//! 收敛固定双系统的目录布局和外部服务适配器：
//! - `layout` 负责 `wparse` / `wfusion` 的仓库路径映射
//! - `client_wparse` 负责 `wparse` 设备侧服务调用
//! - `client_wfusion` 负责 `wfusion` 设备侧健康检查与发布调用

pub mod client_wfusion;
pub mod client_wparse;
pub mod layout;

pub use client_wfusion::{
    DeviceHealthSnapshot, PublishPayload, WfusionService,
    not_implemented as wfusion_not_implemented,
};
pub use client_wparse::{
    DeployCheckResult, DeployResult, OnlineStatus, ServiceError, WarpParseService,
};
pub use layout::{
    ProjectArea, SystemKind, SystemProjectLayout, all_system_layouts, layout_for_system, repo_name,
};
