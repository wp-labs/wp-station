// 工具层内的常量

// ============ WPL 规则文件 ============

/// WPL 规则目录下的解析文件名
pub const WPL_PARSE_FILENAME: &str = "parse.wpl";
/// WPL 规则目录下的样本文件名
pub const WPL_SAMPLE_FILENAME: &str = "sample.dat";

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

pub const WPSRC_SOURCE_OVERRIDE: &str = r#"[[sources]]
key = "gen_udp"
enable = true
connect = "syslog_udp_src"

[sources.params]
addr = "0.0.0.0"
port = 31600
protocol = "udp"
header_mode = "strip"
"#;

/// Sandbox 日志截断最大行数
pub const MAX_LINES: usize = 500;

// ============ 发布任务调度 =============

pub const MAX_BATCH_SIZE: u64 = 50;
pub const LOOP_IDLE_SECONDS: u64 = 1;
pub const FIRST_POLL_DELAY_SECONDS: i64 = 1;

pub const STAGE_CALL_CLIENT: &str = "调用客户端";
pub const STAGE_RUNTIME: &str = "运行状态";
