//! 接入概览中的 WPL 规则包扫描逻辑。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::Path;

use regex::Regex;

use crate::constants::project::{DIR_MODELS, DIR_RULES, DIR_SCHEMAS, DIR_WPL, FILE_WPL_PARSE};
use crate::error::AppError;
use crate::server::RepoLayout;
use crate::utils::SystemKind;

use super::{
    IntegrationRuleFlatItem, IntegrationRuleItem, IntegrationRuleLogType, IntegrationRuleOverview,
};

/// 扫描项目中的规则摘要，按系统返回接入概览页面展示所需的规则侧统计。
pub fn load_integration_rule_overview_from_layout(
    system: SystemKind,
    layout: &RepoLayout,
) -> Result<IntegrationRuleOverview, AppError> {
    match system {
        SystemKind::Wparse => load_wparse_rule_overview_from_layout(layout),
        SystemKind::Wfusion => load_wfusion_rule_overview_from_layout(layout),
    }
}

/// 扫描项目中的 WPL 规则包，并提取接入概览页面展示所需的设备类型与日志类型摘要。
fn load_wparse_rule_overview_from_layout(
    layout: &RepoLayout,
) -> Result<IntegrationRuleOverview, AppError> {
    let wpl_root = layout.models_root.join(DIR_MODELS).join(DIR_WPL);
    if !wpl_root.exists() {
        return Ok(IntegrationRuleOverview {
            system: SystemKind::Wparse,
            items: Vec::new(),
            window_structures: Vec::new(),
            association_rules: Vec::new(),
            window_structure_count: 0,
            association_rule_count: 0,
        });
    }

    let mut package_dirs = fs::read_dir(&wpl_root)
        .map_err(AppError::internal)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    package_dirs.sort();

    let mut items = Vec::new();
    for package_dir in package_dirs {
        let package_key = package_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if package_key.is_empty() || is_ignored_wpl_identifier(&package_key) {
            continue;
        }

        let parse_file = package_dir.join(FILE_WPL_PARSE);
        if !parse_file.exists() {
            continue;
        }

        let content = fs::read_to_string(&parse_file).map_err(AppError::internal)?;
        if let Some(item) = extract_wpl_rule_overview(&content, &package_key) {
            items.push(item);
        }
    }

    items.sort_by(|left, right| left.device_type.cmp(&right.device_type));
    Ok(IntegrationRuleOverview {
        system: SystemKind::Wparse,
        items,
        window_structures: Vec::new(),
        association_rules: Vec::new(),
        window_structure_count: 0,
        association_rule_count: 0,
    })
}

/// 扫描项目中的 WFusion 窗口结构与关联分析规则文件。
fn load_wfusion_rule_overview_from_layout(
    layout: &RepoLayout,
) -> Result<IntegrationRuleOverview, AppError> {
    let models_root = layout.models_root.join(DIR_MODELS);
    let window_structures = collect_named_rule_files(&models_root.join(DIR_SCHEMAS), ".wfs")?;
    let association_rules = collect_named_rule_files(&models_root.join(DIR_RULES), ".wfl")?;
    let window_structure_count = window_structures.len();
    let association_rule_count = association_rules.len();

    Ok(IntegrationRuleOverview {
        system: SystemKind::Wfusion,
        items: Vec::new(),
        window_structures,
        association_rules,
        window_structure_count,
        association_rule_count,
    })
}

/// 判断某个 package / rule 标识是否属于 ignore 类型。
fn is_ignored_wpl_identifier(value: &str) -> bool {
    value.trim().to_ascii_lowercase().starts_with("ignore")
}

/// 解析 `tag(key: "value")` 注解中的键值属性。
fn parse_tag_attributes(raw_tag: &str) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    let pattern = Regex::new(r#"([a-zA-Z0-9_]+)\s*:\s*"([^"]*)""#).expect("valid tag regex");

    for capture in pattern.captures_iter(raw_tag) {
        let key = capture
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let value = capture
            .get(2)
            .map(|value| value.as_str())
            .unwrap_or_default();
        if !key.is_empty() {
            attributes.insert(key.to_string(), value.to_string());
        }
    }

    attributes
}

/// 提取 `tag(...)` 中的实际内容，兼容外层注解包装。
fn extract_tag_payload(raw_annotation: &str) -> &str {
    let pattern = Regex::new(r#"tag\((.*?)\)"#).expect("valid tag wrapper regex");
    pattern
        .captures(raw_annotation)
        .and_then(|capture| capture.get(1).map(|value| value.as_str()))
        .unwrap_or(raw_annotation)
}

/// 从声明行中提取 package / rule 的标识符名称。
fn extract_decl_name(line: &str, keyword: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let remainder = trimmed.strip_prefix(keyword)?.trim_start();
    let name = remainder
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect::<String>();

    if name.is_empty() { None } else { Some(name) }
}

/// 从单个 WPL 文件提取设备类型与日志类型概览。
fn extract_wpl_rule_overview(content: &str, fallback_package: &str) -> Option<IntegrationRuleItem> {
    let mut package_key = fallback_package.trim().to_string();
    let mut package_tag_attributes = HashMap::new();
    let mut grouped = BTreeMap::<String, BTreeSet<String>>::new();
    let mut pending_annotation = String::new();

    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("#[") && line.ends_with(']') {
            pending_annotation = line
                .trim_start_matches("#[")
                .trim_end_matches(']')
                .trim()
                .to_string();
            continue;
        }

        if let Some(name) = extract_decl_name(line, "package") {
            package_key = name;
            package_tag_attributes =
                parse_tag_attributes(extract_tag_payload(pending_annotation.as_str()));
            pending_annotation.clear();
            continue;
        }

        if let Some(rule_key) = extract_decl_name(line, "rule") {
            if rule_key.is_empty() || is_ignored_wpl_identifier(&rule_key) {
                pending_annotation.clear();
                continue;
            }

            let rule_tag_attributes =
                parse_tag_attributes(extract_tag_payload(pending_annotation.as_str()));
            let log_type_name = rule_tag_attributes
                .get("log_desc")
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(rule_key.as_str())
                .trim()
                .to_string();

            grouped.entry(log_type_name).or_default().insert(rule_key);
            pending_annotation.clear();
            continue;
        }

        pending_annotation.clear();
    }

    if package_key.is_empty() || is_ignored_wpl_identifier(&package_key) {
        return None;
    }

    let device_type = package_tag_attributes
        .get("dev_name")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            package_tag_attributes
                .get("dev_type")
                .map(String::as_str)
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or(package_key.as_str())
        .trim()
        .to_string();

    let log_types = grouped
        .into_iter()
        .map(|(log_type_name, rule_keys)| {
            let rule_keys = rule_keys.into_iter().collect::<Vec<_>>();
            let key = format!("{log_type_name}-{}", rule_keys.join("|"));

            IntegrationRuleLogType {
                key,
                log_type_name,
                rule_keys,
            }
        })
        .collect::<Vec<_>>();

    Some(IntegrationRuleItem {
        key: package_key,
        device_type,
        log_types,
    })
}

fn collect_named_rule_files(
    root: &Path,
    extension: &str,
) -> Result<Vec<IntegrationRuleFlatItem>, AppError> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }

    visit_named_rule_files(root, root, extension, &mut files)?;
    files.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(files)
}

fn visit_named_rule_files(
    root: &Path,
    current: &Path,
    extension: &str,
    files: &mut Vec<IntegrationRuleFlatItem>,
) -> Result<(), AppError> {
    let mut entries = fs::read_dir(current)
        .map_err(AppError::internal)?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            visit_named_rule_files(root, &path, extension, files)?;
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some(&extension[1..]) {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .map_err(AppError::internal)?
            .to_path_buf();
        let key = relative.to_string_lossy().replace('\\', "/");
        let name = format_named_rule_name(&relative);
        let content = fs::read_to_string(&path).map_err(AppError::internal)?;
        let rule_names = extract_rule_names(&content);
        files.push(IntegrationRuleFlatItem {
            key,
            name,
            rule_names,
        });
    }

    Ok(())
}

fn format_named_rule_name(relative: &Path) -> String {
    relative
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_default()
        .to_string()
}

/// 提取 WFL/WFS 文件中声明的 rule 名称，避免把字段名如 `rule_id` 当成规则。
fn extract_rule_names(content: &str) -> Vec<String> {
    let pattern =
        Regex::new(r"(?m)^\s*rule\s+([A-Za-z0-9_]+)\b").expect("valid rule declaration regex");
    pattern
        .captures_iter(content)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_string()))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
