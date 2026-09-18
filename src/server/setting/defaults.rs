//! 配置默认值辅助。

use super::RepoStartupStrategy;

pub(super) fn default_database_host() -> String {
    "localhost".to_string()
}

pub(super) fn default_database_port() -> u16 {
    5432
}

pub(super) fn default_database_name() -> String {
    "wp-station".to_string()
}

pub(super) fn default_database_username() -> String {
    "postgres".to_string()
}

pub(super) fn default_database_password() -> String {
    "123456".to_string()
}

pub(super) fn default_max_connections() -> u32 {
    10
}

pub(super) fn default_min_connections() -> u32 {
    2
}

pub(super) fn default_connect_timeout() -> u64 {
    30
}

pub(super) fn default_idle_timeout() -> u64 {
    600
}

pub(super) fn default_ssl_mode() -> String {
    "prefer".to_string()
}

pub(super) fn default_repo_startup_strategy() -> RepoStartupStrategy {
    RepoStartupStrategy::Gitea
}

pub(super) fn default_poll_interval() -> u64 {
    5
}

pub(super) fn default_poll_timeout() -> u64 {
    300
}

pub(super) fn default_max_retries_admin_api() -> u32 {
    60
}

pub(crate) fn default_data_collect_url() -> String {
    "http://localhost:18080/wp-monitor".to_string()
}
