//! 系统基础信息业务逻辑层。
//!
//! 提供服务存活、版本信息和前端特性配置等轻量元数据接口。

use serde::Serialize;

use crate::server::Setting;
use crate::server::setting::default_data_collect_url;

/// 版本信息响应体。
#[derive(Serialize)]
pub struct VersionResponse {
    pub wp_station: &'static str,
    pub wp_parse: &'static str,
    pub wfusion: &'static str,
}

/// 前端特性配置响应体。
#[derive(Serialize)]
pub struct FeaturesConfigResponse {
    pub data_collect_url: String,
    pub default_data_collect_url: String,
}

/// 返回服务存活探针信息。
pub fn hello_logic() -> &'static str {
    "Hello from Actix-web!"
}

/// 返回当前服务与依赖组件版本。
pub fn get_version_logic() -> VersionResponse {
    VersionResponse {
        wp_station: env!("WP_STATION_VERSION"),
        wp_parse: env!("WP_PARSE_VERSION"),
        wfusion: env!("WFUSION_VERSION"),
    }
}

/// 返回前端展示所需的特性配置项。
pub fn get_features_config_logic() -> FeaturesConfigResponse {
    let setting = Setting::load();
    FeaturesConfigResponse {
        data_collect_url: setting.features.data_collect_url,
        default_data_collect_url: default_data_collect_url(),
    }
}
