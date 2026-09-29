//! 接入概览明细格式化与 connector 元数据辅助。

use std::collections::{BTreeMap, HashMap};

use crate::utils::display::connector_type_display_name;

/// connector 模板元数据。
#[derive(Debug, Clone)]
pub(super) struct ConnectorMeta {
    pub(super) type_key: String,
    pub(super) type_label: String,
    pub(super) default_params: BTreeMap<String, String>,
}

/// 合并模板默认参数与运行时参数，得到最终展示用配置。
pub(super) fn build_effective_params(
    meta_map: &HashMap<String, ConnectorMeta>,
    connect: &str,
    params: &BTreeMap<String, toml::Value>,
) -> BTreeMap<String, String> {
    let mut merged = meta_map
        .get(connect)
        .map(|meta| meta.default_params.clone())
        .unwrap_or_default();

    for (key, value) in params {
        let rendered = render_runtime_value(value);
        if !rendered.is_empty() {
            merged.insert(key.clone(), rendered);
        }
    }

    merged
}

/// 根据 connector 模板元数据和运行时参数推断最终类型键和值。
pub(super) fn infer_connector_type(
    meta: Option<&ConnectorMeta>,
    connect: &str,
    params: &BTreeMap<String, String>,
) -> (String, String) {
    let normalized_connect = connect.trim().to_ascii_lowercase();
    let protocol = preferred_value(params, &["protocol"]).to_ascii_lowercase();
    let mut type_key = meta
        .map(|item| item.type_key.trim().to_ascii_lowercase())
        .unwrap_or_else(|| connect.trim().replace(['_', ' '], "-").to_ascii_lowercase());

    if type_key == "syslog" {
        if protocol == "udp" || normalized_connect.contains("udp") {
            type_key = "syslog-udp".to_string();
        } else if protocol == "tcp" || normalized_connect.contains("tcp") {
            type_key = "syslog-tcp".to_string();
        }
    }

    let type_label = connector_type_display_name(&type_key)
        .map(str::to_string)
        .or_else(|| meta.map(|item| item.type_label.clone()))
        .unwrap_or_else(|| type_key.clone());

    (type_key, type_label)
}

/// 按 connector 类型构造页面直接展示的摘要文案。
pub(super) fn build_connector_detail(type_key: &str, params: &BTreeMap<String, String>) -> String {
    match type_key {
        "syslog-udp" | "syslog-tcp" | "tcp" | "udp" => join_detail_segments(&[
            format_prefixed("地址", preferred_value(params, &["addr", "host"])),
            format_prefixed("端口", preferred_value(params, &["port"])),
            format_prefixed("协议", preferred_value(params, &["protocol"])),
        ]),
        "mysql" | "postgres" | "doris" | "clickhouse" => {
            let endpoint = preferred_value(params, &["endpoint", "host"]);
            let (host, port) = split_host_port(endpoint);
            join_detail_segments(&[
                if !host.is_empty() {
                    format!("地址 {host}")
                } else {
                    format_prefixed("地址", endpoint)
                },
                format_prefixed("端口", if !port.is_empty() { &port } else { "" }),
                format_prefixed("数据库", preferred_value(params, &["database"])),
                format_prefixed("数据表", preferred_value(params, &["table"])),
            ])
        }
        "dmdb" => {
            let connection =
                parse_connection_string(preferred_value(params, &["connection_string"]));
            let endpoint = preferred_value(params, &["endpoint"]);
            let (host, port) = split_host_port(endpoint);
            let connection_host = connection
                .get("SERVER")
                .cloned()
                .unwrap_or_else(|| host.clone());
            let connection_port = connection
                .get("TCP_PORT")
                .cloned()
                .unwrap_or_else(|| port.clone());

            join_detail_segments(&[
                if !connection_host.is_empty() {
                    format!("地址 {connection_host}")
                } else {
                    format_prefixed("地址", endpoint)
                },
                format_prefixed("端口", &connection_port),
                format_prefixed("数据库", preferred_value(params, &["database", "schema"])),
                format_prefixed("数据表", preferred_value(params, &["table"])),
            ])
        }
        "file" => {
            let path = join_path_segments(
                preferred_value(params, &["base", "path"]),
                preferred_value(params, &["file", "file_path"]),
            );
            format_prefixed("文件路径", &path)
        }
        "kafka" => join_detail_segments(&[
            format_prefixed("地址", preferred_value(params, &["brokers"])),
            format_prefixed("Topic", preferred_value(params, &["topic"])),
        ]),
        "elasticsearch" => {
            let endpoint = preferred_value(params, &["host", "endpoint"]);
            let (host, endpoint_port) = split_host_port(endpoint);
            let port = preferred_value(params, &["port"]);
            join_detail_segments(&[
                if !host.is_empty() {
                    format!("地址 {host}")
                } else {
                    format_prefixed("地址", endpoint)
                },
                format_prefixed("端口", preferred_non_empty(&[port, &endpoint_port])),
                format_prefixed("索引", preferred_value(params, &["index"])),
            ])
        }
        _ => join_detail_segments(&[
            format_prefixed("地址", preferred_value(params, &["endpoint"])),
            format_prefixed("路径", preferred_value(params, &["api_path"])),
        ]),
    }
}

/// 将 TOML 值转换为页面展示所需的单行字符串。
fn render_runtime_value(value: &toml::Value) -> String {
    match value {
        toml::Value::String(value) => value.trim().to_string(),
        toml::Value::Integer(value) => value.to_string(),
        toml::Value::Float(value) => value.to_string(),
        toml::Value::Boolean(value) => value.to_string(),
        toml::Value::Array(items) => items
            .iter()
            .map(render_runtime_value)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(", "),
        toml::Value::Datetime(value) => value.to_string(),
        toml::Value::Table(_) => String::new(),
    }
}

/// 解析模板默认值，去掉 TOML 字面量外层包装。
pub(super) fn parse_template_default_value(value: &str) -> String {
    let raw = value.trim();
    if raw.is_empty() {
        return String::new();
    }

    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        return raw[1..raw.len() - 1].to_string();
    }

    if raw.starts_with('[') && raw.ends_with(']') {
        return raw[1..raw.len() - 1]
            .split(',')
            .map(|item| item.trim().trim_matches('"').trim_matches('\''))
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
    }

    raw.to_string()
}

/// 解析形如 `SERVER=host;TCP_PORT=5236` 的连接串。
fn parse_connection_string(value: &str) -> HashMap<String, String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .filter_map(|item| {
            let (key, value) = item.split_once('=')?;
            let key = key.trim().to_ascii_uppercase();
            let value = value.trim().to_string();
            (!key.is_empty() && !value.is_empty()).then_some((key, value))
        })
        .collect()
}

/// 将 `host:port` 或 URL 拆成主机和端口。
fn split_host_port(value: &str) -> (String, String) {
    let sanitized = value
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("")
        .trim();
    if sanitized.is_empty() {
        return (String::new(), String::new());
    }

    let Some(index) = sanitized.rfind(':') else {
        return (sanitized.to_string(), String::new());
    };
    if index == 0 || index == sanitized.len() - 1 {
        return (sanitized.to_string(), String::new());
    }

    (
        sanitized[..index].to_string(),
        sanitized[index + 1..].to_string(),
    )
}

/// 按优先级返回第一个非空参数值。
fn preferred_value<'a>(params: &'a BTreeMap<String, String>, keys: &[&str]) -> &'a str {
    for key in keys {
        if let Some(value) = params.get(*key)
            && !value.trim().is_empty()
        {
            return value.as_str();
        }
    }
    ""
}

/// 从一组候选值中选择第一个非空值。
fn preferred_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

/// 非空时拼出 `前缀 值` 形式的摘要片段。
fn format_prefixed(prefix: &str, value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        String::new()
    } else {
        format!("{prefix} {value}")
    }
}

/// 合并多个摘要片段，过滤空值并用中文逗号连接。
fn join_detail_segments(items: &[String]) -> String {
    let filtered = items
        .iter()
        .map(|item| item.trim())
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();
    if filtered.is_empty() {
        "-".to_string()
    } else {
        filtered.join("，")
    }
}

/// 组合基础目录和文件名，构造更友好的路径展示。
fn join_path_segments(base: &str, file: &str) -> String {
    let base = base.trim();
    let file = file.trim();

    if base.is_empty() {
        return file.to_string();
    }
    if file.is_empty() {
        return base.to_string();
    }
    if file.starts_with('/') {
        return file.to_string();
    }

    format!(
        "{}/{}",
        base.trim_end_matches('/'),
        file.trim_start_matches('/')
    )
}
