//! 通用工具与常量模块。
//!
//! 存放不归属任何业务子模块的常量定义、展示格式化函数及其他零散工具函数。

use crate::constants::config::{
    CONNECTION_FILE_ORDER, CONNECTOR_DISPLAY_FALLBACKS, CONNECTOR_TYPE_DISPLAY_NAMES,
    SINK_FILE_ORDER,
};
use crate::constants::project::SINK_DISPLAY_FALLBACKS;
use chrono::{DateTime, FixedOffset, Utc};

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

/// 根据 connector 文件名推导展示名称。
pub fn fallback_connector_display(file_name: &str) -> Option<&'static str> {
    let normalized = normalize_sink_key(file_name);
    if normalized.is_empty() {
        return None;
    }

    CONNECTOR_DISPLAY_FALLBACKS
        .iter()
        .find(|(pattern, _)| normalize_sink_key(pattern) == normalized)
        .map(|(_, label)| *label)
}

/// 根据 connector 类型标识返回展示名称。
pub fn connector_type_display_name(connector_type: &str) -> Option<&'static str> {
    let normalized = normalize_sink_key(connector_type);
    if normalized.is_empty() {
        return None;
    }

    CONNECTOR_TYPE_DISPLAY_NAMES
        .iter()
        .find(|(pattern, _)| normalize_sink_key(pattern) == normalized)
        .map(|(_, label)| *label)
}

/// 返回连接配置文件的预设排序权重。
pub fn connector_file_sort_order(file_name: &str) -> Option<usize> {
    let normalized = normalize_sink_key(file_name);
    CONNECTION_FILE_ORDER
        .iter()
        .position(|candidate| normalize_sink_key(candidate) == normalized)
}

/// 返回输出配置文件的预设排序权重。
pub fn sink_file_sort_order(file_name: &str) -> Option<usize> {
    let normalized = normalize_sink_key(file_name);
    SINK_FILE_ORDER
        .iter()
        .position(|candidate| normalize_sink_key(candidate) == normalized)
}

/// 将 UTC 时间格式化为北京时间字符串（`YYYY-MM-DD HH:MM:SS`），用于前端展示。
pub fn format_beijing_time(time: DateTime<Utc>) -> String {
    let beijing = FixedOffset::east_opt(8 * 3600).expect("北京时间 UTC+8 偏移固定有效");
    time.with_timezone(&beijing)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}
