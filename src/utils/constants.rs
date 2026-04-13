// 工具层内的常量

// ============ WPL 规则文件 ============

/// WPL 规则目录下的解析文件名
pub const WPL_PARSE_FILENAME: &str = "parse.wpl";
/// WPL 规则目录下的样本文件名
pub const WPL_SAMPLE_FILENAME: &str = "sample.dat";

/// sink 配置文件在没有自定义展示名称时的兌底标签
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

/// 沙盒输出文件及含义
pub const OUTPUT_PATHS: [(&str, &str); 4] = [
    ("data/out_dat/default.dat", "数据命中兜底路由"),
    ("data/out_dat/miss.dat", "样本未命中任何规则"),
    ("data/out_dat/residue.dat", "存在残余未处理数据"),
    ("data/out_dat/error.dat", "处理过程中出现错误"),
];

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

// ============ 发布任务调度 =============

pub const MAX_BATCH_SIZE: u64 = 50;
pub const LOOP_IDLE_SECONDS: u64 = 1;
pub const FIRST_POLL_DELAY_SECONDS: i64 = 1;

pub const STAGE_CALL_CLIENT: &str = "调用客户端";
pub const STAGE_RUNTIME: &str = "运行状态";

fn normalize_sink_key(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .replace('\\', "/")
        .to_lowercase()
}

/// 根据 sink 文件路径推导展示名称
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
