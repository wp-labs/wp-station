//! WPL 规则解析与调试字段转换逻辑。

use crate::error::AppError;
use orion_error::UnifiedReason;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use wp_model_core::model::DataRecord;
use wpl::{
    AnnotationType, WparseReason, WplCode, WplEvaluator, WplExpress, WplPackage, WplStatementType,
};

type RunParseProc = (WplExpress, Vec<AnnotationType>);

/// 调试页字段表格使用的统一字段结构。
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ParsedField {
    pub no: i32,
    pub meta: String,
    pub name: String,
    pub value: String,
}

/// 将 `DataRecord` 转换为前端可直接展示的字段列表。
pub fn record_to_fields(record: &DataRecord) -> Vec<ParsedField> {
    record
        .items
        .iter()
        .enumerate()
        .map(|(index, field)| ParsedField {
            no: index as i32 + 1,
            meta: String::new(),
            name: field.get_name().to_string(),
            value: field.get_value().to_string(),
        })
        .collect()
}

/// 使用 WPL 规则解析原始日志，返回原始 `DataRecord` 供后续 OML 等流程复用。
pub fn warp_check_record(wpl: &str, data: &str) -> Result<DataRecord, AppError> {
    let wpl_package = parse_wpl_package(wpl)?;
    let rule_items = extract_rule_items(&wpl_package)
        .map_err(|err| AppError::wpl_parse(format!("构建 WPL 规则失败: {:?}", err)))?;

    if rule_items.is_empty() {
        return Err(AppError::wpl_parse("WPL 中未找到任何规则"));
    }

    try_parse_with_rules(rule_items, data)
}

/// 解析 WPL 代码为包结构，保留更完整的语法错误上下文。
fn parse_wpl_package(wpl: &str) -> Result<WplPackage, AppError> {
    let code = WplCode::build(PathBuf::new(), wpl)?;
    Ok(code.parse_pkg()?)
}

/// 依次尝试所有规则，返回首个成功解析的 `DataRecord`。
fn try_parse_with_rules(rule_items: Vec<RunParseProc>, data: &str) -> Result<DataRecord, AppError> {
    let mut max_depth = 0;
    let mut best_wpl = 1;

    for (index, (wpl_express, _funcs)) in rule_items.into_iter().enumerate() {
        let evaluator = WplEvaluator::from(&wpl_express, None).map_err(|err| {
            AppError::wpl_parse(format!(
                "规则 {} 构建失败: {}",
                index + 1,
                format_wpl_error(&err)
            ))
        })?;

        match evaluator.proc(0, data, 0) {
            Ok((tdc, _pipeline)) => return Ok(tdc),
            Err(err) => {
                best_wpl = index + 1;
                if matches!(err.reason(), WparseReason::Uvs(UnifiedReason::DataError))
                    && max_depth == 0
                {
                    max_depth = 1;
                }
            }
        }
    }

    let friendly_hint = build_best_match_hint(data, max_depth, best_wpl);
    Err(AppError::wpl_best_error(max_depth, friendly_hint))
}

/// 构造友好的失败提示，帮助定位规则停止匹配的大概位置。
fn build_best_match_hint(data: &str, depth: usize, rule_name: usize) -> String {
    let chars: Vec<char> = data.chars().collect();
    if chars.is_empty() {
        return format!(
            "rule {rule_name} Achieved the best match, but the log is empty and the location cannot be determined."
        );
    }

    let bounded_depth = depth.min(chars.len().saturating_sub(1));
    let window_before = 20;
    let window_after = 20;
    let ctx_start = bounded_depth.saturating_sub(window_before);
    let ctx_end = (bounded_depth + window_after + 1).min(chars.len());

    let snippet: String = chars[ctx_start..ctx_end].iter().collect();
    let prefix = if ctx_start > 0 { "…" } else { "" };
    let suffix = if ctx_end < chars.len() { "…" } else { "" };
    let snippet_line = format!("{prefix}{snippet}{suffix}");

    let pointer_offset = prefix.chars().count() + bounded_depth.saturating_sub(ctx_start);
    let pointer = format!(
        "{}↑ This is the final matching position.",
        " ".repeat(pointer_offset)
    );

    format!(
        "规则 {rule_name} 最深匹配字符序号 {pos}。日志上下文:\n{snippet_line}\n{pointer}",
        pos = bounded_depth + 1
    )
}

/// 优先使用 `Display` 输出错误，避免空字符串时丢失上下文。
fn format_wpl_error<T>(err: &T) -> String
where
    T: std::fmt::Display + std::fmt::Debug,
{
    let display = err.to_string();
    if display.trim().is_empty() {
        format!("{err:?}")
    } else {
        display
    }
}

/// 从 WPL 包中提取可执行规则表达式及其注解信息。
fn extract_rule_items(wpl_package: &WplPackage) -> anyhow::Result<Vec<RunParseProc>> {
    let mut rule_pairs = Vec::with_capacity(wpl_package.rules.len());

    for rule in wpl_package.rules.iter() {
        let rule_obj = match &rule.statement {
            WplStatementType::Express(code) => code.clone(),
        };
        let funcs = AnnotationType::convert(rule.statement.tags());
        rule_pairs.push((rule_obj, funcs));
    }
    Ok(rule_pairs)
}
