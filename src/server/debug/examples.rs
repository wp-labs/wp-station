//! 调试示例库。
//!
//! 示例直接读取双系统的 `default_configs/*/models`，不依赖运行中的 Gitea 工作区。

use crate::error::AppError;
use crate::server::Setting;
use crate::utils::SystemKind;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value};
use std::fs;
use std::path::{Path, PathBuf};

/// 调试示例列表项。
#[derive(Debug, Serialize)]
pub struct DebugExample {
    pub name: String,
    pub system: SystemKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sample_data: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wpl_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub oml_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wfs_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub wfl_code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub scenario_code: String,
}

/// 从指定系统的默认 models 目录加载调试示例。
pub fn load_debug_examples(system: SystemKind) -> Result<Vec<DebugExample>, AppError> {
    let system_dir = match system {
        SystemKind::Wparse => "wparse",
        SystemKind::Wfusion => "wfusion",
    };
    let models_root = Setting::workspace_root()
        .join("default_configs")
        .join(system_dir)
        .join("models");
    match system {
        SystemKind::Wparse => load_wparse_examples(&models_root),
        SystemKind::Wfusion => load_wfusion_examples(&models_root),
    }
}

fn load_wparse_examples(models_root: &Path) -> Result<Vec<DebugExample>, AppError> {
    let wpl_root = models_root.join("wpl");
    if !wpl_root.exists() {
        return Ok(Vec::new());
    }

    let mut rule_dirs = read_child_dirs(&wpl_root)?;
    rule_dirs.sort();
    let mut examples = Vec::new();

    for rule_dir in rule_dirs {
        let parse_file = rule_dir.join("parse.wpl");
        let sample_file = rule_dir.join("sample.dat");
        if !parse_file.is_file() || !sample_file.is_file() {
            continue;
        }

        let name = rule_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let oml_file = models_root.join("oml").join(&name).join("adm.oml");
        examples.push(DebugExample {
            name,
            system: SystemKind::Wparse,
            sample_data: read_text(&sample_file)?,
            wpl_code: read_text(&parse_file)?,
            oml_code: read_optional_text(&oml_file)?,
            wfs_code: String::new(),
            wfl_code: String::new(),
            scenario_code: String::new(),
        });
    }

    Ok(examples)
}

fn load_wfusion_examples(models_root: &Path) -> Result<Vec<DebugExample>, AppError> {
    let scenarios_root = models_root.join("scenarios");
    if !scenarios_root.exists() {
        return Ok(Vec::new());
    }

    let mut scenario_files = read_files_with_extension_recursive(&scenarios_root, "wfg")?;
    scenario_files.sort();
    let use_pattern = Regex::new(r#"(?m)^\s*use\s+"([^"]+)""#).map_err(AppError::internal)?;
    let mut examples = Vec::new();

    for scenario_file in scenario_files {
        let scenario_code = read_text(&scenario_file)?;
        let mut wfs_parts = Vec::new();
        let mut wfl_parts = Vec::new();

        for capture in use_pattern.captures_iter(&scenario_code) {
            let Some(relative_path) = capture.get(1).map(|value| value.as_str()) else {
                continue;
            };
            let referenced = resolve_wfusion_reference(models_root, &scenario_file, relative_path);
            match referenced.extension().and_then(|value| value.to_str()) {
                Some("wfs") => wfs_parts.push(read_optional_text(&referenced)?),
                Some("wfl") => wfl_parts.push(read_optional_text(&referenced)?),
                _ => {}
            }
        }

        let name = scenario_file
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        examples.push(DebugExample {
            name,
            system: SystemKind::Wfusion,
            sample_data: scenario_to_ndjson(&scenario_code),
            wpl_code: String::new(),
            oml_code: String::new(),
            wfs_code: join_non_empty(wfs_parts),
            wfl_code: join_non_empty(wfl_parts),
            scenario_code,
        });
    }

    Ok(examples)
}

fn read_child_dirs(root: &Path) -> Result<Vec<PathBuf>, AppError> {
    let entries = fs::read_dir(root).map_err(AppError::internal)?;
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect())
}

fn read_files_with_extension_recursive(
    root: &Path,
    extension: &str,
) -> Result<Vec<PathBuf>, AppError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = Vec::new();

    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(AppError::internal)? {
            let path = entry.map_err(AppError::internal)?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                files.push(path);
            }
        }
    }

    Ok(files)
}

fn read_text(path: &Path) -> Result<String, AppError> {
    fs::read_to_string(path).map_err(|error| {
        AppError::internal(format!(
            "读取示例文件失败: path={}, error={}",
            path.display(),
            error
        ))
    })
}

fn read_optional_text(path: &Path) -> Result<String, AppError> {
    if path.is_file() {
        read_text(path)
    } else {
        Ok(String::new())
    }
}

fn resolve_wfusion_reference(
    models_root: &Path,
    scenario_file: &Path,
    relative_path: &str,
) -> PathBuf {
    let referenced = Path::new(relative_path);
    let file_name = referenced.file_name().unwrap_or(referenced.as_os_str());
    let category = match referenced.extension().and_then(|value| value.to_str()) {
        Some("wfs") => "schemas",
        Some("wfl") => "rules",
        _ => {
            return scenario_file
                .parent()
                .unwrap_or(models_root)
                .join(referenced);
        }
    };
    let group_name = scenario_file
        .parent()
        .and_then(Path::file_name)
        .unwrap_or_default();

    // default_configs 的场景引用沿用逻辑目录，真实文件按同名规则组归档。
    let grouped = models_root.join(category).join(group_name).join(file_name);
    if grouped.is_file() {
        return grouped;
    }

    let flat = models_root.join(category).join(file_name);
    if flat.is_file() {
        return flat;
    }

    grouped
}

fn join_non_empty(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// 从 WFG 中提取 `use(...) with(n)` 样例，转换为编辑器可直接使用的 NDJSON。
fn scenario_to_ndjson(source: &str) -> String {
    let stream_pattern = Regex::new(r"\bstream\s+([A-Za-z_][A-Za-z0-9_]*)\s+gen\b").ok();
    let use_pattern = Regex::new(r"use\((.*)\)\s+with\((\d+)\)").ok();
    let stream = stream_pattern
        .as_ref()
        .and_then(|pattern| pattern.captures(source))
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_default();
    let mut rows = Vec::new();

    for line in source.lines() {
        let Some(captures) = use_pattern
            .as_ref()
            .and_then(|pattern| pattern.captures(line))
        else {
            continue;
        };
        let args = captures
            .get(1)
            .map(|value| value.as_str())
            .unwrap_or_default();
        let count = captures
            .get(2)
            .and_then(|value| value.as_str().parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 50);
        let mut record = parse_assignments(args);
        if !stream.is_empty() {
            record.insert("_stream".to_string(), Value::String(stream.clone()));
        }

        let base_sequence = rows.len();
        for index in 0..count {
            let mut row = record.clone();
            let sequence = base_sequence + index + 1;
            row.entry("event_id".to_string())
                .or_insert_with(|| Value::String(format!("example-{sequence}")));
            row.entry("log_id".to_string())
                .or_insert_with(|| Value::String(format!("example-{sequence}")));
            row.entry("occur_time".to_string()).or_insert_with(|| {
                Value::Number(
                    (1_700_000_000_000_000_000_i64 + sequence as i64 * 1_000_000_000).into(),
                )
            });
            if !stream.is_empty() {
                row.entry("log_type".to_string())
                    .or_insert_with(|| Value::String(stream.clone()));
            }
            rows.push(Value::Object(row).to_string());
        }
    }

    rows.join("\n")
}

fn parse_assignments(source: &str) -> Map<String, Value> {
    split_arguments(source)
        .into_iter()
        .filter_map(|item| {
            let (key, value) = item.split_once('=')?;
            Some((key.trim().to_string(), parse_scalar(value.trim())))
        })
        .collect()
}

fn split_arguments(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;

    for character in source.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }
        if character == '\\' && quoted {
            current.push(character);
            escaped = true;
            continue;
        }
        if character == '"' {
            quoted = !quoted;
            current.push(character);
            continue;
        }
        if character == ',' && !quoted {
            result.push(current.trim().to_string());
            current.clear();
            continue;
        }
        current.push(character);
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn parse_scalar(value: &str) -> Value {
    if value.starts_with('"') && value.ends_with('"') {
        return serde_json::from_str(value)
            .unwrap_or_else(|_| Value::String(value.trim_matches('"').to_string()));
    }
    if let Ok(number) = value.parse::<i64>() {
        return Value::Number(number.into());
    }
    if let Ok(number) = value.parse::<f64>()
        && let Some(number) = serde_json::Number::from_f64(number)
    {
        return Value::Number(number);
    }
    if let Ok(boolean) = value.parse::<bool>() {
        return Value::Bool(boolean);
    }
    Value::String(value.to_string())
}
