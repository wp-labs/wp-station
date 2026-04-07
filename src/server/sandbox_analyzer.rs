use std::path::Path;

use regex::Regex;

use crate::utils::sandbox_workspace::collect_output_checks;

use super::sandbox::{Conclusion, OutputFileStatus};

/// 沙盒阶段执行中的错误详情。
#[derive(Debug)]
pub struct StageError {
    pub summary: String,
    pub code: Option<String>,
}

impl StageError {
    /// 使用摘要构造错误。
    pub fn new(summary: impl Into<String>) -> Self {
        StageError {
            summary: summary.into(),
            code: None,
        }
    }

    /// 构造同时包含错误码的错误。
    pub fn with_code(summary: impl Into<String>, code: impl Into<String>) -> Self {
        StageError {
            summary: summary.into(),
            code: Some(code.into()),
        }
    }
}

/// 运行期的统计指标，供结论计算使用。
#[derive(Default, Debug, Clone)]
pub struct RuntimeMetrics {
    pub input_count: usize,
    pub miss_count: usize,
    pub error_count: usize,
    pub success_count: usize,
    pub daemon_ready: bool,
    pub wpgen_exit_code: Option<i32>,
    pub wpgen_generated: Option<usize>,
}

/// 运行输出分析结果，包含诊断日志与指标。
#[derive(Debug, Clone)]
pub struct RuntimeAnalysis {
    pub output_checks: Vec<OutputFileStatus>,
    pub metrics: RuntimeMetrics,
    pub log_text: String,
    pub passed: bool,
}

/// 分析 wpgen 输出是否成功，并返回样本数量。
pub fn analyse_wpgen_result(log_path: &Path) -> Result<(usize, String), StageError> {
    if contains_keyword(log_path, &["error", "panic"])? {
        let snippet = read_tail_snippet(log_path, 400)
            .map(|s| format!(": {}", s))
            .unwrap_or_default();
        return Err(StageError::with_code(
            format!("wpgen 日志包含错误信息{}", snippet),
            "WPGEN_STDERR",
        ));
    }

    let content =
        std::fs::read_to_string(log_path).map_err(|err| StageError::new(err.to_string()))?;
    let generated_re = Regex::new(r"generated\s*=\s*(\d+)").unwrap();
    if let Some(caps) = generated_re.captures(&content) {
        let count: usize = caps
            .get(1)
            .and_then(|m| m.as_str().parse().ok())
            .unwrap_or(0);
        if count == 0 {
            return Err(StageError::with_code(
                "wpgen 未生成任何样本",
                "WPGEN_ZERO_SAMPLE",
            ));
        }
        Ok((count, format!("wpgen 生成 {} 条样本", count)))
    } else {
        Err(StageError::with_code(
            "未能解析 wpgen 输出中的 generated= 值",
            "WPGEN_PARSE_FAILED",
        ))
    }
}

/// 汇总 daemon 输出结果，包括输出文件与日志指标。
pub fn analyse_runtime_output(
    project_dir: &Path,
    daemon_stdout: &Path,
    expected_success: usize,
) -> Result<RuntimeAnalysis, StageError> {
    let output_checks =
        collect_output_checks(project_dir).map_err(|err| StageError::new(err.to_string()))?;

    let stdout = std::fs::read_to_string(daemon_stdout).unwrap_or_default();
    let lower_stdout = stdout.to_lowercase();

    let metrics = RuntimeMetrics {
        miss_count: lower_stdout.matches("rule miss").count(),
        error_count: lower_stdout.matches(" error ").count(),
        success_count: lower_stdout.matches("success").count(),
        ..Default::default()
    };

    let mut log_lines = Vec::new();
    log_lines.push("运行输出文件检查：".to_string());
    for check in &output_checks {
        log_lines.push(format!("$ cat {} | wc -l", check.relative_path));
        if check.line_count == 0 {
            log_lines.push(format!("结果: 0 行（{}）", check.meaning));
        } else {
            log_lines.push(format!(
                "结果: {} 行（{}）",
                check.line_count, check.meaning
            ));
            log_lines.push(format!(
                "[DIAG] {} 非空: {}",
                check.relative_path, check.meaning
            ));
        }
        log_lines.push(String::new());
    }

    log_lines.push("wparse 日志指标：".to_string());
    log_lines.push(format!("期望成功解析条数: {}", expected_success));
    log_lines.push(format!("success 日志次数: {}", metrics.success_count));
    log_lines.push(format!("rule miss 次数: {}", metrics.miss_count));
    log_lines.push(format!("ERROR 次数: {}", metrics.error_count));
    if metrics.miss_count > 0 {
        log_lines.push(format!("[DIAG] rule miss 计数 {} 次", metrics.miss_count));
    }
    if metrics.error_count > 0 {
        log_lines.push(format!(
            "[DIAG] wparse ERROR 日志 {} 条",
            metrics.error_count
        ));
    }
    let passed = output_checks.iter().all(|item| item.is_empty) && metrics.error_count == 0;

    Ok(RuntimeAnalysis {
        output_checks,
        metrics,
        log_text: log_lines.join("\n"),
        passed,
    })
}

fn contains_keyword(path: &Path, needles: &[&str]) -> Result<bool, StageError> {
    if !path.exists() {
        return Ok(false);
    }
    let content = std::fs::read_to_string(path).map_err(|err| StageError::new(err.to_string()))?;
    let lower = content.to_lowercase();
    Ok(needles
        .iter()
        .any(|keyword| lower.contains(&keyword.to_lowercase())))
}

fn read_tail_snippet(path: &Path, max_chars: usize) -> Option<String> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(10);
    let mut snippet = lines[start..].join("\n");
    if snippet.len() > max_chars {
        snippet.truncate(max_chars);
        snippet.push_str("...");
    }
    Some(snippet)
}

/// 依据运行指标得出最终结论。
pub fn finalize_conclusion(
    output_checks: &[OutputFileStatus],
    metrics: &RuntimeMetrics,
) -> Conclusion {
    let mut conclusion = Conclusion::default();
    conclusion.output_file_checks = output_checks.to_vec();
    conclusion.suspected_files = output_checks
        .iter()
        .filter(|item| !item.is_empty)
        .map(|item| item.relative_path.clone())
        .collect();
    conclusion.input_count = metrics.input_count;
    conclusion.runtime_miss_count = metrics.miss_count;
    conclusion.runtime_error_count = metrics.error_count;
    conclusion.daemon_ready = Some(metrics.daemon_ready);
    conclusion.wpgen_exit_code = metrics.wpgen_exit_code;
    conclusion.wpgen_generated_count = metrics.wpgen_generated;
    conclusion.passed =
        output_checks.iter().all(|item| item.is_empty) && conclusion.runtime_error_count == 0;

    conclusion
}
