//! connector 模板扫描与字段提取逻辑。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::constants::project::{DIR_CONNECTORS, DIR_SINK_D, DIR_SOURCE_D};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::RepoLayout;

use super::{ConfigTemplateDef, ConfigTemplateField};

/// connector 模板扫描后的中间结构。
#[derive(Debug, Clone)]
struct ConnectorTemplateSource {
    template_file: String,
    connect: String,
    connector_type: String,
    allow_override: Vec<String>,
    params: BTreeMap<String, TomlValue>,
}

/// 轻量 TOML 值表示，用于模板字段提取。
#[derive(Debug, Clone)]
enum TomlValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Array(Vec<TomlValue>),
    Table(BTreeMap<String, TomlValue>),
}

/// 返回指定 scope 的配置模板列表。
pub fn list_config_templates(scope: RuleType) -> Result<Vec<ConfigTemplateDef>, AppError> {
    let layout = crate::server::Setting::load().wparse_layout();
    list_config_templates_from_layout(&layout, scope)
}

/// 根据项目布局扫描 connectors 目录并构建模板。
pub fn list_config_templates_from_layout(
    layout: &RepoLayout,
    scope: RuleType,
) -> Result<Vec<ConfigTemplateDef>, AppError> {
    let connectors_dir = match scope {
        RuleType::Source => layout
            .connectors_root
            .join(DIR_CONNECTORS)
            .join(DIR_SOURCE_D),
        RuleType::Sink => layout.connectors_root.join(DIR_CONNECTORS).join(DIR_SINK_D),
        _ => {
            return Err(AppError::validation(
                "配置模板 scope 仅支持 source 或 sink".to_string(),
            ));
        }
    };

    let mut items = scan_connector_templates(&connectors_dir)?
        .into_iter()
        .map(|source| convert_connector_template(scope, source))
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        left.template_file
            .cmp(&right.template_file)
            .then_with(|| left.connect.cmp(&right.connect))
    });
    Ok(items)
}

/// 扫描单个 connectors 目录下的所有 TOML 模板文件。
fn scan_connector_templates(dir: &Path) -> Result<Vec<ConnectorTemplateSource>, AppError> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut result = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !file_name.ends_with(".toml") {
            continue;
        }

        let content = std::fs::read_to_string(&path).map_err(AppError::internal)?;
        let connector = parse_connector_template(file_name.to_string(), &content)?;
        result.push(connector);
    }

    Ok(result)
}

/// 解析单个 connector 模板文件中的 id/type/allow_override/params 信息。
fn parse_connector_template(
    template_file: String,
    content: &str,
) -> Result<ConnectorTemplateSource, AppError> {
    let id = first_string_assignment(content, "id")
        .ok_or_else(|| AppError::validation(format!("connector 模板缺少 id: {template_file}")))?;
    let connector_type = first_string_assignment(content, "type")
        .ok_or_else(|| AppError::validation(format!("connector 模板缺少 type: {template_file}")))?;

    let allow_override = parse_allow_override(content);
    let params = parse_connector_params(content);

    Ok(ConnectorTemplateSource {
        template_file,
        connect: id,
        connector_type,
        allow_override,
        params,
    })
}

/// 将扫描结果转换为 Station 使用的模板定义。
fn convert_connector_template(
    scope: RuleType,
    source: ConnectorTemplateSource,
) -> ConfigTemplateDef {
    let default_enabled = matches!(
        super::template_id_from_file(&source.template_file).as_str(),
        "file-default"
    )
    .then_some(true)
    .or(Some(false))
    .filter(|_| matches!(scope, RuleType::Source));

    let fields = source
        .allow_override
        .into_iter()
        .map(|name| {
            let required = is_required_field(scope, &source.connector_type, &name);
            let advanced = is_advanced_field(scope, &name);
            let default_value = if advanced {
                None
            } else {
                source.params.get(&name).map(render_toml_value)
            };

            ConfigTemplateField {
                name,
                required,
                default_value,
                advanced,
            }
        })
        .collect::<Vec<_>>();

    ConfigTemplateDef {
        scope,
        template_file: source.template_file,
        connect: source.connect,
        connector_type: source.connector_type,
        default_enabled,
        fields,
    }
}

/// 判断某字段是否应在模板表单中标为必填。
fn is_required_field(scope: RuleType, connector_type: &str, field_name: &str) -> bool {
    match scope {
        RuleType::Source => match field_name {
            "base" | "file" | "addr" | "port" | "brokers" | "topic" | "endpoint" | "username"
            | "password" | "database" | "table" | "connection_string" | "driver" | "dsn"
            | "cursor_column" => true,
            _ => matches!(
                (connector_type, field_name),
                ("doris", "user") | ("elasticsearch", "host") | ("victoriametrics", "insert_url")
            ),
        },
        RuleType::Sink => match field_name {
            "base" | "file" | "addr" | "port" | "brokers" | "topic" | "endpoint" | "username"
            | "database" | "table" | "host" | "index" | "insert_url" => true,
            _ => matches!(
                (connector_type, field_name),
                ("doris", "user")
                    | ("mysql", "password")
                    | ("postgres", "password")
                    | ("clickhouse", "database")
                    | ("clickhouse", "table")
            ),
        },
        _ => false,
    }
}

/// 判断某字段是否应被视作高级字段，从模板快速插入中省略。
fn is_advanced_field(scope: RuleType, field_name: &str) -> bool {
    let common = matches!(
        field_name,
        "batch"
            | "batch_size"
            | "poll_interval_ms"
            | "error_backoff_ms"
            | "connect_timeout_secs"
            | "query_timeout_secs"
            | "timeout_secs"
            | "max_retries"
            | "flush_interval_secs"
            | "udp_recv_buffer"
            | "tcp_recv_bytes"
            | "instances"
            | "headers"
            | "sync"
            | "max_backoff"
            | "attach_meta_tags"
            | "strip_header"
            | "create_time_field"
    );

    if common {
        return true;
    }

    matches!(scope, RuleType::Sink) && matches!(field_name, "num_partitions" | "replication")
}

/// 解析 `allow_override = [...]` 列表。
fn parse_allow_override(content: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut in_allow_override = false;
    let mut buffer = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if !in_allow_override {
            if let Some((lhs, rhs)) = trimmed.split_once('=')
                && lhs.trim() == "allow_override"
            {
                in_allow_override = true;
                buffer.push_str(rhs.trim());
                if rhs.contains(']') {
                    break;
                }
            }
            continue;
        }

        buffer.push(' ');
        buffer.push_str(trimmed);
        if trimmed.contains(']') {
            break;
        }
    }

    if buffer.is_empty() {
        return values;
    }

    let start = buffer.find('[').unwrap_or(0);
    let end = buffer.rfind(']').unwrap_or(buffer.len());
    let inner = &buffer[start + 1..end];
    for item in inner.split(',') {
        let value = item.trim().trim_matches('"').trim();
        if !value.is_empty() {
            values.push(value.to_string());
        }
    }

    values
}

/// 解析 connector 模板中的 `[connectors.params]` 和嵌套子表字段。
fn parse_connector_params(content: &str) -> BTreeMap<String, TomlValue> {
    let mut params = BTreeMap::new();
    let mut section_stack: Vec<String> = Vec::new();

    for line in content.lines() {
        let trimmed = strip_inline_comment(line).trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("[[") && trimmed.ends_with("]]") {
            section_stack.clear();
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let section = trimmed.trim_start_matches('[').trim_end_matches(']');
            section_stack = section
                .split('.')
                .map(|part| part.trim().to_string())
                .collect();
            continue;
        }

        if !matches!(
            section_stack.as_slice(),
            [head, tail @ ..]
                if head == DIR_CONNECTORS && tail.first().map(|s| s.as_str()) == Some("params")
        ) {
            continue;
        }

        let Some((lhs, rhs)) = trimmed.split_once('=') else {
            continue;
        };
        let key = lhs.trim();
        if key.is_empty() {
            continue;
        }

        let value = parse_toml_scalar_or_array(rhs.trim());
        let nested_path = &section_stack[2..];
        if nested_path.is_empty() {
            params.insert(key.to_string(), value);
        } else {
            let root = params
                .entry(nested_path[0].clone())
                .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
            insert_nested_table_value(root, &nested_path[1..], key, value);
        }
    }

    params
}

/// 向嵌套 `TomlValue::Table` 中递归插入值。
fn insert_nested_table_value(
    root: &mut TomlValue,
    nested_path: &[String],
    key: &str,
    value: TomlValue,
) {
    let TomlValue::Table(table) = root else {
        return;
    };

    if nested_path.is_empty() {
        table.insert(key.to_string(), value);
        return;
    }

    let child = table
        .entry(nested_path[0].clone())
        .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
    insert_nested_table_value(child, &nested_path[1..], key, value);
}

/// 解析 TOML 标量或数组字面量。
fn parse_toml_scalar_or_array(raw: &str) -> TomlValue {
    let value = raw.trim();
    if value.starts_with('[') && value.ends_with(']') {
        let inner = &value[1..value.len() - 1];
        let items = inner
            .split(',')
            .filter_map(|item| {
                let trimmed = item.trim();
                (!trimmed.is_empty()).then(|| parse_toml_scalar_or_array(trimmed))
            })
            .collect::<Vec<_>>();
        return TomlValue::Array(items);
    }

    if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
        return TomlValue::String(value[1..value.len() - 1].to_string());
    }

    if matches!(value, "true" | "false") {
        return TomlValue::Boolean(value == "true");
    }

    if let Ok(number) = value.replace('_', "").parse::<i64>() {
        return TomlValue::Integer(number);
    }

    if let Ok(number) = value.replace('_', "").parse::<f64>() {
        return TomlValue::Float(number);
    }

    TomlValue::String(value.to_string())
}

/// 将内部 TOML 值恢复为模板默认值展示字符串。
fn render_toml_value(value: &TomlValue) -> String {
    match value {
        TomlValue::String(value) => format!("\"{value}\""),
        TomlValue::Integer(value) => value.to_string(),
        TomlValue::Float(value) => value.to_string(),
        TomlValue::Boolean(value) => value.to_string(),
        TomlValue::Array(items) => {
            let values = items.iter().map(render_toml_value).collect::<Vec<_>>();
            format!("[{}]", values.join(", "))
        }
        TomlValue::Table(_) => "{}".to_string(),
    }
}

/// 去掉行内注释，但保留字符串字面量中的 `#`。
fn strip_inline_comment(line: &str) -> &str {
    let mut in_string = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_string = !in_string,
            '#' if !in_string => return &line[..index],
            _ => {}
        }
    }
    line
}

/// 读取首个 `key = "value"` 形式的字符串赋值。
fn first_string_assignment(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| parse_string_assignment(line, key))
}

/// 解析单行字符串赋值。
fn parse_string_assignment(line: &str, key: &str) -> Option<String> {
    let trimmed = strip_inline_comment(line).trim();
    let (lhs, rhs) = trimmed.split_once('=')?;
    if lhs.trim() != key {
        return None;
    }

    let rhs = rhs.trim();
    if !rhs.starts_with('"') {
        return None;
    }

    let rhs = &rhs[1..];
    let end = rhs.find('"')?;
    Some(rhs[..end].to_string())
}

#[allow(dead_code)]
fn _connector_dir(layout: &RepoLayout, scope: RuleType) -> PathBuf {
    match scope {
        RuleType::Source => layout.infra_root.join(DIR_CONNECTORS).join(DIR_SOURCE_D),
        RuleType::Sink => layout.infra_root.join(DIR_CONNECTORS).join(DIR_SINK_D),
        _ => layout.infra_root.join(DIR_CONNECTORS),
    }
}
