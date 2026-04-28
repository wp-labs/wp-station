//! 通用工具与常量模块。
//!
//! 存放不归属任何业务子模块的常量定义、展示格式化函数及其他零散工具函数。

use chrono::{DateTime, FixedOffset, Utc};

// ============ WPL 规则文件 ============

/// WPL 规则目录下的解析文件名
pub const WPL_PARSE_FILENAME: &str = "parse.wpl";
/// WPL 规则目录下的样本文件名
pub const WPL_SAMPLE_FILENAME: &str = "sample.dat";

/// sink 配置文件在没有自定义展示名称时的兜底标签
pub const SINK_DISPLAY_FALLBACKS: &[(&str, &str)] = &[
    ("business.d/sink.toml", "输出配置"),
    ("infra.d/error.toml", "异常数据"),
    ("infra.d/miss.toml", "未命中WPL数据"),
    ("infra.d/default.toml", "未命中OML数据"),
    ("infra.d/monitor.toml", "监控数据"),
    ("infra.d/residue.toml", "残留数据"),
    ("infra.d/intercept.toml", "拦截数据"),
    ("privacy.toml", "隐私数据"),
];

// ============ Sandbox 数据相关 ============

/// 沙盒历史记录默认保留条数
pub const DEFAULT_HISTORY_LIMIT: u64 = 20;

/// 沙盒输出文件及其含义
pub const OUTPUT_PATHS: [(&str, &str); 4] = [
    ("data/out_dat/default.dat", "数据命中兜底路由"),
    ("data/out_dat/miss.dat", "样本未命中任何规则"),
    ("data/out_dat/residue.dat", "存在残余未处理数据"),
    ("data/out_dat/error.dat", "处理过程中出现错误"),
];

/// 沙盒运行时强制覆盖的 business sink 配置，确保输出到本地文件供分析读取。
pub const BUSINESS_SINK_OVERRIDE: &str = r#"version = "1.0"

[sink_group]
name = "kafka_sink"
oml = ["*"]
parallel = 1

[[sink_group.sinks]]
name = "all_sink"
connect = "file_json_sink"
tags = []

[sink_group.sinks.params]
base = "./data/out_dat/"
file = "all.json"
"#;

/// 沙盒运行时强制使用的 UDP 监听端口。
pub const SANDBOX_RUNTIME_UDP_PORT: u16 = 31601;
/// 沙盒运行时使用的 source key。
pub const SANDBOX_RUNTIME_SOURCE_KEY: &str = "gen_udp";
/// 沙盒运行时要求存在的 source connector。
pub const SANDBOX_RUNTIME_SOURCE_CONNECTOR: &str = "syslog_udp_src";
/// 沙盒运行时 wpgen 输出 connector。
pub const SANDBOX_RUNTIME_OUTPUT_CONNECTOR: &str = "syslog_udp_sink";
/// 沙盒 UDP source 的默认监听地址。
pub const SANDBOX_RUNTIME_SOURCE_ADDR: &str = "0.0.0.0";
/// 沙盒 UDP 输出的默认目标地址。
pub const SANDBOX_RUNTIME_OUTPUT_ADDR: &str = "0.0.0.0";
/// 沙盒运行时统一使用 UDP 协议。
pub const SANDBOX_RUNTIME_PROTOCOL: &str = "udp";
/// 沙盒 syslog source 的头处理模式。
pub const SANDBOX_RUNTIME_HEADER_MODE: &str = "strip";

/// Sandbox 日志截断最大行数
pub const MAX_LINES: usize = 500;

// ============ 发布任务调度 ============

/// 单次轮询处理的最大发布目标数，防止单轮阻塞过久。
pub const MAX_BATCH_SIZE: u64 = 50;
/// 每轮发布轮询之间的空闲等待秒数。
pub const LOOP_IDLE_SECONDS: u64 = 1;
/// 新建发布目标后首次探活的延迟秒数，给设备端留出响应时间。
pub const FIRST_POLL_DELAY_SECONDS: i64 = 1;

/// 发布阶段标签：调用客户端接口
pub const STAGE_CALL_CLIENT: &str = "调用客户端";
/// 发布阶段标签：运行状态检查
pub const STAGE_RUNTIME: &str = "运行状态";

// ============ 工具函数 ============

/// 统一 sink 文件路径的规范化形式，用于模糊匹配时消除路径分隔符和大小写差异。
fn normalize_sink_key(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .replace('\\', "/")
        .to_lowercase()
}

/// 根据 sink 文件路径推导展示名称。
///
/// 先精确匹配预置映射表，再按文件名（不含目录）模糊匹配。若存在多个同名冲突则返回 `None`，
/// 避免错误关联。
pub fn fallback_sink_display(file_name: &str) -> Option<&'static str> {
    let normalized = normalize_sink_key(file_name);
    if normalized.is_empty() {
        return None;
    }

    if let Some((_, label)) = SINK_DISPLAY_FALLBACKS
        .iter()
        .find(|(pattern, _)| normalize_sink_key(pattern) == normalized)
    {
        return Some(*label);
    }

    let base = normalized.rsplit('/').next().unwrap_or("").to_string();
    if base.is_empty() {
        return None;
    }

    let mut candidate: Option<&'static str> = None;
    let mut conflict = false;
    for (pattern, label) in SINK_DISPLAY_FALLBACKS.iter() {
        let normalized_pattern = normalize_sink_key(pattern);
        let pattern_base = normalized_pattern
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string();
        if pattern_base == base {
            if candidate.is_some() {
                conflict = true;
                break;
            }
            candidate = Some(*label);
        }
    }

    if conflict { None } else { candidate }
}

/// 将 UTC 时间格式化为北京时间字符串（`YYYY-MM-DD HH:MM:SS`），用于前端展示。
pub fn format_beijing_time(time: DateTime<Utc>) -> String {
    let beijing = FixedOffset::east_opt(8 * 3600).expect("北京时间 UTC+8 偏移固定有效");
    time.with_timezone(&beijing)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
