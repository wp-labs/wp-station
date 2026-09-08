//! 调试功能业务逻辑层。
//!
//! 汇总解析调试、格式化和知识库调试等能力，
//! 对外保持统一的调试业务入口。

mod examples;
mod knowledge;
mod wfusion_replay;

pub use self::examples::{DebugExample, load_debug_examples};
use self::wfusion_replay::replay_events as replay_wfusion_events;
use crate::error::AppError;
use crate::utils::warp_check_record;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use toml_edit::visit_mut::{self, VisitMut};
use wf_engine::alert::{OutputRecord, data_record_to_json_string};
use wf_lang::{CheckError, Severity};
use wp_data_fmt::{FormatType, Json, RecordFormatter};
use wp_model_core::model::DataRecord;

/// 通用格式化类型。
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DebugFormatKind {
    Wpl,
    Oml,
    Wfs,
    Wfl,
    Wfg,
    Toml,
}

impl DebugFormatKind {
    fn label(self) -> &'static str {
        match self {
            Self::Wpl => "WPL",
            Self::Oml => "OML",
            Self::Wfs => "WFS",
            Self::Wfl => "WFL",
            Self::Wfg => "WFG",
            Self::Toml => "TOML",
        }
    }
}

pub use self::knowledge::{
    debug_knowledge_query_fields_logic, debug_knowledge_query_logic,
    debug_knowledge_query_rows_logic, debug_knowledge_status_logic,
};

/// API 层之间共享的解析结果缓存。
pub type SharedRecord = Arc<Mutex<Option<DataRecord>>>;

// ============ 请求参数结构体 ============

/// 调试解析请求体。
#[derive(Deserialize)]
pub struct DebugParseRequest {
    pub rules: String,
    pub logs: String,
}

/// 调试转换请求体。
#[derive(Deserialize)]
pub struct DebugTransformRequest {
    pub oml: String,
    #[serde(default)]
    pub parse_result: Option<serde_json::Value>,
}

/// 知识库状态查询参数。
#[derive(Deserialize)]
pub struct DebugKnowledgeStatusQuery {}

/// 调试知识库查询请求体。
#[derive(Deserialize)]
pub struct DebugKnowledgeQueryRequest {
    pub table: String,
    #[serde(default)]
    pub source_kind: Option<String>,
    pub sql: String,
}

/// WFusion 规则编辑器解析请求体。
#[derive(Deserialize)]
pub struct DebugWfusionRuleEditorParseRequest {
    pub events_ndjson: String,
    pub wfs: String,
    pub wfl: String,
}

// ============ 响应结构体 ============

/// 解析结果原始记录响应体。
#[derive(Serialize, Deserialize)]
pub struct RecordResponseRaw {
    pub fields: DataRecord,
    pub format_json: String,
    pub multiple_logs: bool,
}

/// 知识库状态列表项。
#[derive(Serialize)]
pub struct DebugKnowledgeStatusItem {
    pub tag_name: String,
    pub label: String,
    pub suggested_sql: String,
    pub source_kind: String,
    pub is_active: bool,
}

/// 调试知识库查询响应体。
#[derive(Serialize)]
pub struct DebugKnowledgeQueryResponse {
    pub success: bool,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
}

/// WFusion 规则编辑器诊断项。
#[derive(Serialize)]
pub struct DebugWfusionRuleEditorDiagnostic {
    pub severity: String,
    pub file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// WFusion 规则编辑器试跑统计。
#[derive(Serialize)]
pub struct DebugWfusionRuleEditorSummary {
    pub event_count: u64,
    pub match_count: u64,
    pub error_count: u64,
}

/// WFusion 规则编辑器解析响应。
#[derive(Serialize)]
pub struct DebugWfusionRuleEditorParseResponse {
    pub success: bool,
    pub stage: String,
    pub diagnostics: Vec<DebugWfusionRuleEditorDiagnostic>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<DebugWfusionRuleEditorSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alerts: Vec<serde_json::Value>,
}

/// 将 WFusion 内部告警导出为更接近最终 sink 输出的 JSON。
///
/// 优先走 `to_data_record()`，这样能把 `yield (...)` 里的字段一并导出；
/// 若导出失败，则回退为基础告警头字段，避免编辑器结果完全不可见。
fn export_wfusion_alert_json(alert: OutputRecord) -> serde_json::Value {
    let exported_json = alert
        .to_data_record()
        .and_then(|record| data_record_to_json_string(&record))
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok());

    exported_json
        .unwrap_or_else(|| serde_json::to_value(&alert).unwrap_or_else(|_| serde_json::json!({})))
}

/// 解析日志并返回字段列表
pub async fn debug_parse_logic(
    shared_record: Arc<Mutex<Option<DataRecord>>>,
    rules: String,
    logs: String,
) -> Result<RecordResponseRaw, AppError> {
    let first_log = logs
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    // 调用 warp_check_record 获取 DataRecord
    let record = warp_check_record(&rules, first_log.first().unwrap_or(&""))?;

    // 存入 SharedRecord，供后续转换使用
    let mut record_guard = shared_record.lock().await;
    *record_guard = Some(record.clone());

    // 生成 format_json
    let formatter = FormatType::Json(Json);
    let json_string = formatter.fmt_record(&record);

    // 返回 RecordResponseRaw，包含完整的 DataRecord 和 format_json
    Ok(RecordResponseRaw {
        fields: record,
        format_json: json_string,
        multiple_logs: first_log.len() > 1,
    })
}

/// 使用最近一次解析结果执行 OML 转换
pub async fn debug_transform_logic(
    shared_record: Arc<Mutex<Option<DataRecord>>>,
    oml: String,
) -> Result<RecordResponseRaw, AppError> {
    let record = {
        let record_guard = shared_record.lock().await;
        record_guard.clone()
    }
    .ok_or(AppError::NoParseResult)?;

    let transformed = crate::utils::oml::convert_record(&oml, record).await?;
    let formatter = FormatType::Json(Json);
    let json_string = formatter.fmt_record(&transformed);

    Ok(RecordResponseRaw {
        fields: transformed,
        format_json: json_string,
        multiple_logs: false,
    })
}

/// 解析并试跑 WFusion 规则编辑器输入。
pub fn debug_wfusion_rule_editor_parse_logic(
    req: DebugWfusionRuleEditorParseRequest,
) -> DebugWfusionRuleEditorParseResponse {
    let wfs_path = Path::new("schemas/editor.wfs");
    let wfl_path = Path::new("rules/editor.wfl");
    let schemas = match wf_lang::parse_wfs(&req.wfs) {
        Ok(schemas) => schemas,
        Err(err) => {
            return build_rule_editor_failure(
                "wfs_parse",
                diagnostics_from_lang_error(
                    &err,
                    "error",
                    wfs_path.display().to_string(),
                    Some("请先修复 WFS 语法错误"),
                ),
            );
        }
    };

    let wfl_ast = match wf_lang::parse_wfl_with_diagnostics(&req.wfl, wfl_path) {
        Ok(wfl_ast) => wfl_ast,
        Err(err) => {
            return build_rule_editor_failure(
                "wfl_parse",
                diagnostics_from_lang_error(
                    &err,
                    "error",
                    wfl_path.display().to_string(),
                    Some("请先修复 WFL 语法错误"),
                ),
            );
        }
    };

    let errors = wf_lang::check_wfl(&wfl_ast, &schemas);
    let warnings = wf_lang::lint_wfl(&wfl_ast, &schemas);
    let diagnostics = errors
        .iter()
        .flat_map(|diag| map_check_diagnostic(diag, &wfl_ast, &req.wfl, wfl_path))
        .chain(
            warnings
                .iter()
                .flat_map(|diag| map_check_diagnostic(diag, &wfl_ast, &req.wfl, wfl_path)),
        )
        .collect::<Vec<_>>();

    if !errors.is_empty() {
        return DebugWfusionRuleEditorParseResponse {
            success: false,
            stage: "wfl_check".to_string(),
            diagnostics,
            summary: None,
            alerts: vec![],
        };
    }

    let reader = BufReader::new(req.events_ndjson.as_bytes());
    let replay_result = match replay_wfusion_events(&req.wfl, &schemas, reader, false) {
        Ok(result) => result,
        Err(err) => {
            return build_rule_editor_failure(
                "replay",
                vec![build_message_diagnostic(
                    "error",
                    "events.ndjson".to_string(),
                    None,
                    err.to_string(),
                    Some("请检查 NDJSON 的 _stream、字段类型和时间字段是否与 WFS/WFL 一致"),
                )],
            );
        }
    };

    let alerts = replay_result
        .alerts
        .into_iter()
        .map(export_wfusion_alert_json)
        .collect::<Vec<_>>();

    DebugWfusionRuleEditorParseResponse {
        success: true,
        stage: "success".to_string(),
        diagnostics,
        summary: Some(DebugWfusionRuleEditorSummary {
            event_count: replay_result.event_count,
            match_count: replay_result.match_count,
            error_count: replay_result.error_count,
        }),
        alerts,
    }
}

/// WPL 代码格式化
pub fn wpl_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Wpl, code)
}

/// OML 代码格式化
pub fn oml_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Oml, code)
}

/// WFS 代码格式化
pub fn wfs_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Wfs, code)
}

/// WFL 代码格式化
pub fn wfl_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Wfl, code)
}

/// WFG 代码格式化
pub fn wfg_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Wfg, code)
}

/// TOML 代码格式化
pub fn toml_format_logic(code: String) -> Result<String, AppError> {
    format_code_logic(DebugFormatKind::Toml, code)
}

/// 通用代码格式化。
pub fn format_code_logic(kind: DebugFormatKind, code: String) -> Result<String, AppError> {
    let formatted = match kind {
        DebugFormatKind::Wpl => tree_sitter_wpl::format(&code).map_err(|e| e.to_string()),
        DebugFormatKind::Oml => tree_sitter_oml::format(&code).map_err(|e| e.to_string()),
        DebugFormatKind::Wfs => tree_sitter_wfl::format_wfs(&code).map_err(|e| e.to_string()),
        DebugFormatKind::Wfl => tree_sitter_wfl::format_wfl(&code).map_err(|e| e.to_string()),
        DebugFormatKind::Wfg => tree_sitter_wfl::format_wfg(&code).map_err(|e| e.to_string()),
        DebugFormatKind::Toml => format_toml(&code).map_err(|e| e.to_string()),
    };

    formatted.map_err(|e| AppError::validation(format!("格式化 {} 代码失败: {}", kind.label(), e)))
}

/// TOML 格式化：保留注释与表结构的同时，统一基础缩进与空白。
fn format_toml(content: &str) -> Result<String, toml_edit::TomlError> {
    let mut document = content.parse::<toml_edit::DocumentMut>()?;
    let mut formatter = TomlSpacingFormatter;
    formatter.visit_document_mut(&mut document);
    document.as_table_mut().fmt();
    Ok(document.to_string())
}

/// 仅规范 TOML 的键值分隔空白，保留注释与原有表结构。
struct TomlSpacingFormatter;

impl VisitMut for TomlSpacingFormatter {
    fn visit_table_like_kv_mut(
        &mut self,
        mut key: toml_edit::KeyMut<'_>,
        node: &mut toml_edit::Item,
    ) {
        let key_prefix = key.leaf_decor().prefix().cloned();
        key.leaf_decor_mut().clear();
        if let Some(prefix) = key_prefix {
            key.leaf_decor_mut().set_prefix(prefix);
        }

        visit_mut::visit_table_like_kv_mut(self, key, node);
    }

    fn visit_value_mut(&mut self, node: &mut toml_edit::Value) {
        preserve_value_suffix_and_reset_prefix(node);
        visit_mut::visit_value_mut(self, node);
    }

    fn visit_array_mut(&mut self, node: &mut toml_edit::Array) {
        visit_mut::visit_array_mut(self, node);
        node.fmt();
    }

    fn visit_inline_table_mut(&mut self, node: &mut toml_edit::InlineTable) {
        visit_mut::visit_inline_table_mut(self, node);
        node.fmt();
    }
}

/// 清理值前缀中的显式空白，让编码时回退到默认的 `= ` 形式，同时保留尾部注释。
fn preserve_value_suffix_and_reset_prefix(value: &mut toml_edit::Value) {
    let suffix = value.decor().suffix().cloned();
    value.decor_mut().clear();
    if let Some(suffix) = suffix {
        value.decor_mut().set_suffix(suffix);
    }
}

fn build_rule_editor_failure(
    stage: &str,
    diagnostics: Vec<DebugWfusionRuleEditorDiagnostic>,
) -> DebugWfusionRuleEditorParseResponse {
    DebugWfusionRuleEditorParseResponse {
        success: false,
        stage: stage.to_string(),
        diagnostics,
        summary: None,
        alerts: vec![],
    }
}

fn build_message_diagnostic(
    severity: &str,
    file: String,
    category: Option<String>,
    message: String,
    hint: Option<&str>,
) -> DebugWfusionRuleEditorDiagnostic {
    DebugWfusionRuleEditorDiagnostic {
        severity: severity.to_string(),
        file,
        category,
        message,
        rule: None,
        test: None,
        line: None,
        column: None,
        snippet: None,
        hint: hint.map(|value| value.to_string()),
    }
}

fn map_check_diagnostic(
    diag: &CheckError,
    wfl_ast: &wf_lang::ast::WflFile,
    wfl_source: &str,
    wfl_path: &Path,
) -> Vec<DebugWfusionRuleEditorDiagnostic> {
    let severity = match diag.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    };
    let formatted =
        wf_lang::diagnostics::format_check_error_with_source(diag, wfl_ast, wfl_source, wfl_path);
    parse_structured_diagnostics(&formatted, severity, wfl_path.display().to_string(), None)
}

fn diagnostics_from_lang_error(
    err: &wf_lang::LangError,
    severity: &str,
    fallback_file: String,
    hint: Option<&str>,
) -> Vec<DebugWfusionRuleEditorDiagnostic> {
    let detail = err.detail().clone().unwrap_or_else(|| err.to_string());
    parse_structured_diagnostics(&detail, severity, fallback_file, hint)
}

fn parse_structured_diagnostics(
    detail: &str,
    default_severity: &str,
    fallback_file: String,
    hint: Option<&str>,
) -> Vec<DebugWfusionRuleEditorDiagnostic> {
    let text = detail.trim();
    if text.is_empty() {
        return vec![build_message_diagnostic(
            default_severity,
            fallback_file,
            None,
            "未知错误".to_string(),
            hint,
        )];
    }

    let normalized = text.strip_prefix("semantic errors:\n").unwrap_or(text);
    let chunks = split_diagnostic_chunks(normalized);
    let diagnostics = chunks
        .into_iter()
        .map(|chunk| parse_diagnostic_chunk(&chunk, default_severity, &fallback_file, hint))
        .collect::<Vec<_>>();

    if diagnostics.is_empty() {
        vec![build_message_diagnostic(
            default_severity,
            fallback_file,
            None,
            normalized.to_string(),
            hint,
        )]
    } else {
        diagnostics
    }
}

fn split_diagnostic_chunks(detail: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();

    for line in detail.lines() {
        if line.starts_with("file: ") && !current.is_empty() {
            chunks.push(current.join("\n").trim().to_string());
            current.clear();
        }
        current.push(line.to_string());
    }

    if !current.is_empty() {
        chunks.push(current.join("\n").trim().to_string());
    }

    if chunks.is_empty() {
        vec![detail.trim().to_string()]
    } else {
        chunks
    }
}

fn parse_diagnostic_chunk(
    chunk: &str,
    default_severity: &str,
    fallback_file: &str,
    hint: Option<&str>,
) -> DebugWfusionRuleEditorDiagnostic {
    let mut file = fallback_file.to_string();
    let mut category = None;
    let mut message_lines = Vec::new();
    let mut rule = None;
    let mut test = None;
    let mut line = None;
    let mut column = None;
    let mut snippet_lines = Vec::new();
    let mut collecting_snippet = false;
    let mut severity = default_severity.to_string();

    for raw_line in chunk.lines() {
        let trimmed = raw_line.trim_end();

        if let Some(value) = trimmed.strip_prefix("file: ") {
            file = value.trim().to_string();
            collecting_snippet = false;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("category: ") {
            category = Some(value.trim().to_string());
            collecting_snippet = false;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("rule: ") {
            rule = Some(value.trim().to_string());
            collecting_snippet = false;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("test: ") {
            test = Some(value.trim().to_string());
            collecting_snippet = false;
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("location: ") {
            let (parsed_line, parsed_column) = parse_location(value);
            line = parsed_line;
            column = parsed_column;
            collecting_snippet = false;
            continue;
        }

        if trimmed.starts_with("warning: ") {
            severity = "warning".to_string();
            message_lines.push(trimmed.to_string());
            collecting_snippet = false;
            continue;
        }
        if trimmed.starts_with("error: ") || trimmed.starts_with("parse error at ") {
            message_lines.push(trimmed.to_string());
            collecting_snippet = false;
            if trimmed.starts_with("parse error at ") {
                let (parsed_line, parsed_column) = parse_parse_error_location(trimmed);
                if line.is_none() {
                    line = parsed_line;
                }
                if column.is_none() {
                    column = parsed_column;
                }
            }
            continue;
        }

        if looks_like_snippet_line(trimmed) || collecting_snippet {
            collecting_snippet = true;
            snippet_lines.push(trimmed.to_string());
            continue;
        }

        if !trimmed.is_empty() {
            message_lines.push(trimmed.to_string());
        }
    }

    let message = message_lines.join("\n").trim().to_string();
    let snippet = if snippet_lines.is_empty() {
        None
    } else {
        Some(snippet_lines.join("\n"))
    };

    DebugWfusionRuleEditorDiagnostic {
        severity,
        file,
        category,
        message: if message.is_empty() {
            chunk.trim().to_string()
        } else {
            message
        },
        rule,
        test,
        line,
        column,
        snippet,
        hint: hint.map(|value| value.to_string()),
    }
}

fn parse_location(raw: &str) -> (Option<usize>, Option<usize>) {
    let value = raw.trim();
    let Some(line_part) = value.strip_prefix("line ") else {
        return (None, None);
    };
    let Some((line_text, column_part)) = line_part.split_once(", column ") else {
        return (line_part.trim().parse::<usize>().ok(), None);
    };
    (
        line_text.trim().parse::<usize>().ok(),
        column_part.trim().parse::<usize>().ok(),
    )
}

fn parse_parse_error_location(raw: &str) -> (Option<usize>, Option<usize>) {
    let Some(line_part) = raw.strip_prefix("parse error at line ") else {
        return (None, None);
    };
    let Some((line_text, column_part)) = line_part.split_once(", column ") else {
        return (line_part.trim().parse::<usize>().ok(), None);
    };
    (
        line_text.trim().parse::<usize>().ok(),
        column_part.trim().parse::<usize>().ok(),
    )
}

fn looks_like_snippet_line(raw: &str) -> bool {
    raw.contains('|')
        && (raw.trim_start().starts_with('|')
            || raw
                .trim_start()
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit()))
}
