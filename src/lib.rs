//! `wp-station` 库入口。
//!
//! 对外暴露后端模块和少量高频公共类型。

#[macro_use]
extern crate tracing;

pub mod api;
pub mod constants;
pub mod db;
pub mod error;
pub mod server;
pub mod utils;

// 重新导出常用模块
pub use db::{DbPool, Device, DeviceStatus, NewDevice, NewRelease, Release, RuleType, init_pool};
pub use server::{DatabaseConf, DatabaseKind, Setting, WebConf};
pub use utils::{
    ParsedField, init_default_configs_to_infra, init_default_configs_to_models, warp_check_record,
};
