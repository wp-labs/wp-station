use std::path::PathBuf;

use chrono::Utc;
use serde::Serialize;

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::ProjectLayout;
use crate::server::sync::sync_to_gitea_all;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::knowledge::reload_knowledge;
use crate::utils::project_check::check_component;
use crate::utils::{ProjectSnapshot, load_project_snapshot_from_layout};

#[derive(Serialize)]
pub struct ProjectImportResponse {
    pub summary: ProjectImportSummary,
    pub validation: ProjectImportValidation,
}

#[derive(Serialize)]
pub struct ProjectImportSummary {
    pub rules_deleted: usize,
    pub rules_imported: usize,
    pub knowledge_deleted: usize,
    pub knowledge_imported: usize,
    pub rule_breakdown: Vec<ProjectImportBreakdown>,
    pub warnings: Vec<String>,
    pub failed_files: usize,
    pub project_root: String,
}

#[derive(Serialize)]
pub struct ProjectImportBreakdown {
    pub rule_type: String,
    pub count: usize,
}

#[derive(Serialize)]
pub struct ProjectImportValidation {
    pub passed: bool,
    pub message: String,
}

pub async fn import_project_from_files_logic(
    operator: Option<String>,
) -> Result<ProjectImportResponse, AppError> {
    let operator_for_log = operator.clone();
    let setting = Setting::load();
    let layout = setting.project_layout();
    let project_path = resolve_project_path(&layout);

    let snapshot = load_project_snapshot_from_layout(&layout)?;
    if snapshot.rules.is_empty() && snapshot.knowledge.is_empty() {
        return Err(AppError::validation(
            "项目目录中未找到可导入的规则或知识库".to_string(),
        ));
    }

    // 文件已经是主数据源；这里先执行组件校验，避免坏配置进入后续同步/发布链路。
    check_component(RuleType::All.to_check_component())?;

    let result = async {
        let ProjectSnapshot {
            rules,
            knowledge,
            rule_stats,
            warnings,
            failed_files,
        } = snapshot;

        let total_rules = rules.len();
        let total_knowledge = knowledge.len();

        if let Err(err) = reload_knowledge(&layout) {
            warn!("知识库重载失败（忽略）: {}", err);
        }

        let commit_message = format!("初始化更新配置 {}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
        sync_to_gitea_all(&commit_message).await;

        let mut breakdown: Vec<ProjectImportBreakdown> = rule_stats
            .into_iter()
            .map(|(rule_type, count)| ProjectImportBreakdown {
                rule_type: rule_type.as_ref().to_string(),
                count,
            })
            .collect();
        breakdown.sort_by(|a, b| a.rule_type.cmp(&b.rule_type));

        let summary = ProjectImportSummary {
            rules_deleted: 0,
            rules_imported: total_rules,
            knowledge_deleted: 0,
            knowledge_imported: total_knowledge,
            rule_breakdown: breakdown,
            warnings,
            failed_files,
            project_root: project_path.to_string_lossy().to_string(),
        };

        let validation = ProjectImportValidation {
            passed: true,
            message: "项目组件校验通过".to_string(),
        };

        Ok::<_, AppError>(ProjectImportResponse {
            summary,
            validation,
        })
    }
    .await;

    let mut log_params = OperationLogParams::new();
    if let Some(op) = operator_for_log {
        log_params = log_params.with_operator(op);
    }
    if let Ok(ref resp) = result {
        log_params = log_params
            .with_field("rules_deleted", resp.summary.rules_deleted.to_string())
            .with_field("rules_imported", resp.summary.rules_imported.to_string())
            .with_field(
                "knowledge_imported",
                resp.summary.knowledge_imported.to_string(),
            )
            .with_field("project_root", resp.summary.project_root.clone());
    }

    write_operation_log_for_result(
        OperationLogBiz::RuleFile,
        OperationLogAction::Update,
        log_params,
        &result,
    )
    .await;

    result
}

fn resolve_project_path(layout: &ProjectLayout) -> PathBuf {
    let base = Setting::workspace_root()
        .join("tmp")
        .join("project-import-view");
    let summary = format!(
        "{} + {}",
        layout.models_root.display(),
        layout.infra_root.display()
    );
    PathBuf::from(format!("{} ({})", base.display(), summary))
}
