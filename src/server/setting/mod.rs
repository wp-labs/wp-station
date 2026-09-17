//! 应用配置与工作区路径模型。
//!
//! 负责加载 `config/config.toml`、提供默认值，
//! 并集中定义服务运行时使用的设置结构体。

mod defaults;

use config::{Config, File};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use strum::{AsRefStr, Display, EnumString};

use self::defaults::{
    default_connect_timeout, default_database_host, default_database_name,
    default_database_password, default_database_port, default_database_username,
    default_idle_timeout, default_max_connections, default_max_retries_admin_api,
    default_min_connections, default_poll_interval, default_poll_timeout,
    default_repo_startup_strategy, default_ssl_mode,
};
use crate::utils::{SystemKind, layout_for_system};

pub(crate) use self::defaults::default_data_collect_url;

/// 日志输出配置。
#[derive(Debug, Deserialize, Clone)]
pub struct LogConf {
    pub level: String,
    pub output: String,
    pub output_path: String,
}

impl Default for LogConf {
    fn default() -> Self {
        LogConf {
            level: "debug".to_string(),
            output: "Console".to_string(),
            output_path: "./logs/".to_string(),
        }
    }
}

/// Web 服务监听配置。
#[derive(Debug, Deserialize, Clone)]
pub struct WebConf {
    pub host: String,
    pub port: u16,
}

impl Default for WebConf {
    fn default() -> Self {
        WebConf {
            host: "0.0.0.0".to_string(),
            port: 8081,
        }
    }
}

/// 数据库连接配置。
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConf {
    #[serde(default)]
    pub url: String,
    #[serde(default = "default_database_host")]
    pub host: String,
    #[serde(default = "default_database_port")]
    pub port: u16,
    #[serde(default = "default_database_name")]
    pub name: String,
    #[serde(default = "default_database_username")]
    pub username: String,
    #[serde(default = "default_database_password")]
    pub password: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
    #[serde(default = "default_min_connections")]
    pub min_connections: u32,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout: u64,
    #[serde(default = "default_ssl_mode")]
    pub ssl_mode: String,
}

/// 数据库类型枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum DatabaseKind {
    Postgres,
    Sqlite,
}

impl Default for DatabaseConf {
    fn default() -> Self {
        DatabaseConf {
            url: String::new(),
            host: default_database_host(),
            port: default_database_port(),
            name: default_database_name(),
            username: default_database_username(),
            password: default_database_password(),
            max_connections: default_max_connections(),
            min_connections: default_min_connections(),
            connect_timeout: default_connect_timeout(),
            idle_timeout: default_idle_timeout(),
            ssl_mode: default_ssl_mode(),
        }
    }
}

impl DatabaseConf {
    /// 返回数据库类型。
    pub fn database_kind(&self) -> DatabaseKind {
        let url = self.url.trim().to_ascii_lowercase();
        if url.starts_with("sqlite:") {
            DatabaseKind::Sqlite
        } else {
            DatabaseKind::Postgres
        }
    }

    /// 生成连接字符串
    pub fn connection_string(&self) -> String {
        if !self.url.trim().is_empty() {
            let url = self.url.trim();
            return if matches!(self.database_kind(), DatabaseKind::Sqlite) {
                self.resolve_sqlite_url(url)
            } else {
                url.to_string()
            };
        }
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.name
        )
    }

    /// 生成连接字符串（带 SSL 模式）
    pub fn connection_string_with_options(&self) -> String {
        if !self.url.trim().is_empty() {
            return self.connection_string();
        }
        match self.database_kind() {
            DatabaseKind::Sqlite => self.connection_string(),
            DatabaseKind::Postgres => {
                format!("{}?sslmode={}", self.connection_string(), self.ssl_mode)
            }
        }
    }

    /// 将 SQLite 相对路径固定到应用工作区，避免连接池新建连接时随进程 cwd 变化。
    fn resolve_sqlite_url(&self, url: &str) -> String {
        let (base, suffix) = url
            .find(['?', '#'])
            .map(|index| (&url[..index], &url[index..]))
            .unwrap_or((url, ""));
        let (prefix, raw_path) = if let Some(path) = base.strip_prefix("sqlite://") {
            ("sqlite://", path)
        } else if let Some(path) = base.strip_prefix("sqlite:") {
            ("sqlite:", path)
        } else {
            return url.to_string();
        };

        if raw_path.is_empty() || raw_path == ":memory:" {
            return url.to_string();
        }

        let path = Path::new(raw_path);
        if path.is_absolute() {
            return url.to_string();
        }

        let absolute_path = Setting::workspace_root().join(path);
        format!("{}{}{}", prefix, absolute_path.display(), suffix)
    }

    /// 生成用于日志输出的脱敏数据库描述
    pub fn safe_summary(&self) -> String {
        match self.database_kind() {
            DatabaseKind::Sqlite => {
                let sqlite_url = self.connection_string();
                let path = sqlite_url
                    .trim_start_matches("sqlite://")
                    .trim_start_matches("sqlite:");
                format!("sqlite:{}", path)
            }
            DatabaseKind::Postgres => {
                if !self.url.trim().is_empty() {
                    "postgres:url(provided)".to_string()
                } else {
                    format!(
                        "{}@{}:{}/{}?sslmode={}",
                        self.username, self.host, self.port, self.name, self.ssl_mode
                    )
                }
            }
        }
    }
}

/// Gitea 访问配置。
#[derive(Debug, Deserialize, Clone)]
pub struct GiteaConf {
    pub base_url: String,
    pub username: String,
    pub password: String,
    #[serde(default = "default_repo_startup_strategy")]
    pub repo_startup_strategy: RepoStartupStrategy,
}

/// Station 启动时本地双仓库与 Gitea 远端仓库的冲突处理策略。
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepoStartupStrategy {
    /// 强制以 Gitea 为准：启动时用远端仓库覆盖本地目录。
    Gitea,
    /// 强制以本地为准：启动时用本地仓库覆盖远端 main 分支。
    Local,
}

impl Default for GiteaConf {
    fn default() -> Self {
        GiteaConf {
            base_url: "http://127.0.0.1:3000".to_string(),
            username: "gitea".to_string(),
            password: "123456".to_string(),
            repo_startup_strategy: default_repo_startup_strategy(),
        }
    }
}

/// Assist 外部服务配置。
#[derive(Debug, Deserialize, Clone, Default)]
pub struct AssistConf {
    pub base_url: String,
    #[serde(default)]
    pub callback_base_url: String,
}

/// 设备管理接口配置。
///
/// 当前由 `wparse` / `wfusion` 共用，用于：
/// - 访问协议（HTTP/HTTPS）
/// - TLS CA 证书
/// - 发布轮询节奏
#[derive(Debug, Deserialize, Clone)]
pub struct AdminApiConf {
    pub enabled: bool,
    #[serde(default)]
    pub ca_file: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
    #[serde(default = "default_poll_timeout")]
    pub poll_timeout_seconds: u64,
    #[serde(default = "default_max_retries_admin_api")]
    pub max_retries: u32,
}

/// 前端特性开关相关配置。
#[derive(Debug, Deserialize, Clone)]
pub struct FeaturesConf {
    #[serde(default = "default_data_collect_url")]
    pub data_collect_url: String,
}

impl Default for FeaturesConf {
    fn default() -> Self {
        FeaturesConf {
            data_collect_url: default_data_collect_url(),
        }
    }
}

/// 应用顶层运行配置。
#[derive(Debug, Deserialize, Clone)]
pub struct Setting {
    pub log: LogConf,
    pub web: WebConf,
    pub database: DatabaseConf,
    #[serde(default)]
    pub gitea: GiteaConf,
    #[serde(default)]
    pub assist: AssistConf,
    pub admin_api: AdminApiConf,
    #[serde(default)]
    pub features: FeaturesConf,
}

/// 单个系统的 models/infra 仓库根目录，以及共享 connectors 仓库根目录。
#[derive(Debug, Clone)]
pub struct RepoLayout {
    pub models_root: PathBuf,
    pub infra_root: PathBuf,
    pub connectors_root: PathBuf,
}

impl Default for Setting {
    fn default() -> Self {
        Setting {
            log: LogConf::default(),
            web: WebConf::default(),
            database: DatabaseConf::default(),
            gitea: GiteaConf::default(),
            assist: AssistConf::default(),
            admin_api: AdminApiConf {
                enabled: true,
                ca_file: String::new(),
                poll_interval_seconds: default_poll_interval(),
                poll_timeout_seconds: default_poll_timeout(),
                max_retries: default_max_retries_admin_api(),
            },
            features: FeaturesConf::default(),
        }
    }
}

impl Setting {
    /// 从配置文件和环境变量加载运行配置。
    pub fn load() -> Self {
        static SETTING: OnceLock<Setting> = OnceLock::new();

        SETTING
            .get_or_init(|| {
                let config_path = "config/config.toml";

                if !Path::new(&config_path).exists() {
                    panic!(
                        "配置文件 {} 不存在，请先创建配置文件再启动服务",
                        config_path
                    );
                }

                // 环境变量可覆盖配置文件中的任意字段
                // 格式：WP_STATION__DATABASE__HOST、WP_STATION__DATABASE__NAME 等
                // 前缀与首层 key、各层 key 之间统一用双下划线 __ 分隔
                let builder = Config::builder()
                    .add_source(File::with_name(config_path))
                    .add_source(
                        config::Environment::with_prefix("WP_STATION")
                            .separator("__")
                            .try_parsing(true),
                    );

                let config = builder.build().unwrap_or_else(|err| {
                    panic!("读取配置文件 {} 失败: {}", config_path, err);
                });

                config.try_deserialize().unwrap_or_else(|err| {
                    panic!("解析配置文件 {} 失败: {}", config_path, err);
                })
            })
            .clone()
    }

    /// 获取工作空间根目录（配置文件所在目录的父目录）
    pub fn workspace_root() -> &'static std::path::PathBuf {
        static WORKSPACE_ROOT: OnceLock<std::path::PathBuf> = OnceLock::new();

        WORKSPACE_ROOT.get_or_init(|| {
            // 在服务启动时保存当前工作目录
            std::env::current_dir().unwrap_or_else(|err| panic!("无法获取当前工作目录: {}", err))
        })
    }

    /// 返回 `wparse` 的固定双仓库布局。
    pub fn wparse_layout(&self) -> RepoLayout {
        layout_for_system(SystemKind::Wparse).as_repo_layout()
    }
}
