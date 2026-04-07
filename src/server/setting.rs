use config::{Config, File};
use serde::Deserialize;
use std::path::Path;
use std::sync::OnceLock;

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

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConf {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub username: String,
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

fn default_max_connections() -> u32 {
    10
}

fn default_min_connections() -> u32 {
    2
}

fn default_connect_timeout() -> u64 {
    30
}

fn default_idle_timeout() -> u64 {
    600
}

fn default_ssl_mode() -> String {
    "prefer".to_string()
}

impl Default for DatabaseConf {
    fn default() -> Self {
        DatabaseConf {
            host: "localhost".to_string(),
            port: 5432,
            name: "wp-station".to_string(),
            username: "postgres".to_string(),
            password: "123456".to_string(),
            max_connections: 10,
            min_connections: 2,
            connect_timeout: 30,
            idle_timeout: 600,
            ssl_mode: "prefer".to_string(),
        }
    }
}

impl DatabaseConf {
    /// 生成连接字符串
    pub fn connection_string(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, self.password, self.host, self.port, self.name
        )
    }

    /// 生成连接字符串（带 SSL 模式）
    pub fn connection_string_with_options(&self) -> String {
        format!("{}?sslmode={}", self.connection_string(), self.ssl_mode)
    }

    /// 生成用于日志输出的脱敏数据库描述
    pub fn safe_summary(&self) -> String {
        format!(
            "{}@{}:{}/{}?sslmode={}",
            self.username, self.host, self.port, self.name, self.ssl_mode
        )
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct GiteaConf {
    pub base_url: String,
    pub username: String,
    pub password: String,
}

impl Default for GiteaConf {
    fn default() -> Self {
        GiteaConf {
            base_url: "http://127.0.0.1:3000".to_string(),
            username: "gitea".to_string(),
            password: "123456".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct AssistConf {
    pub base_url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct WarparseConf {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_deploy_path")]
    pub deploy_path: String,
    #[serde(default = "default_status_path")]
    pub status_path: String,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
    #[serde(default = "default_poll_timeout")]
    pub poll_timeout_seconds: u64,
    #[serde(default = "default_max_retries_warparse")]
    pub max_retries: u32,
}

impl Default for WarparseConf {
    fn default() -> Self {
        WarparseConf {
            base_url: default_base_url(),
            deploy_path: default_deploy_path(),
            status_path: default_status_path(),
            poll_interval_seconds: default_poll_interval(),
            poll_timeout_seconds: default_poll_timeout(),
            max_retries: default_max_retries_warparse(),
        }
    }
}

fn default_base_url() -> String {
    "http://127.0.0.1:19090".to_string()
}

fn default_deploy_path() -> String {
    "/admin/v1/reloads/model".to_string()
}

fn default_status_path() -> String {
    "/admin/v1/runtime/status".to_string()
}

fn default_poll_interval() -> u64 {
    5
}

fn default_poll_timeout() -> u64 {
    300
}

fn default_max_retries_warparse() -> u32 {
    60
}

#[derive(Debug, Deserialize, Clone)]
pub struct Setting {
    pub log: LogConf,
    pub web: WebConf,
    pub database: DatabaseConf,
    #[serde(default = "default_project_root")]
    pub project_root: String,
    #[serde(default)]
    pub gitea: GiteaConf,
    #[serde(default)]
    pub assist: AssistConf,
    #[serde(default)]
    pub warparse: WarparseConf,
}

fn default_project_root() -> String {
    "./project_root".to_string()
}

impl Default for Setting {
    fn default() -> Self {
        Setting {
            log: LogConf::default(),
            web: WebConf::default(),
            database: DatabaseConf::default(),
            project_root: default_project_root(),
            gitea: GiteaConf::default(),
            assist: AssistConf::default(),
            warparse: WarparseConf::default(),
        }
    }
}

impl Setting {
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
}
