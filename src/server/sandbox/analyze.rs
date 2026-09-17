//! 沙盒运行结果分析逻辑。

use std::path::Path;

use regex::Regex;

use crate::utils::SystemKind;
use crate::utils::sandbox::collect_output_checks;

use super::{Conclusion, OutputFileStatus};

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
    pub output_count: usize,
    pub passed: bool,
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

/// 分析生成器输出是否成功，并返回样本数量。
pub fn analyse_generator_result(
    system: SystemKind,
    log_path: &Path,
) -> Result<(usize, String), StageError> {
    if contains_keyword(log_path, &["error", "panic"])? {
        let snippet = read_tail_snippet(log_path, 400)
            .map(|s| format!(": {}", s))
            .unwrap_or_default();
        return Err(StageError::with_code(
            format!("{} 日志包含错误信息{}", generator_name(system), snippet),
            "GENERATOR_STDERR",
        ));
    }

    let content =
        std::fs::read_to_string(log_path).map_err(|err| StageError::new(err.to_string()))?;
    let generated_re = match system {
        SystemKind::Wparse => Regex::new(r"generated\s*=\s*(\d+)").unwrap(),
        // wfgen 使用 --send 时输出 "Sent X events"，使用 --out 时输出 "Generated X events"，
        // 此处同时匹配两种格式，并对多场景的条数求和。
        SystemKind::Wfusion => Regex::new(r"(?i)(?:generated|sent)\s+(\d+)\s+events").unwrap(),
    };
    if !generated_re.is_match(&content) {
        return Err(StageError::with_code(
            format!("未能解析 {} 输出中的生成条数", generator_name(system)),
            "GENERATOR_PARSE_FAILED",
        ));
    }
    let count: usize = generated_re
        .captures_iter(&content)
        .filter_map(|caps| caps.get(1))
        .filter_map(|m| m.as_str().parse::<usize>().ok())
        .sum();
    if count == 0 {
        return Err(StageError::with_code(
            format!("{} 未生成任何样本", generator_name(system)),
            "GENERATOR_ZERO_SAMPLE",
        ));
    }
    Ok((
        count,
        format!("{} 生成 {} 条样本", generator_name(system), count),
    ))
}

/// 汇总 daemon 输出结果，包括输出文件与日志指标。
pub fn analyse_runtime_output(
    system: SystemKind,
    project_dir: &Path,
    daemon_stdout: &Path,
    expected_success: usize,
) -> Result<RuntimeAnalysis, StageError> {
    let output_checks = match system {
        SystemKind::Wparse => {
            collect_output_checks(project_dir).map_err(|err| StageError::new(err.to_string()))?
        }
        SystemKind::Wfusion => collect_wfusion_output_checks(project_dir)?,
    };
    let success_output_path = match system {
        SystemKind::Wparse => project_dir.join("data/out_dat/all.json"),
        SystemKind::Wfusion => project_dir.join("data/out_dat/alert.json"),
    };
    let monitor_output_path = project_dir.join("data/out_dat/metrics.ndjson");
    let output_count = count_file_lines(&success_output_path).map_err(|err| {
        StageError::new(format!(
            "统计 {} 输出条数失败: {}",
            success_output_path.display(),
            err
        ))
    })?;
    let monitor_count =
        count_file_lines(&monitor_output_path).map_err(|err| StageError::new(err.to_string()))?;

    let stdout = std::fs::read_to_string(daemon_stdout).unwrap_or_default();
    let lower_stdout = stdout.to_lowercase();

    let mut metrics = RuntimeMetrics {
        miss_count: lower_stdout.matches("rule miss").count(),
        error_count: lower_stdout.matches(" error ").count(),
        output_count,
        ..Default::default()
    };

    let mut log_lines = Vec::new();
    log_lines.push("运行输出文件检查：".to_string());
    for check in &output_checks {
        let observation_suffix = if check.affects_pass {
            ""
        } else {
            "，仅观察，不参与通过判定"
        };
        log_lines.push(format!("$ cat {} | wc -l", check.relative_path));
        if check.line_count == 0 {
            log_lines.push(format!(
                "结果: 0 行（{}{}）",
                check.meaning, observation_suffix
            ));
        } else {
            log_lines.push(format!(
                "结果: {} 行（{}{}）",
                check.line_count, check.meaning, observation_suffix
            ));
            if check.affects_pass {
                log_lines.push(format!(
                    "[DIAG] {} 非空: {}",
                    check.relative_path, check.meaning
                ));
            }
        }
        log_lines.push(String::new());
    }
    log_lines.push(format!(
        "$ cat {} | wc -l",
        success_output_path
            .strip_prefix(project_dir)
            .unwrap_or(&success_output_path)
            .display()
    ));
    log_lines.push(format!("结果: {} 行（成功输出数据）", metrics.output_count));
    log_lines.push(String::new());
    if matches!(system, SystemKind::Wfusion) {
        log_lines.push("$ cat data/out_dat/metrics.ndjson | wc -l".to_string());
        log_lines.push(format!("结果: {} 行（监控输出数据）", monitor_count));
        log_lines.push(String::new());
    }

    log_lines.push(format!("{} 日志指标：", daemon_name(system)));
    log_lines.push(format!("模拟发送数量 {} 条", expected_success));
    log_lines.push(format!("成功输出数量 {} 条", metrics.output_count));
    log_lines.push(format!("rule miss 次数: {}", metrics.miss_count));
    if metrics.miss_count > 0 {
        log_lines.push(format!("[DIAG] rule miss 计数 {} 次", metrics.miss_count));
    }
    let passed = match system {
        SystemKind::Wparse => {
            output_checks
                .iter()
                .filter(|item| item.affects_pass)
                .all(|item| item.is_empty)
                && metrics.output_count >= expected_success
        }
        SystemKind::Wfusion => {
            output_checks
                .iter()
                .filter(|item| item.affects_pass)
                .all(|item| item.is_empty)
                && metrics.output_count > 0
        }
    };
    metrics.passed = passed;

    Ok(RuntimeAnalysis {
        output_checks,
        metrics,
        log_text: log_lines.join("\n"),
        passed,
    })
}

/// 检查日志文件中是否包含任一关键字。
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

/// 读取日志尾部片段，用于错误摘要展示。
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

/// 统计单个文件的行数。
fn count_file_lines(path: &Path) -> Result<usize, std::io::Error> {
    if !path.exists() {
        return Ok(0);
    }
    let content = std::fs::read_to_string(path)?;
    Ok(content.lines().count())
}

/// 依据运行指标得出最终结论。
pub fn finalize_conclusion(
    output_checks: &[OutputFileStatus],
    metrics: &RuntimeMetrics,
) -> Conclusion {
    let mut conclusion = Conclusion {
        output_file_checks: output_checks.to_vec(),
        suspected_files: output_checks
            .iter()
            .filter(|item| item.affects_pass && !item.is_empty)
            .map(|item| item.relative_path.clone())
            .collect(),
        input_count: metrics.input_count,
        runtime_miss_count: metrics.miss_count,
        runtime_error_count: metrics.error_count,
        runtime_output_count: metrics.output_count,
        daemon_ready: Some(metrics.daemon_ready),
        wpgen_exit_code: metrics.wpgen_exit_code,
        wpgen_generated_count: metrics.wpgen_generated,
        ..Default::default()
    };
    conclusion.passed = metrics.passed;

    conclusion
}

fn collect_wfusion_output_checks(project_dir: &Path) -> Result<Vec<OutputFileStatus>, StageError> {
    let specs = [
        ("data/out_dat/default.ndjson", "事件落入默认输出"),
        ("data/out_dat/error.ndjson", "运行处理出现错误"),
    ];
    let mut results = Vec::new();
    for (relative, meaning) in specs {
        let path = project_dir.join(relative);
        let line_count = count_file_lines(&path).map_err(|err| StageError::new(err.to_string()))?;
        results.push(OutputFileStatus {
            relative_path: relative.to_string(),
            is_empty: line_count == 0,
            line_count,
            meaning: meaning.to_string(),
            affects_pass: true,
        });
    }
    Ok(results)
}

fn daemon_name(system: SystemKind) -> &'static str {
    match system {
        SystemKind::Wparse => "wparse",
        SystemKind::Wfusion => "wfusion",
    }
}

fn generator_name(system: SystemKind) -> &'static str {
    match system {
        SystemKind::Wparse => "wpgen",
        SystemKind::Wfusion => "wfgen",
    }
}
