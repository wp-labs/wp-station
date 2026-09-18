//! 项目导入摘要与预检辅助。
//!
//! 这一层负责在不改动真实仓库的前提下：
//! - 识别导入范围
//! - 构造预检目录
//! - 生成导入结果摘要

use std::collections::HashMap;
use std::path::Path;

use tempfile::tempdir;

use crate::constants::project::IMPORTABLE_ROOT_DIRS;
use crate::error::AppError;
use crate::server::RepoLayout;
use crate::utils::project_check::{ProjectCheckTarget, validate_project_in_dir};
use crate::utils::{
    ProjectSnapshot, SystemKind, compose_repo_layout_into, layout_for_system,
    load_project_snapshot, load_project_snapshot_from_repo_layout,
};

use super::{
    ImportScope, ProjectImportResponse, ProjectImportSummary, ProjectImportValidation,
    build_rule_breakdown_from_snapshot,
};

/// 为归档预检构造摘要结果，不写入真实项目目录。
pub(super) fn validate_project_import_preview(
    system: SystemKind,
    source_dir: &Path,
) -> Result<ProjectImportSummary, AppError> {
    let scope = super::detect_import_scope(source_dir)?;
    let layout = layout_for_system(system).as_repo_layout();
    let _preview_dir = build_preview_project_dir(system, source_dir, &layout, &scope)?;
    build_import_summary_from_source_dir(
        source_dir,
        source_dir.to_string_lossy().as_ref(),
        &layout,
        &scope,
        None,
    )
}

/// 构造预检用的临时项目目录，并在其中执行组件校验。
pub(super) fn build_preview_project_dir(
    system: SystemKind,
    source_dir: &Path,
    layout: &RepoLayout,
    scope: &ImportScope,
) -> Result<tempfile::TempDir, AppError> {
    let preview_dir = tempdir().map_err(AppError::internal)?;
    compose_repo_layout_into(layout, preview_dir.path())?;
    super::apply_import_scope_to_dir(source_dir, preview_dir.path(), scope)?;
    validate_project_in_dir(system, preview_dir.path(), ProjectCheckTarget::WholeProject)?;
    Ok(preview_dir)
}

/// 基于导入源目录构造最终导入响应。
pub(super) fn build_import_response_from_source_dir(
    source_dir: &Path,
    layout: &RepoLayout,
    validation_message: &str,
    source_label: &str,
    scope: &ImportScope,
    previous_snapshot: Option<&ProjectSnapshot>,
) -> Result<ProjectImportResponse, AppError> {
    let summary = build_import_summary_from_source_dir(
        source_dir,
        source_label,
        layout,
        scope,
        previous_snapshot,
    )?;
    let validation = ProjectImportValidation {
        passed: true,
        message: scope.summary_message(validation_message),
    };

    Ok(ProjectImportResponse {
        summary,
        validation,
    })
}

/// 从导入源目录构造导入摘要。
fn build_import_summary_from_source_dir(
    source_dir: &Path,
    source_label: &str,
    layout: &RepoLayout,
    scope: &ImportScope,
    previous_snapshot: Option<&ProjectSnapshot>,
) -> Result<ProjectImportSummary, AppError> {
    let snapshot = load_snapshot_for_import_scope(source_dir, scope)?;
    let current_snapshot = if let Some(snapshot) = previous_snapshot {
        filter_snapshot_by_import_scope(snapshot.clone(), scope)
    } else {
        filter_snapshot_by_import_scope(load_project_snapshot_from_repo_layout(layout)?, scope)
    };
    let (rules_deleted, knowledge_deleted) = build_deleted_counts(&snapshot, &current_snapshot);
    build_import_summary_from_snapshot(
        snapshot,
        source_label,
        layout.models_root.to_string_lossy().to_string(),
        layout.infra_root.to_string_lossy().to_string(),
        scope,
        rules_deleted,
        knowledge_deleted,
    )
}

/// 从项目快照构造统一导入摘要。
fn build_import_summary_from_snapshot(
    snapshot: ProjectSnapshot,
    source_label: &str,
    models_root: String,
    infra_root: String,
    scope: &ImportScope,
    rules_deleted: usize,
    knowledge_deleted: usize,
) -> Result<ProjectImportSummary, AppError> {
    if snapshot.rules.is_empty() && snapshot.knowledge.is_empty() {
        return Err(AppError::validation(
            "导入后的项目目录中未找到可导入的规则或知识库".to_string(),
        ));
    }

    let ProjectSnapshot {
        rules,
        knowledge,
        mut warnings,
        failed_files,
        ..
    } = snapshot;

    if !scope.retained_dir_names().is_empty() {
        warnings.push(format!(
            "本次归档仅覆盖目录: {}；保留当前目录: {}",
            scope.imported_dirs.join(", "),
            scope.retained_dir_names().join(", ")
        ));
    }

    let snapshot_for_breakdown = ProjectSnapshot {
        rules: rules.clone(),
        knowledge: knowledge.clone(),
        rule_stats: build_rule_stats(&rules),
        warnings: Vec::new(),
        failed_files: 0,
    };
    let breakdown = build_rule_breakdown_from_snapshot(&snapshot_for_breakdown);

    Ok(ProjectImportSummary {
        rules_deleted,
        rules_imported: rules.len(),
        knowledge_deleted,
        knowledge_imported: knowledge.len(),
        imported_dirs: scope.imported_dir_names(),
        retained_dirs: scope.retained_dir_names(),
        rule_breakdown: breakdown,
        warnings,
        failed_files,
        source_dir: source_label.to_string(),
        models_root,
        infra_root,
    })
}

fn load_snapshot_for_import_scope(
    source_dir: &Path,
    scope: &ImportScope,
) -> Result<ProjectSnapshot, AppError> {
    let snapshot = load_project_snapshot(source_dir)?;
    Ok(filter_snapshot_by_import_scope(snapshot, scope))
}

fn build_deleted_counts(
    source_snapshot: &ProjectSnapshot,
    current_snapshot: &ProjectSnapshot,
) -> (usize, usize) {
    let source_rules: std::collections::HashSet<_> = source_snapshot
        .rules
        .iter()
        .map(|rule| {
            (
                rule.rule_type,
                normalized_import_identity(rule.rule_type, &rule.file_name),
            )
        })
        .collect();
    let current_rules: std::collections::HashSet<_> = current_snapshot
        .rules
        .iter()
        .map(|rule| {
            (
                rule.rule_type,
                normalized_import_identity(rule.rule_type, &rule.file_name),
            )
        })
        .collect();
    let rules_deleted = current_rules.difference(&source_rules).count();

    let source_knowledge: std::collections::HashSet<_> = source_snapshot
        .knowledge
        .iter()
        .map(|item| item.file_name.clone())
        .collect();
    let current_knowledge: std::collections::HashSet<_> = current_snapshot
        .knowledge
        .iter()
        .map(|item| item.file_name.clone())
        .collect();
    let knowledge_deleted = current_knowledge.difference(&source_knowledge).count();

    (rules_deleted, knowledge_deleted)
}

fn normalized_import_identity(rule_type: crate::db::RuleType, file_name: &str) -> String {
    let normalized = file_name.trim().trim_matches('/').replace('\\', "/");
    if normalized.is_empty() {
        return normalized;
    }

    match rule_type {
        crate::db::RuleType::Schema
        | crate::db::RuleType::Rule
        | crate::db::RuleType::Scenarios => {
            let parts: Vec<&str> = normalized
                .split('/')
                .filter(|part| !part.is_empty())
                .collect();
            if parts.len() == 2 {
                let folder = parts[0];
                let file = parts[1];
                let stem = file.rsplit_once('.').map(|(name, _)| name).unwrap_or(file);
                if stem == folder {
                    return file.to_string();
                }
            }
            normalized
        }
        _ => normalized,
    }
}

fn filter_snapshot_by_import_scope(
    snapshot: ProjectSnapshot,
    scope: &ImportScope,
) -> ProjectSnapshot {
    let imported_dirs: Vec<&str> = scope.imported_dirs.to_vec();
    let ProjectSnapshot {
        rules,
        knowledge,
        warnings,
        failed_files,
        ..
    } = snapshot;

    let filtered_rules: Vec<_> = rules
        .into_iter()
        .filter(|rule| {
            imported_dirs.iter().any(|root_dir| match *root_dir {
                "models" => matches!(
                    rule.rule_type,
                    crate::db::RuleType::Wpl
                        | crate::db::RuleType::Oml
                        | crate::db::RuleType::Windows
                        | crate::db::RuleType::Schema
                        | crate::db::RuleType::Rule
                        | crate::db::RuleType::Scenarios
                ),
                "conf" => matches!(
                    rule.rule_type,
                    crate::db::RuleType::Parse | crate::db::RuleType::Wpgen
                ),
                "connectors" => matches!(
                    rule.rule_type,
                    crate::db::RuleType::SourceConnect | crate::db::RuleType::SinkConnect
                ),
                "topology" => matches!(
                    rule.rule_type,
                    crate::db::RuleType::Source | crate::db::RuleType::Sink
                ),
                _ => false,
            })
        })
        .collect();

    let filtered_knowledge = if imported_dirs.contains(&"models") {
        knowledge
    } else {
        Vec::new()
    };

    ProjectSnapshot {
        rule_stats: build_rule_stats(&filtered_rules),
        rules: filtered_rules,
        knowledge: filtered_knowledge,
        warnings,
        failed_files,
    }
}

fn build_rule_stats(
    rules: &[crate::utils::project_fs::ProjectRuleFile],
) -> HashMap<crate::db::RuleType, usize> {
    let mut stats = HashMap::new();
    for rule in rules {
        *stats.entry(rule.rule_type).or_insert(0) += 1;
    }
    stats
}

/// 在不覆盖真实项目目录的前提下校验导入范围是否有效。
pub(super) fn validate_import_scope_with_repo_layout(
    system: SystemKind,
    source_dir: &Path,
    layout: &RepoLayout,
    scope: &ImportScope,
) -> Result<(), AppError> {
    let _ = build_preview_project_dir(system, source_dir, layout, scope)?;
    Ok(())
}

/// 识别当前导入包实际包含哪些顶层目录。
pub(super) fn detect_import_scope(source_dir: &Path) -> Result<ImportScope, AppError> {
    let imported_dirs: Vec<&'static str> = IMPORTABLE_ROOT_DIRS
        .iter()
        .copied()
        .filter(|name| source_dir.join(name).is_dir())
        .collect();

    if imported_dirs.is_empty() {
        return Err(AppError::validation(
            "导入包中未找到可导入目录，至少需要包含 conf、connectors、topology、models 中的一个"
                .to_string(),
        ));
    }

    Ok(ImportScope { imported_dirs })
}
