//! 统一业务常量定义。
//!
//! 收敛主应用中的目录名、文件名、发布分组、沙盒运行时默认值等可共享常量，
//! 避免在多个模块中重复硬编码。

pub mod project {
    /// 所有系统仓库都挂在 gitea 目录下，按 system__area 命名。
    pub const DIR_GITEA_ROOT: &str = "gitea";
    pub const REPO_WPARSE_MODELS: &str = "wparse__models";
    pub const REPO_WPARSE_INFRA: &str = "wparse__infra";
    pub const REPO_WFUSION_MODELS: &str = "wfusion__models";
    pub const REPO_WFUSION_INFRA: &str = "wfusion__infra";
    pub const REPO_SHARED_CONNECTORS: &str = "shared__connectors";

    /// 项目核心配置目录结构：规则模型、基础设施配置、connector 模板与 topology 拓扑都从这里展开。
    pub const DIR_CONF: &str = "conf";
    pub const DIR_CONNECTORS: &str = "connectors";
    pub const DIR_TOPOLOGY: &str = "topology";
    pub const DIR_MODELS: &str = "models";
    pub const DIR_RUNTIME: &str = "runtime";

    /// 仓库内部常见的子目录名，供规则管理、配置管理、模板扫描与发布链路共用。
    pub const DIR_SOURCE_D: &str = "source.d";
    pub const DIR_SINK_D: &str = "sink.d";
    pub const DIR_SOURCES: &str = "sources";
    pub const DIR_SINKS: &str = "sinks";
    pub const DIR_WPL: &str = "wpl";
    pub const DIR_OML: &str = "oml";
    pub const DIR_SCHEMAS: &str = "schemas";
    pub const DIR_RULES: &str = "rules";
    pub const DIR_SCENARIOS: &str = "scenarios";
    pub const DIR_KNOWLEDGE: &str = "knowledge";
    pub const DIR_BUSINESS_D: &str = "business.d";
    pub const DIR_INFRA_D: &str = "infra.d";

    /// 各业务模块约定使用的核心文件名，避免在读写项目文件时散落硬编码。
    pub const FILE_WPARSE: &str = "wparse.toml";
    pub const FILE_WFUSION: &str = "wfusion.toml";
    pub const FILE_WPGEN: &str = "wpgen.toml";
    pub const FILE_WINDOWS: &str = "windows.toml";
    pub const FILE_WFUSION_GLOBAL_RULE: &str = "_global.wfl";
    pub const FILE_KNOWDB: &str = "knowdb.toml";
    pub const FILE_WPL_PARSE: &str = "parse.wpl";
    pub const FILE_WPL_SAMPLE: &str = "sample.dat";
    pub const FILE_OML_ADM: &str = "adm.oml";
    pub const FILE_WPSRC: &str = "wpsrc.toml";
    pub const FILE_DEFAULTS: &str = "defaults.toml";
    pub const FILE_PRIVACY: &str = "privacy.toml";
    pub const FILE_BUSINESS_SINK: &str = "sink.toml";

    /// 配置包导入时允许解包覆盖的顶层目录，仅限项目主数据目录，避免误写其他路径。
    pub const IMPORTABLE_ROOT_DIRS: [&str; 4] =
        [DIR_CONF, DIR_CONNECTORS, DIR_TOPOLOGY, DIR_MODELS];
    /// 导入归档时的临时展开目录名。
    pub const ARCHIVE_IMPORT_STAGING_DIR: &str = "project-archive-imports";

    /// 输出配置文件在 UI 中的兜底中文展示名，避免文件名直接暴露给用户。
    pub const SINK_DISPLAY_FALLBACKS: &[(&str, &str)] = &[
        ("business.d/sink.toml", "输出配置"),
        ("infra.d/monitor.toml", "监控数据"),
        ("infra.d/miss.toml", "未命中WPL数据"),
        ("infra.d/default.toml", "未命中OML数据"),
        ("infra.d/error.toml", "异常数据"),
        ("infra.d/residue.toml", "残留数据"),
        ("infra.d/intercept.toml", "拦截数据"),
        ("privacy.toml", "隐私数据"),
    ];
}

pub mod config {
    /// connector 模板文件的默认展示名兜底表，扫描不到更友好的名字时使用。
    pub const CONNECTOR_DISPLAY_FALLBACKS: &[(&str, &str)] = &[
        ("00-file-default.toml", "File"),
        ("10-syslog-udp.toml", "Syslog (UDP)"),
        ("11-syslog-tcp.toml", "Syslog (TCP)"),
        ("12-tcp.toml", "TCP"),
        ("20-http.toml", "HTTP"),
        ("30-kafka.toml", "Kafka"),
        ("40-mysql.toml", "MySQL"),
        ("50-postgres.toml", "Postgres"),
        ("60-dmdb-connect_string.toml", "DMDB (Connection String)"),
        ("61-dmdb-endpoint.toml", "DMDB (Endpoint)"),
        ("62-dmdb-dsn.toml", "DMDB (DSN)"),
        ("00-blackhole-sink.toml", "Blackhole"),
        ("01-file-prototext.toml", "File (Prototext)"),
        ("02-file-json.toml", "File (JSON)"),
        ("03-file-kv.toml", "File (KV)"),
        ("04-file-raw.toml", "File (RAW)"),
        ("09-file-test.toml", "Test Rescue"),
        ("13-udp.toml", "UDP"),
        ("14-count.toml", "Count"),
        ("40-prometheus.toml", "Prometheus"),
        ("50-mysql.toml", "MySQL"),
        ("60-doris.toml", "Doris"),
        ("60-postgres.toml", "Postgres"),
        ("70-victorialogs.toml", "VictoriaLogs"),
        ("80-victoriametrics.toml", "VictoriaMetrics"),
        ("90-elasticsearch.toml", "Elasticsearch"),
        ("100-clickhouse.toml", "ClickHouse"),
        ("101-http.toml", "HTTP"),
        ("110-dmdb-connect_string.toml", "DMDB (Connection String)"),
        ("111-dmdb-endpoint.toml", "DMDB (Endpoint)"),
        ("112-dmdb-dsn.toml", "DMDB (DSN)"),
    ];

    /// 连接配置页左侧列表的默认排序，兼容历史文件命名与用户认知顺序。
    pub const CONNECTION_FILE_ORDER: &[&str] = &[
        "00-file-default.toml",
        "10-syslog-udp.toml",
        "11-syslog-tcp.toml",
        "12-tcp.toml",
        "30-kafka.toml",
        "40-mysql.toml",
        "00-blackhole-sink.toml",
        "01-file-prototext.toml",
        "02-file-json.toml",
        "03-file-kv.toml",
        "04-file-raw.toml",
        "09-file-test.toml",
        "40-prometheus.toml",
        "50-mysql.toml",
        "60-doris.toml",
        "60-postgres.toml",
        "70-victorialogs.toml",
        "80-victoriametrics.toml",
        "90-elasticsearch.toml",
        "100-clickhouse.toml",
        "101-http.toml",
    ];

    /// 业务 sink 与基础设施 sink 在配置页中的默认展示顺序。
    pub const SINK_FILE_ORDER: &[&str] = &[
        "business.d/sink.toml",
        "infra.d/monitor.toml",
        "infra.d/miss.toml",
        "infra.d/default.toml",
        "infra.d/error.toml",
        "infra.d/residue.toml",
    ];

    /// connector 类型到中文名称的映射，用于接入概览、模板弹窗和配置页统一展示口径。
    pub const CONNECTOR_TYPE_DISPLAY_NAMES: &[(&str, &str)] = &[
        ("file", "文件"),
        ("kafka", "Kafka"),
        ("dmdb", "达梦数据库"),
        ("mysql", "MySQL"),
        ("postgres", "PostgreSQL"),
        ("doris", "Doris"),
        ("clickhouse", "ClickHouse"),
        ("elasticsearch", "Elasticsearch"),
        ("victorialogs", "VictoriaLogs"),
        ("victoriametrics", "VictoriaMetrics"),
        ("prometheus", "Prometheus"),
        ("http", "HTTP"),
        ("syslog-udp", "Syslog UDP"),
        ("syslog-tcp", "Syslog TCP"),
        ("tcp", "TCP"),
        ("udp", "UDP"),
    ];
}

pub mod sandbox {
    /// 预发布详情页默认保留的历史任务条数。
    pub const DEFAULT_HISTORY_LIMIT: u64 = 20;
    /// 单个阶段日志返回给前端时允许展示的最大行数，避免日志过长拖慢页面。
    pub const MAX_LOG_LINES: usize = 500;
    /// 沙盒项目目录仅保留最近 3 次；所有任务的阶段日志长期保留。
    pub const RUNTIME_ARTIFACT_RETENTION_RUNS: usize = 3;

    /// WParse 沙盒运行产物及其检查说明。
    ///
    /// 第三个字段表示文件是否参与沙盒通过判定。ignore/raw_log 是辅助输出，
    /// 需要展示统计结果但不能因为有内容而判定本次预发布失败。
    pub const OUTPUT_PATHS: [(&str, &str, bool); 6] = [
        ("data/out_dat/default.dat", "数据命中兜底路由", true),
        ("data/out_dat/miss.dat", "样本未命中任何规则", true),
        ("data/out_dat/residue.dat", "存在残余未处理数据", true),
        ("data/out_dat/error.dat", "处理过程中出现错误", true),
        ("data/out_dat/ignore.json", "忽略输出数据", false),
        ("data/out_dat/raw_log.json", "原始日志输出", false),
    ];

    /// 沙盒里强制覆盖的业务 sink，统一把消息写到本地产物目录，避免影响真实下游。
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

    /// 沙盒运行时固定使用的本地回环输入输出参数，保证 wpgen / wparse 在隔离环境内互通。
    pub const RUNTIME_UDP_PORT: u16 = 31601;
    pub const RUNTIME_SOURCE_KEY: &str = "gen_udp";
    pub const RUNTIME_SOURCE_CONNECTOR: &str = "syslog_udp_src";
    pub const RUNTIME_OUTPUT_CONNECTOR: &str = "udp_out_sink";
    pub const RUNTIME_SOURCE_ADDR: &str = "0.0.0.0";
    pub const RUNTIME_OUTPUT_ADDR: &str = "127.0.0.1";
    pub const RUNTIME_PROTOCOL: &str = "udp";
    pub const RUNTIME_HEADER_MODE: &str = "keep";
    pub const WFUSION_RUNTIME_TCP_PORT: u16 = 9800;
    pub const WFUSION_RUNTIME_SOURCE_KEY: &str = "sandbox_tcp";
    pub const WFUSION_RUNTIME_SOURCE_CONNECTOR: &str = "tcp_src";

    /// daemon 启动后额外等待一小段时间，再拉起 wpgen，减少端口刚就绪时的竞态。
    pub const DAEMON_READY_BEFORE_WPGEN_WAIT_MS: u64 = 1_000;
}

pub mod release {
    /// 发布记录支持的配置分组标识，用于草稿、规则、设施与全量发布之间统一判断。
    pub const GROUP_MODELS: &str = "models";
    pub const GROUP_INFRA: &str = "infra";
    pub const GROUP_ALL: &str = "all";
    pub const GROUP_DRAFT: &str = "draft";

    /// 发布调度器的基础运行参数：单次抓取批量、空闲轮询间隔、首次状态回查延迟。
    pub const MAX_BATCH_SIZE: u64 = 50;
    pub const LOOP_IDLE_SECONDS: u64 = 1;
    pub const FIRST_POLL_DELAY_SECONDS: i64 = 1;

    /// 发布阶段轨迹里专门记录客户端调用与运行状态回查的阶段名称，前后端依赖固定中文文案匹配。
    pub const STAGE_CALL_CLIENT: &str = "调用客户端";
    pub const STAGE_RUNTIME: &str = "运行状态";

    /// 返回发布分组对应的中文标题。
    pub fn group_title(group: &str) -> &str {
        match group {
            GROUP_MODELS => "规则配置",
            GROUP_INFRA => "设施配置",
            GROUP_ALL => "全量配置",
            GROUP_DRAFT => "草稿",
            _ => group,
        }
    }

    /// 返回发布分组对应的发布按钮文案。
    pub fn publish_label(group: &str) -> &'static str {
        match group {
            GROUP_MODELS => "发布规则",
            GROUP_INFRA => "发布设施",
            GROUP_ALL => "发布",
            _ => "发布",
        }
    }
}

pub mod warparse {
    /// WarpParse 客户端固定使用的发布与运行状态查询接口路径。
    pub const DEPLOY_PATH: &str = "/admin/v1/reloads/model";
    pub const STATUS_PATH: &str = "/admin/v1/runtime/status";
}

pub mod gitea {
    /// Gitea 基线版本使用的保留 tag 名称，用于定位默认对比基线。
    pub const REPO_BASELINE_TAG: &str = "v1.0.0";
}

pub mod assist {
    /// AI 辅助任务超过该时长仍未回写结果时，视为陈旧任务并允许后续兜底处理。
    pub const STALE_AI_TASK_RELEASE_SECONDS: i64 = 30 * 60;
}

pub mod device {
    /// 创建设备或即时探活时，请求设备端接口的默认连接超时时间。
    pub const CREATE_DEVICE_CONNECT_TIMEOUT_SECONDS: u64 = 3;
}

pub mod api {
    /// 配置包导入接口允许接收的最大归档体积，防止超大文件拖垮服务。
    pub const MAX_ARCHIVE_BYTES: usize = 200 * 1024 * 1024;
}
