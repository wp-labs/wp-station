use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::server::Setting;
use crate::server::sandbox::{DiagnosticHit, SandboxStage};

#[derive(Clone, Copy)]
enum StageMatcher {
    #[allow(dead_code)]
    Any,
    Exact(SandboxStage),
}

#[derive(Clone, Copy)]
enum LogSource {
    StageLog,
    #[allow(dead_code)]
    Workspace(&'static str),
}

struct DiagnosticRule {
    stage: StageMatcher,
    keyword: &'static str,
    suggestion: &'static str,
    log: LogSource,
    priority: i32,
}

/// 根据阶段与日志内容生成命中提示，最多返回 5 条。
pub fn collect_stage_hits(
    stage: SandboxStage,
    stage_log_path: Option<&str>,
    workspace_path: Option<&Path>,
) -> Vec<DiagnosticHit> {
    let mut hits = Vec::new();
    let stage_log = stage_log_path.and_then(read_text);
    let mut workspace_cache: HashMap<&'static str, Option<String>> = HashMap::new();

    for rule in DIAGNOSTIC_RULES {
        if !rule.stage.matches(stage) {
            continue;
        }
        let content = match rule.log {
            LogSource::StageLog => stage_log.as_deref(),
            LogSource::Workspace(relative) => {
                let entry = workspace_cache.entry(relative).or_insert_with(|| {
                    workspace_path
                        .map(|root| root.join(relative))
                        .and_then(|full| read_text(full.to_string_lossy().as_ref()))
                });
                entry.as_deref()
            }
        };
        if let Some(text) = content
            && text.contains(rule.keyword)
        {
            hits.push(DiagnosticHit {
                keyword: rule.keyword.to_string(),
                suggestion: rule.suggestion.to_string(),
                priority: rule.priority,
            });
        }
    }

    hits.sort_by_key(|hit| hit.priority);
    hits.truncate(5);
    hits
}

fn read_text(path: &str) -> Option<String> {
    let resolved = resolve_stage_log_path(path);
    std::fs::read_to_string(resolved).ok()
}

impl StageMatcher {
    fn matches(self, stage: SandboxStage) -> bool {
        match self {
            StageMatcher::Any => true,
            StageMatcher::Exact(expected) => expected == stage,
        }
    }
}

const DIAGNOSTIC_RULES: &[DiagnosticRule] = &[
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::StartDaemon),
        keyword: "configuration error",
        suggestion: "检查 conf/wparse.toml 的配置格式，exit code 300 表示配置解析失败",
        log: LogSource::StageLog,
        priority: 5,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::StartDaemon),
        keyword: "address already in use",
        suggestion: "端口 19090 被占用，检查是否有残留 wparse 进程（ps aux | grep wparse）",
        log: LogSource::StageLog,
        priority: 5,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::StartDaemon),
        keyword: "bind failed",
        suggestion: "端口绑定失败，确认 conf/wparse.toml 中 admin_api.bind 配置是否正确",
        log: LogSource::StageLog,
        priority: 10,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::StartDaemon),
        keyword: "panic",
        suggestion: "wparse daemon 出现 panic，请在 daemon.stderr.log 中查看堆栈",
        log: LogSource::StageLog,
        priority: 15,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::RunWpgen),
        keyword: "wpgen.toml",
        suggestion: "wpgen 配置解析失败，请检查 conf/wpgen.toml 的内容格式",
        log: LogSource::StageLog,
        priority: 20,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] data/out_dat/miss.dat",
        suggestion: "miss.dat 非空：有样本未命中规则，请校验 models/wpl/*/parse.wpl 的匹配条件",
        log: LogSource::StageLog,
        priority: 30,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] data/out_dat/residue.dat",
        suggestion: "residue.dat 非空：存在残余数据，检查 models/oml/*/adm.oml 是否完整",
        log: LogSource::StageLog,
        priority: 30,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] data/out_dat/default.dat",
        suggestion: "default.dat 非空：数据命中兜底路由，检查路由条件是否精准",
        log: LogSource::StageLog,
        priority: 35,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] data/out_dat/error.dat",
        suggestion: "error.dat 非空：处理过程中出现错误，请检查 OML/知识库逻辑",
        log: LogSource::StageLog,
        priority: 25,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] rule miss 计数",
        suggestion: "wparse 日志存在 rule miss，请检查 WPL 匹配条件是否覆盖样本。",
        log: LogSource::StageLog,
        priority: 40,
    },
    DiagnosticRule {
        stage: StageMatcher::Exact(SandboxStage::AnalyseRuntimeOutput),
        keyword: "[DIAG] wparse ERROR 日志",
        suggestion: "wparse 日志出现 ERROR，优先排查 parse/oml 执行栈或外部依赖。",
        log: LogSource::StageLog,
        priority: 25,
    },
];

fn resolve_stage_log_path(path: &str) -> PathBuf {
    let path_ref = Path::new(path);
    if path_ref.is_absolute() {
        path_ref.to_path_buf()
    } else {
        Setting::workspace_root().join(path_ref)
    }
}
