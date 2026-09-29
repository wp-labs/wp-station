//! 模板片段渲染与内容合并逻辑。

use std::collections::HashSet;

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::RepoLayout;

use super::{ConfigTemplateDef, ConfigTemplateField, RenderedConfigTemplate};

/// 渲染配置模板预览与合并结果。
pub fn render_config_template(
    layout: &RepoLayout,
    scope: RuleType,
    template_id: &str,
    current_content: &str,
) -> Result<RenderedConfigTemplate, AppError> {
    let templates = super::list_config_templates_from_layout(layout, scope)?;
    let template = templates
        .iter()
        .find(|item| template_id_from_file(&item.template_file) == template_id)
        .cloned()
        .ok_or_else(|| AppError::not_found(format!("未找到配置模板: {template_id}")))?;

    let instance_name = next_unique_instance_name(scope, current_content, template_id);
    let display_name = display_name_from_file(&template.template_file);
    let required_fields = template
        .fields
        .iter()
        .filter(|field| field.required)
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();
    let inserted_fields = template
        .fields
        .iter()
        .filter(|field| !field.advanced && field.default_value.is_some())
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();
    let omitted_fields = template
        .fields
        .iter()
        .filter(|field| field.advanced)
        .map(|field| field.name.clone())
        .collect::<Vec<_>>();

    let warnings = Vec::new();
    let snippet = match scope {
        RuleType::Source => render_source_template(&template, &instance_name),
        RuleType::Sink => render_sink_template(&template, &instance_name, current_content),
        _ => {
            return Err(AppError::validation(
                "配置模板 scope 仅支持 source 或 sink".to_string(),
            ));
        }
    };
    let content = merge_template_content(current_content, &snippet);

    Ok(RenderedConfigTemplate {
        scope,
        template_id: template_id.to_string(),
        template_file: template.template_file,
        display_name,
        connect: template.connect,
        connector_type: template.connector_type,
        instance_name,
        required_fields,
        inserted_fields,
        omitted_fields,
        warnings,
        snippet,
        content,
    })
}

/// 将 source 模板定义渲染成可直接插入 topology 的片段。
fn render_source_template(template: &ConfigTemplateDef, instance_name: &str) -> String {
    let mut lines = vec![
        "[[sources]]".to_string(),
        format!("key = \"{instance_name}\""),
        format!("enable = {}", template.default_enabled.unwrap_or(false)),
        format!("connect = \"{}\"", template.connect),
        "tags = []".to_string(),
    ];

    let params = render_template_fields(&template.fields);
    if !params.is_empty() {
        lines.push(String::new());
        lines.push("[sources.params]".to_string());
        lines.extend(params);
    }

    lines.join("\n")
}

/// 将 sink 模板定义渲染成可直接插入业务 sink 拓扑的片段。
fn render_sink_template(
    template: &ConfigTemplateDef,
    instance_name: &str,
    current_content: &str,
) -> String {
    let mut lines = Vec::new();

    if !has_named_section(current_content, "[sink_group]") {
        if !has_assignment(current_content, "version") {
            lines.push(r#"version = "1.0""#.to_string());
            lines.push(String::new());
        }

        lines.extend([
            "[sink_group]".to_string(),
            r#"name = "all""#.to_string(),
            r#"oml = ["*"]"#.to_string(),
            "parallel = 1".to_string(),
            String::new(),
        ]);
    }

    lines.push("[[sink_group.sinks]]".to_string());
    lines.push(format!("name = \"{instance_name}\""));
    lines.push(format!("connect = \"{}\"", template.connect));
    lines.push("tags = []".to_string());

    let params = render_template_fields(&template.fields);
    if !params.is_empty() {
        lines.push(String::new());
        lines.push("[sink_group.sinks.params]".to_string());
        lines.extend(params);
    }

    lines.join("\n")
}

/// 渲染非高级字段的默认参数列表。
fn render_template_fields(fields: &[ConfigTemplateField]) -> Vec<String> {
    fields
        .iter()
        .filter(|field| !field.advanced)
        .filter_map(|field| {
            field
                .default_value
                .as_ref()
                .map(|value| format!("{} = {}", field.name, value))
        })
        .collect()
}

/// 为新模板实例生成当前内容中唯一的名称。
fn next_unique_instance_name(scope: RuleType, current_content: &str, template_id: &str) -> String {
    let normalized_suffix = template_id.replace('-', "_");
    let base = match scope {
        RuleType::Source => format!("gen_{normalized_suffix}"),
        RuleType::Sink => format!("all_{normalized_suffix}"),
        _ => normalized_suffix,
    };
    let key_name = if matches!(scope, RuleType::Source) {
        "key"
    } else {
        "name"
    };

    let existing = extract_string_assignments(current_content, key_name)
        .into_iter()
        .collect::<HashSet<_>>();

    if !existing.contains(&base) {
        return base;
    }

    for index in 2.. {
        let candidate = format!("{base}_{index}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }

    base
}

/// 从现有内容中提取所有指定 key 的字符串赋值。
fn extract_string_assignments(content: &str, key: &str) -> Vec<String> {
    content
        .lines()
        .filter_map(|line| parse_string_assignment(line, key))
        .collect()
}

/// 解析单行 `key = "value"` 字符串赋值。
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

/// 判断内容中是否已经存在具名 section。
fn has_named_section(content: &str, section: &str) -> bool {
    content.lines().any(|line| line.trim() == section)
}

/// 判断内容中是否已经存在指定 key 的赋值。
fn has_assignment(content: &str, key: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = strip_inline_comment(line).trim();
        let Some((lhs, _)) = trimmed.split_once('=') else {
            return false;
        };
        lhs.trim() == key
    })
}

/// 将新片段追加到现有内容尾部，保持空行分隔。
fn merge_template_content(current_content: &str, snippet: &str) -> String {
    let trimmed = current_content.trim_end_matches(['\r', '\n']);
    if trimmed.is_empty() {
        return format!("{snippet}\n");
    }

    format!("{trimmed}\n\n{snippet}\n")
}

/// 根据模板文件名生成稳定的模板 ID。
pub fn template_id_from_file(template_file: &str) -> String {
    strip_numeric_prefix(template_file)
        .trim_end_matches(".toml")
        .to_string()
}

/// 根据模板文件名生成展示名。
pub fn display_name_from_file(template_file: &str) -> String {
    strip_numeric_prefix(template_file).to_string()
}

/// 去掉形如 `10-` 的数字前缀。
fn strip_numeric_prefix(template_file: &str) -> &str {
    let Some((prefix, rest)) = template_file.split_once('-') else {
        return template_file;
    };

    if prefix.chars().all(|ch| ch.is_ascii_digit()) {
        rest
    } else {
        template_file
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
