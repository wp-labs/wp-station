/**
 * 配置管理服务模块
 * 提供规则配置的查询、校验和保存功能
 * 部分类型已接入真实 API，剩余类型仍使用 Mock 数据
 */

import httpRequest from './request';

// 前端枚举：规则 / 连接类型，与后端 RuleType 枚举保持一致
export const RuleType = Object.freeze({
  SOURCE: 'source',
  SINK: 'sink',
  PARSE: 'parse',
  SOURCE_CONNECT: 'source_connect',
  SINK_CONNECT: 'sink_connect',
  WPL: 'wpl',
  OML: 'oml',
  KNOWLEDGE: 'knowledge',
});

const uniqueNames = (items) => Array.from(new Set((items || []).filter(Boolean)));
const uniqueConnectionItems = (items) => {
  const seen = new Set();

  return (items || []).filter((item) => {
    const file = item?.file;
    if (!file || seen.has(file)) {
      return false;
    }

    seen.add(file);
    return true;
  });
};

const LEGACY_CONNECTION_DISPLAY_NAMES = Object.freeze({
  '00-file-default.toml': 'File',
  '10-syslog-udp.toml': 'Syslog (UDP)',
  '11-syslog-tcp.toml': 'Syslog (TCP)',
  '12-tcp.toml': 'TCP',
  '30-kafka.toml': 'Kafka',
  '40-mysql.toml': 'MySQL',
  '00-blackhole-sink.toml': 'Blackhole',
  '01-file-prototext.toml': 'File (Prototext)',
  '02-file-json.toml': 'File (JSON)',
  '03-file-kv.toml': 'File (KV)',
  '04-file-raw.toml': 'File (RAW)',
  '09-file-test.toml': 'Test Rescue',
  '40-prometheus.toml': 'Prometheus',
  '50-mysql.toml': 'MySQL',
  '60-doris.toml': 'Doris',
  '60-postgres.toml': 'Postgres',
  '70-victorialogs.toml': 'VictoriaLogs',
  '80-victoriametrics.toml': 'VictoriaMetrics',
  '90-elasticsearch.toml': 'Elasticsearch',
  '100-clickhouse.toml': 'ClickHouse',
  '101-http.toml': 'HTTP',
});

const getConnectionDisplayName = (file, displayName) => {
  if (displayName && String(displayName).trim()) {
    return String(displayName).trim();
  }

  return LEGACY_CONNECTION_DISPLAY_NAMES[file] || file?.replace(/\.toml$/i, '') || '';
};

// Mock 网络延迟时间（毫秒）
const MOCK_DELAY = 200;

// Mock 配置内容 - sink 源配置
const sinkConfigMap = {
    'business.d/sink.toml': `[[sinks]]
key = "business_main"
type = "kafka"
topic = "business_logs"
brokers = ["localhost:9092"]`,
    'infra.d/default.toml': `[[sinks]]
key = "infra_default"
type = "file"
path = "/var/log/infra/default.log"`,
    'infra.d/error.toml': `[[sinks]]
key = "infra_error"
type = "file"
path = "/var/log/infra/error.log"`,
    'infra.d/intercept.toml': `[[sinks]]
key = "infra_intercept"
type = "file"
path = "/var/log/infra/intercept.log"`,
    'infra.d/miss.toml': `[[sinks]]
key = "infra_miss"
type = "file"
path = "/var/log/infra/miss.log"`,
    'infra.d/monitor.toml': `[[sinks]]
key = "infra_monitor"
type = "prometheus"
endpoint = "http://localhost:9090"`,
    'infra.d/residue.toml': `[[sinks]]
key = "infra_residue"
type = "file"
path = "/var/log/infra/residue.log"`,
    'defaults.toml': `# Default sink configuration
[[sinks]]
key = "default"
type = "stdout"`,
    'privacy.toml': `# Privacy sink configuration
[[sinks]]
key = "privacy"
type = "kafka"
topic = "privacy_logs"
brokers = ["localhost:9092"]`,
};

// Mock 配置内容 - knowledge 数据集
const knowledgeDataMap = {
    address: {
      config: `version = 2

[[tables]]
name = "address"

[tables.columns]
by_header = ["name", "province", "city"]

[tables.expected_rows]
min = 3`,
      createSql: `CREATE TABLE IF NOT EXISTS {table} (
  id      INTEGER PRIMARY KEY,
  name    TEXT NOT NULL,
  pinying TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_{table}_name ON {table}(name);`,
      insertSql: `INSERT INTO {table} (name, pinying) VALUES (?1, ?2);`,
      data: `id,name,province,city
1,人民中路,广东,广州
2,南京东路,上海,上海
3,世纪大道,上海,浦东`,
    },
    example: {
      config: `version = 2

[[tables]]
name = "example"

[tables.columns]
by_header = ["id", "value"]

[tables.expected_rows]
min = 1`,
      createSql: `CREATE TABLE IF NOT EXISTS {table} (
  id    INTEGER PRIMARY KEY,
  value TEXT NOT NULL
);`,
      insertSql: `INSERT INTO {table} (id, value) VALUES (?1, ?2);`,
      data: `id,value
1,test1
2,test2`,
    },
};

// Mock 配置内容 - oml 富化规则
const omlModelsMap = {
  apt: `name : aapt_module_log
rule : 
    huawei/aapt_module_log/SCANRESULT
    huawei/aapt_module_log/ANTIAPT
---
pos_sn = read(dev_sn);
access_ip: ip = read(access_ip);
log_type = read(log_type);

logtype = read(log_type);
dev_serial_num = read(dev_sn);
dev_type = read(dev_type);
log_category = read(log_category);
collect_ip: ip = read(access_ip);
collect_time_tmp = Time::now();
collect_time = pipe @collect_time_tmp | to_timestamp_ms;
collect_time_date = Time::now_date();
collect_time_hour = Time::now_hour();
raw_msg = take(raw_msg);
data_business_type = read(data_business_type);

dev_name = read(Hostname);
dev_vendor = read(dev_vendor);
occur_time = pipe read(TimeStamp) | to_timestamp_zone(8,ms);
log_version = read(dd);
log_module = read(ModuleName);
rfc_severity = read(SeverityHeader);
log_desc = read(symbol);
src_system_log_type = match read(option:[type]) {
    chars(l) => chars(日志信息);
    chars(s) => chars(安全日志信息);
    chars(t) => chars(告警信息);
    chars(d) => chars(debugging信息);
};
log_count = read(Count);
log_content = read(Content);
log_id = read(SyslogId);
virtual_system_name = read(VSys);
policy_name = read(Policy);
sip = read(SrcIp);
dip = read(DstIp);
sport = read(SrcPort);
dport = read(DstPort);
src_safe_zone = read(SrcZone);
dst_safe_zone = read(DstZone);
user_name = read(User);
protocol = read(Protocol);
app_name = read(Application);
config_file = read(Profile);
file_name = read(FileName);
file_type = read(Type);
file_size = read(Size);
flow_direction = read(Direction);
sandbox_type = read(SandboxType);
sample_submit_time = pipe read(SubTime) | to_timestamp_zone(8,ms);
scan_result = read(ScanResult);
severity = match read(option:[RiskLevel]) {
    chars(high-risk) => chars(4);
    chars(middle-risk) => chars(3);
    chars(low-risk) => chars(2);
};
file_hash = read(Hash);
threat_name = read(ThreatName);
protect_action = read(Action);
origin_alert_cat_cd = read(ThreatType);
origin_alert_cat_name = match read(option:[ThreatType]){
    chars(File Reputation) => chars(恶意文件);
    chars(Malicious URL) => chars(恶意URL);
};
alert_cat_level1_cd = match read(option:[ThreatType]){
    chars(File Reputation) => chars(105);
    chars(Malicious URL) => chars(103);
    _ => read(alert_cat_level1_cd);
};
alert_cat_level1_name = match read(option:[ThreatType]){
    chars(File Reputation) => chars(异常活动);
    chars(Malicious URL) => chars(内容安全);
    _ => read(alert_cat_level1_name);
};
alert_cat_level2_cd = match read(option:[ThreatType]){
    chars(File Reputation) => chars(105012);
    chars(Malicious URL) => chars(103008);
    _ => read(alert_cat_level2_cd);
};
alert_cat_level2_name = match read(option:[ThreatType]){
    chars(File Reputation) => chars(可疑文件);
    chars(Malicious URL) => chars(恶意URL);
    _ => read(alert_cat_level2_name);
};`,
    aws: `model adm_aws version "1.0" {
  router {
    endpoint = "http://oml-service/aws"
    timeout_ms = 900
    retry = 0
  }

  features {
    account = take(option:[aws_account])
    region = take(option:[aws_region])
  }
}`,
    nginx: `model adm_nginx version "1.0" {
  router {
    endpoint = "http://oml-service/nginx"
    timeout_ms = 800
    retry = 0
  }

  features {
    request_url = take(option:[request_url])
    status_code = take(option:[status_code])
  }
}`,
    sysmon: `model adm_sysmon version "1.0" {
  router {
    endpoint = "http://oml-service/sysmon"
    timeout_ms = 800
    retry = 1
  }

  features {
    process_name = take(option:[process_name])
    parent_pid = take(option:[parent_pid])
    event_type = take(option:[event_type])
  }
}`,
};

// Mock 配置内容 - wpl 规则文件
const wplRulesMap = {
    apt: `package /apt/ {
    rule apt {
        (
            chars\\#,
            time:TimeStamp,
            chars:xx,
            chars:Hostname,
            chars\\%\\%, 
            digit:dd, 
            chars:ModuleName\\/,
            chars:SeverityHeader\\/,
            symbol(ANTI-APT)\\(,
            chars:type\\),
            chars:Count<[,]>,
            chars\\:,
            chars:Content\\(,
        ),
        (
            kv(digit@SyslogId),
            kv(chars@VSys),
            kv(chars@Policy),
            kv(chars@SrcIp),
            kv(chars@DstIp),
            kv(digit@SrcPort),
            kv(digit@DstPort),
            kv(chars@SrcZone),
            kv(chars@DstZone),
            kv(chars@User),
            kv(chars@Protocol),
            kv(chars@Application),
            kv(chars@Profile),
            kv(chars@Direction),
            kv(chars@ThreatType),
            kv(chars@ThreatName),
            kv(chars@Action),
            kv(chars@FileType),
            kv(chars@Hash)\\),
        )\\,
    }
}`,
    aws: `package /aws/ {
    rule aws {
        (
            chars:type,
            chars:time,
            chars:elb,
            chars:client_host,
            chars:target_host,
            float:request_processing_time,
            float:target_processing_time,
            float:response_processing_time,
            digit:elb_status_code,
            digit:target_status_code,
            digit:received_bytes,
            digit:sent_bytes,
            chars:request | (chars:request_method, chars:request_url, chars:request_protocol),
            chars:user_agent,
            chars:ssl_cipher,
            chars:ssl_protocol,
            chars:target_group_arn,
            chars:trace_id,
            chars:domain_name,
            chars:chosen_cert_arn,
            chars:matched_rule_priority,
            chars:request_creation_time,
            chars:actions_executed,
            chars:redirect_url,
            chars:error_reason,
            chars:target_port_list,
            chars:target_status_code_list,
            chars:classification,
            chars:classification_reason,
            chars:traceability_id,
        )
    }
}`,
    nginx: `package /nginx/ {
   rule example {
        (ip:sip,_^2,time/clf:recv_time<[,]>,http/request",http/status,digit,chars",http/agent",_")
   }
}`,
    sysmon: `package /sysmon/ {
    rule sysmon {
        (_:pri<<,>>,3*_,chars:dev_ip),(_\\S\\y\\s\\m\\o\\n\\:,
        json(
            @Id:id,
            @Description/ProcessId:process_id,
            @Level:severity,
            @Description/CommandLine:cmd_line,
            @Description/ParentCommandLine:parent_cmd_line,
            @Description/LogonGuid:logon_guid,
            @Description/LogonId:logon_id,
            @Description/Image:process_path,
            @Description/ParentImage:parent_process_path,
            @Description/ParentProcessGuid:parent_process_guid,
            @Description/ParentProcessId:parent_process_id,
            @Description/ParentUser:parent_process_user,
            @Description/ProcessGuid:process_guid,
            @Description/Company:product_company,
            @Description/Description:process_desc,
            @Description/FileVersion:file_version,
            chars@Description/Hashes | (chars:md5_value | (chars:md5, chars:md5_hash)\\=, chars:sha256_hash)\\,
            @Description/IntegrityLevel:integrity_level,
            @Description/OriginalFileName:origin_file_name,
            @Description/Product:product_name,
            @Description/RuleName:rule_name,
            @Description/User:user_name,
            time@Description/UtcTime:occur_time,
            @Description/TerminalSessionId:terminal_session_id,
            @Description/CurrentDirectory:current_dir,
            @Keywords:keywords
            )| exists_chars(id, 1)
        )
    }
}`,
};

// Mock 配置内容 - 连接配置（来源）
const connectionSourceMap = {
  '00-file-default.toml': `[[connectors]]
id = "file_src"
type = "file"
allow_override = ["base", "file", "encode"]
[connectors.params]
base = "data/in_dat"
file = "gen.dat"
encode = "text"`,
  '10-syslog-udp.toml': `[[connectors]]
id = "syslog_udp_src"
type = "syslog"
allow_override = ["addr", "port", "protocol", "tcp_recv_bytes", "header_mode", "prefer_newline"]
[connectors.params]
addr = "0.0.0.0"
port = 1514
protocol = "udp"
header_mode = "strip"
tcp_recv_bytes = 10485760`,
  '11-syslog-tcp.toml': `[[connectors]]
id = "syslog_tcp_src"
type = "syslog"
allow_override = ["addr", "port", "protocol", "tcp_recv_bytes", "header_mode", "prefer_newline"]
[connectors.params]
addr = "127.0.0.1"
port = 1514
protocol = "tcp"
header_mode = "strip"
tcp_recv_bytes = 10485760`,
  '30-kafka.toml': `[[connectors]]
id = "kafka_src"
type = "kafka"
allow_override = ["topic", "group_id", "config"]
[connectors.params]
brokers = "localhost:9092"
topic   = ["access_log"]
group_id = "wparse_default_group"`,
};

// Mock 配置内容 - 连接配置（输出源）
const connectionSinkMap = {
  '00-blackhole-sink.toml': `[[connectors]]
id = "blackhole_sink"
type = "blackhole"`,
  '02-file-json.toml': `[[connectors]]
id = "file_json_sink"
type = "file"
allow_override = ["base","file"]
[connectors.params]
fmt  = "json"
base = "./data/out_dat"
file = "default.json"`,
  '30-kafka.toml': `[[connectors]]
id = "kafka_sink"
type = "kafka"
allow_override = ["topic", "config", "num_partitions", "replication", "brokers"]

[connectors.params]
brokers = "localhost:9092"
topic = "wparse_output"
num_partitions = 1
replication = 1
#config = ["compression.type=snappy", "acks=all"]`,
  '40-prometheus.toml': `[[connectors]]
id = "prometheus_sink"
type = "prometheus"
allow_override = ["endpoint", "source_key_format", "sink_key_format"]
[connectors.params]
endpoint = "127.0.0.1:35666"
source_key_format = "(?P<source_type>.)_(?P<access_source>.)"
sink_key_format = "(?P<rule>.)_(?P<sink_type>.)_sink"`,
};

const baseContentMap = {
  parse: `version = "1.0"
robust = "normal"

[models]
wpl = "./models/wpl"
oml = "./models/oml"
sources = "./models/source"
sinks = "./models/sink"
lib_root = "./lib"

[performance]
parse_workers = 3
rate_limit_rps = 100000

[rescue]
path = "./data/rescue"

[log_conf]
level = "debug,parse=debug,ctrl=warn,oml=info,launch=warn,sink=debug"
output = "Console"
# output = "File"
[log_conf.file]
path = "./data/logs/"

[stat]
window_sec = 60

[[stat.pick]]
key = "pick_stat"
target = "*"  # 必须保持 *
fields = ["access_source", "source_type"]
top_n = 20

[[stat.parse]]
key = "parse_stat"
target = "*"  # 必须保持 *
fields = ["access_ip", "log_desc", "log_type", "rule_name"]  # 控制收集的维度
top_n = 20

[[stat.sink]]
key = "sink_stat"
target = "*"  # 必须保持 *
fields = ["access_ip", "log_desc", "log_type"]
top_n = 20`,
  source: `[[sources]]
key = "sample_data"
enable = true
connect = "file_src"
tags = ["dev_sn: sample_data_001"]
params= { base = "./data/in_dat", file = "gen.dat", encode = "text" }

[[sources]]
key = "kafka_access"
connect = "kafka_src"
enable = false
params= { topic = ["access_log", "error_log"], config = ["auto.offset.reset=earliest", "enable.auto.commit=true"] }
tags = ["source:kafka", "type:log"]

[[sources]]
key = "syslog_udp"
connect = "syslog_udp_src"
enable = false
params= { port = 1514, strip_header = true, attach_meta_tags = true }
tags = ["protocol:syslog", "transport:udp"]`,
};

/**
 * 获取规则配置内容
 * @param {Object} options - 查询选项
 * @param {string} options.type - 配置类型（source/wpl/oml/knowledge/sink）
 * @param {string} options.file - 文件名（可选）
 * @returns {Promise<Object>} 配置内容
 */
export async function fetchRuleConfig(options) {
  const { type, file } = options;

  // Source 配置走真实后端：/api/config?rule_type=source&file=wpsrc.toml
  if (type === RuleType.SOURCE) {
    const targetFile = file || 'wpsrc.toml';
    try {
      const response = await httpRequest.get('/config', {
        params: {
          rule_type: RuleType.SOURCE,
          file: targetFile,
        },
      });

      return {
        type,
        file: targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 后端返回 NOT_FOUND（连接配置文件不存在）时，视为尚未创建，返回空内容
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }
  
  // 连接配置（来源连接 / 输出连接）走真实后端：/api/config?rule_type=source_connect|sink_connect&file=xxx.toml
  if (type === 'source_connect' || type === 'sink_connect') {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何连接配置文件');
    }

    try {
      const response = await httpRequest.get('/config', {
        params: {
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        displayName: response?.display_name || response?.displayName || undefined,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      throw error;
    }
  }

  // 解析配置（parse）复用连接配置接口，使用 rule_type=parse, file 固定为 wparse.toml
  if (type === RuleType.PARSE) {
    const targetFile = 'wparse.toml';
    try {
      const response = await httpRequest.get('/config', {
        params: {
          rule_type: RuleType.PARSE,
          file: targetFile,
        },
      });

      return {
        type,
        file: targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  // sink 配置走真实后端：/api/config?rule_type=sink&file=xxx.toml
  if (type === RuleType.SINK) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何 sink 配置文件');
    }

    try {
      const response = await httpRequest.get('/config', {
        params: {
          rule_type: RuleType.SINK,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      throw error;
    }
  }

  // wpl / oml 规则配置走通用规则接口：/api/config/rules
  if (type === RuleType.WPL || type === RuleType.OML) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何规则文件');
    }

    try {
      const response = await httpRequest.get('/config/rules', {
        params: {
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        content: response?.content || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 规则文件不存在时，返回空内容，方便前端展示空白编辑器
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          content: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  // knowledge 配置走通用规则接口：/api/config/rules（返回多块内容）
  if (type === RuleType.KNOWLEDGE) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何数据集');
    }

    try {
      const response = await httpRequest.get('/config/rules', {
        params: {
          rule_type: type,
          file: targetFile,
        },
      });

      return {
        type,
        file: response?.file || targetFile,
        config: response?.config || '',
        createSql: response?.create_sql || response?.createSql || '',
        insertSql: response?.insert_sql || response?.insertSql || '',
        data: response?.data || '',
        lastModified: response?.last_modified || undefined,
      };
    } catch (error) {
      const status = error?.response?.status;
      const errorCode = error?.response?.data?.error?.code;

      // 知识库配置不存在时，返回空内容，方便前端展示空白编辑器
      if (status === 404 || errorCode === 'NOT_FOUND') {
        return {
          type,
          file: targetFile,
          config: '',
          createSql: '',
          insertSql: '',
          data: '',
          lastModified: undefined,
        };
      }

      throw error;
    }
  }

  // 模拟网络延迟
  await new Promise((resolve) => {
    setTimeout(resolve, MOCK_DELAY);
  });

  // 连接配置需要根据 file 参数返回具体文件内容
  if (type === 'connection' && file) {
    const content = connectionSourceMap[file] || connectionSinkMap[file] || '';
    return {
      type,
      file,
      content,
      lastModified: new Date().toISOString(),
    };
  }

  const mockContentMap = {
    ...baseContentMap,
    wpl: wplRulesMap[file] || wplRulesMap.apt,
    oml: omlModelsMap[file] || omlModelsMap.apt,
    sink: sinkConfigMap[file] || sinkConfigMap['defaults.toml'],
  };

  return {
    type,
    file: file || `${type}.toml`,
    content: mockContentMap[type] || '',
    lastModified: new Date().toISOString(),
  };
}

/**
 * 获取规则列表
 * @param {Object} options - 查询选项
 * @param {string} options.type - 配置类型（wpl/oml/knowledge/connection）
 * @param {string} [options.keyword] - 文件名关键字（可选）
 * @returns {Promise<string[]>} 文件或数据集列表
 */
export async function fetchRuleFiles(options) {
  const { type, page, pageSize, keyword } = options;

  // wpl / oml / knowledge 规则列表走后端：/api/config/rules/files
  if (type === RuleType.WPL || type === RuleType.OML || type === RuleType.KNOWLEDGE) {
    const currentPage = typeof page === 'number' && page > 0 ? page : 1;
    const defaultPageSize = 15;
    const currentPageSize =
      typeof pageSize === 'number' && pageSize > 0 ? pageSize : defaultPageSize;

    const keywordParam =
      typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;

    const response = await httpRequest.get('/config/rules/files', {
      params: {
        rule_type: type,
        page: currentPage,
        page_size: currentPageSize,
        keyword: keywordParam,
      },
    });

    const items = Array.isArray(response?.items) ? response.items : [];
    const files = uniqueNames(items.map((item) => item.file));

    return {
      items: files,
      total: typeof response?.total === 'number' ? response.total : files.length,
      page: typeof response?.page === 'number' ? response.page : currentPage,
      pageSize:
        typeof response?.page_size === 'number' ? response.page_size : currentPageSize,
    };
  }

  // sink 连接配置列表走后端：/api/config/files
  if (type === RuleType.SINK) {
    const response = await httpRequest.get('/config/files', {
      params: {
        rule_type: RuleType.SINK,
      },
    });

    const items = Array.isArray(response?.items) ? response.items : [];
    const seen = new Map();
    items.forEach((item) => {
      const file = typeof item?.file === 'string' ? item.file : '';
      if (!file || seen.has(file)) {
        return;
      }
      const displayName =
        typeof item?.display_name === 'string' && item.display_name.trim()
          ? item.display_name.trim()
          : undefined;
      seen.set(file, { file, displayName });
    });
    const normalizedItems = Array.from(seen.values());

    return {
      items: normalizedItems,
      total: normalizedItems.length,
      page: 1,
      pageSize: normalizedItems.length || 1,
    };
  }

  await new Promise((resolve) => {
    setTimeout(resolve, MOCK_DELAY);
  });

  if (type === RuleType.WPL) {
    return Object.keys(wplRulesMap);
  }
  if (type === RuleType.OML) {
    return Object.keys(omlModelsMap);
  }
  if (type === 'connection') {
    // 返回连接配置的分组结构
    return {
      sources: Object.keys(connectionSourceMap),
      sinks: Object.keys(connectionSinkMap),
    };
  }
  return [];
}

// 创建规则文件（wpl / oml / knowledge）
export async function createRuleFile(options) {
  const { type, file } = options;

  if (!type || !file) {
    throw new Error('创建规则文件时必须提供类型和文件名');
  }

  await httpRequest.post('/config/rules/files', {
    rule_type: type,
    file,
  });
}

// 删除规则文件（wpl / oml / knowledge）
export async function deleteRuleFile(options) {
  const { type, file } = options;

  if (!type || !file) {
    throw new Error('删除规则文件时必须提供类型和文件名');
  }

  await httpRequest.delete('/config/rules/files', {
    params: {
      rule_type: type,
      file,
    },
  });
}

/**
 * 获取连接配置文件列表（来源 / 输出源），走真实后端接口
 * @param {Object} [options]
 * @param {string} [options.keyword] - 文件名关键字（可选）
 * @returns {Promise<{sources: Array<{file: string, displayName: string}>, sinks: Array<{file: string, displayName: string}>}>}
 */
export async function fetchConnectionFiles(options = {}) {
  const { keyword } = options;

  const keywordParam =
    typeof keyword === 'string' && keyword.trim() ? keyword.trim() : undefined;

  const [sourceResponse, sinkResponse] = await Promise.all([
    httpRequest.get('/config/files', {
      params: {
        rule_type: 'source_connect',
        keyword: keywordParam,
      },
    }),
    httpRequest.get('/config/files', {
      params: {
        rule_type: 'sink_connect',
        keyword: keywordParam,
      },
    }),
  ]);

  const sourceItems = Array.isArray(sourceResponse?.items) ? sourceResponse.items : [];
  const sinkItems = Array.isArray(sinkResponse?.items) ? sinkResponse.items : [];

  const sources = uniqueConnectionItems(
    sourceItems.map((item) => ({
      file: item?.file,
      displayName: getConnectionDisplayName(item?.file, item?.display_name),
    })),
  );
  const sinks = uniqueConnectionItems(
    sinkItems.map((item) => ({
      file: item?.file,
      displayName: getConnectionDisplayName(item?.file, item?.display_name),
    })),
  );

  return { sources, sinks };
}

/**
 * 创建连接配置文件（来源 / 输出源）
 * @param {Object} options
 * @param {('source'|'sink')} options.category - 配置类别
 * @param {string} options.file - 文件名
 * @param {string} [options.displayName] - 展示名
 */
export async function createConnectionConfigFile(options) {
  const { category, file, displayName } = options;

  if (!category || !file) {
    throw new Error('创建连接配置文件时必须提供类别和文件名');
  }

  await httpRequest.post('/config/files', {
    rule_type: category,
    file,
    display_name: displayName || undefined,
  });
}

/**
 * 删除连接配置文件（来源 / 输出源）
 * @param {Object} options
 * @param {('source_connect'|'sink_connect')} options.category - 配置类别
 * @param {string} options.file - 文件名
 */
export async function deleteConnectionConfigFile(options) {
  const { category, file } = options;

  if (!category || !file) {
    throw new Error('删除连接配置文件时必须提供类别和文件名');
  }

  await httpRequest.delete('/config/files', {
    params: {
      rule_type: category,
      file,
    },
  });
}

/**
 * 校验规则配置
 * @param {Object} options - 校验选项
 * @param {string} options.type - 配置类型
 * @param {string} options.content - 配置内容
 * @returns {Promise<Object>} 校验结果
 */
export async function validateRuleConfig(options) {
  const { type, file, content } = options;

  // 所有类型统一走真实后端校验：POST /api/config/rules/validate

  // 若未显式传入文件名，则根据类型给一个合理的默认值
  let targetFile = file;
  if (!targetFile) {
    if (type === RuleType.SOURCE) {
      targetFile = 'wpsrc.toml';
    } else if (type === RuleType.PARSE) {
      targetFile = 'wparse.toml';
    } else {
      targetFile = `${type}.toml`;
    }
  }

  const currentContent = content || '';

  const response = await httpRequest.post('/config/rules/validate', {
    rule_type: type,
    file: targetFile,
    content: currentContent,
  });

  const lineCount = currentContent ? currentContent.split('\n').length : 0;

  return {
    filename: targetFile,
    lines: lineCount,
    valid: Boolean(response?.valid),
    // 后端目前仅返回 valid/message，这里先不生成逐行错误列表
    warnings: 0,
    errors: [],
    message: response?.message,
  };
}

/**
 * 保存规则配置
 * @param {Object} options - 保存选项
 * @param {string} options.type - 配置类型
 * @param {string} options.file - 文件名
 * @param {string} options.content - 配置内容
 * @returns {Promise<Object>} 保存结果
 */
export async function saveRuleConfig(options) {
  const { type, file, content } = options;

  // Source 配置走真实后端保存：POST /api/config
  if (type === RuleType.SOURCE) {
    const targetFile = file || 'wpsrc.toml';

    await httpRequest.post('/config', {
      rule_type: RuleType.SOURCE,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // 连接配置（来源连接 / 输出连接）走真实后端保存：POST /api/config
  if (type === 'source_connect' || type === 'sink_connect') {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何连接配置文件');
    }

    await httpRequest.post('/config', {
      rule_type: type,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // 解析配置（parse）走真实后端保存：POST /api/config，固定文件 wparse.toml
  if (type === RuleType.PARSE) {
    const targetFile = 'wparse.toml';

    await httpRequest.post('/config', {
      rule_type: RuleType.PARSE,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // sink 配置走真实后端保存：POST /api/config
  if (type === RuleType.SINK) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何 sink 配置文件');
    }

    await httpRequest.post('/config', {
      rule_type: RuleType.SINK,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // wpl / oml 规则保存走通用规则接口：POST /api/config/rules/save
  if (type === RuleType.WPL || type === RuleType.OML) {
    const targetFile = file;
    if (!targetFile) {
      throw new Error('当前未选择任何规则文件');
    }

    await httpRequest.post('/config/rules/save', {
      rule_type: type,
      file: targetFile,
      content: content || '',
    });

    const fileSize = content ? content.length : 0;
    return {
      success: true,
      fileSize,
      message: '保存成功',
    };
  }

  // 其余类型暂时继续使用 Mock 行为
  await new Promise((resolve) => {
    setTimeout(resolve, MOCK_DELAY);
  });

  const fileSize = content ? content.length : 0;

  return {
    success: true,
    fileSize,
    message: '保存成功',
  };
}

// 保存知识库规则配置（knowledge 类型）
export async function saveKnowledgeRule(options) {
  const { file, config, createSql, insertSql, data } = options;

  if (!file) {
    throw new Error('当前未选择任何数据集');
  }

  await httpRequest.post('/config/knowledge/save', {
    file,
    config: config ?? '',
    create_sql: createSql ?? '',
    insert_sql: insertSql ?? '',
    data: data ?? '',
  });

  const fileSize = (config || '').length + (createSql || '').length + (insertSql || '').length + (data || '').length;

  return {
    success: true,
    fileSize,
    message: '保存成功',
  };
}

export async function fetchKnowdbConfig() {
  const response = await httpRequest.get('/config/knowledge/knowdb');
  return {
    file: response?.file || 'knowdb.toml',
    content: response?.content || '',
    lastModified: response?.last_modified || null,
  };
}

export async function saveKnowdbConfig(content) {
  await httpRequest.post('/config/knowledge/knowdb', {
    content: content ?? '',
  });
  return {
    success: true,
    message: '保存成功',
  };
}


/**
 * 执行知识库 SQL 查询
 * @param {string} sql - SQL 查询语句
 * @returns {Promise<{fields: Array, columns: Array}>} 处理后的查询结果
 */
export async function executeKnowledgeSql(sql) {
  const response = await httpRequest.post('/db', { sql });

  // 兼容多种响应格式：
  // 1. 完整格式: { code: 200, msg: "success", data: [...] }
  // 2. 已解包格式: [...] (直接是 data 数组)
  const dataArray = Array.isArray(response) 
    ? response 
    : (response && response.code === 200 && Array.isArray(response.data) ? response.data : null);

  if (dataArray && dataArray.length > 0) {
    // 新后端返回格式: 二维数组，每行是字段对象数组
    // [[{meta, name, value: {Digit|Chars: x}}, ...], ...]
    const firstRow = dataArray[0];
    
    // 检测是否为新格式（数组的数组，且内部对象有 name 和 value 属性）
    const isNewFormat = Array.isArray(firstRow) && 
      firstRow.length > 0 && 
      firstRow[0] && 
      typeof firstRow[0].name === 'string' && 
      firstRow[0].value !== undefined;

    if (isNewFormat) {
      // 从第一行提取列名
      const headers = firstRow.map((field) => field.name);

      // 生成列定义
      const columns = headers.map((header) => ({
        title: header,
        dataIndex: header,
        key: header,
      }));

      // 解析每行数据
      const fields = dataArray.map((row, rowIndex) => {
        const rowData = { key: rowIndex };
        row.forEach((field) => {
          // 从 value 对象中提取实际值（Digit 或 Chars）
          const valueObj = field.value;
          let actualValue = valueObj;
          if (valueObj && typeof valueObj === 'object') {
            // 取对象的第一个值（Digit 或 Chars 的值）
            actualValue = Object.values(valueObj)[0];
          }
          rowData[field.name] = actualValue;
        });
        return rowData;
      });

      return { fields, columns };
    }
  }

  if (response && response.code !== 200) {
    throw new Error(response.msg || '查询失败');
  } 
  return { fields: [], columns: [] };
}
