//! 项目导入导出业务逻辑层。
//!
//! 负责两类输入：
//! 1. 传统目录导入
//! 2. 归档预检 / 确认导入 / 归档导出
//!
//! 双系统改造后，目录落点统一通过 `system -> layout` 解析。

mod archive;
mod io;
mod summary;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::Utc;
use flate2::Compression;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use tempfile::tempdir;

use self::archive::{
    archive_import_dir, archive_import_staging_root, extract_archive, find_import_project_root,
    new_archive_import_id, sanitize_upload_name,
};
use self::io::{
    apply_import_scope_to_dir, copy_named_entry, normalize_source_dir,
    overwrite_repo_layout_from_legacy_dir, overwrite_repo_layout_from_partial_dir,
    validate_legacy_project_dir,
};
use self::summary::{
    build_import_response_from_source_dir, detect_import_scope,
    validate_import_scope_with_repo_layout, validate_project_import_preview,
};
use crate::constants::project::{
    DIR_CONF, DIR_CONNECTORS, DIR_MODELS, DIR_TOPOLOGY, IMPORTABLE_ROOT_DIRS,
};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::sync::{sync_shared_connectors_to_infra_gitea, sync_to_gitea};
use crate::server::{RepoLayout, refresh_draft_release_logic};
use crate::utils::knowledge::reload_knowledge;
use crate::utils::project_check::{ProjectCheckTarget, validate_project_in_dir};
use crate::utils::{
    ProjectSnapshot, SystemKind, layout_for_system, load_project_snapshot_from_repo_layout,
};

/// 归档或目录导入时解析得到的覆盖范围。
#[derive(Debug, Clone)]
struct ImportScope {
    imported_dirs: Vec<&'static str>,
}

impl ImportScope {
    /// 返回本次导入实际覆盖的目录名列表。
    fn imported_dir_names(&self) -> Vec<String> {
        self.imported_dirs
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    /// 返回本次导入不会覆盖、将继续保留现状的目录名列表。
    fn retained_dir_names(&self) -> Vec<String> {
        IMPORTABLE_ROOT_DIRS
            .iter()
            .filter(|name| !self.imported_dirs.contains(name))
            .map(|name| (*name).to_string())
            .collect()
    }

    /// 判断是否属于全量导入。
    fn is_full_import(&self) -> bool {
        self.imported_dirs.len() == IMPORTABLE_ROOT_DIRS.len()
    }

    /// 生成人类可读的导入范围摘要。
    fn summary_message(&self, prefix: &str) -> String {
        let imported = self.imported_dirs.join(", ");
        let retained = self.retained_dir_names();
        if retained.is_empty() {
            format!("{prefix}；本次覆盖目录: {imported}")
        } else {
            format!(
                "{prefix}；本次覆盖目录: {}；保留当前目录: {}",
                imported,
                retained.join(", ")
            )
        }
    }
}

/// 目录导入请求。
#[derive(Debug, Deserialize)]
pub struct ProjectImportRequest {
    /// 导入目标系统。
    pub system: SystemKind,
    /// 待导入目录。当前目录导入仅支持 `wparse` 的既有目录结构。
    pub source_dir: String,
}

/// 项目导入响应。
#[derive(Serialize)]
pub struct ProjectImportResponse {
    pub summary: ProjectImportSummary,
    pub validation: ProjectImportValidation,
}

/// 项目导入摘要。
#[derive(Serialize)]
pub struct ProjectImportSummary {
    pub rules_deleted: usize,
    pub rules_imported: usize,
    pub knowledge_deleted: usize,
    pub knowledge_imported: usize,
    pub imported_dirs: Vec<String>,
    pub retained_dirs: Vec<String>,
    pub rule_breakdown: Vec<ProjectImportBreakdown>,
    pub warnings: Vec<String>,
    pub failed_files: usize,
    pub source_dir: String,
    /// 导入后实际写入的 models 仓库根目录。
    pub models_root: String,
    /// 导入后实际写入的 infra 仓库根目录。
    pub infra_root: String,
}

/// 各规则类型的导入数量统计。
#[derive(Debug, Serialize)]
pub struct ProjectImportBreakdown {
    pub rule_type: String,
    pub count: usize,
    pub files: Vec<String>,
}

/// 项目导入校验结果。
#[derive(Serialize)]
pub struct ProjectImportValidation {
    pub passed: bool,
    pub message: String,
}

/// 归档确认导入请求。
#[derive(Debug, Deserialize)]
pub struct ProjectArchiveConfirmRequest {
    pub system: SystemKind,
    pub import_id: String,
}

/// 归档预检响应。
#[derive(Serialize)]
pub struct ProjectArchivePreviewResponse {
    pub import_id: String,
    pub file_name: String,
    pub summary: ProjectImportSummary,
    pub validation: ProjectImportValidation,
}

/// 项目归档导出结果。
pub struct ProjectArchiveExport {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

pub(super) fn build_rule_breakdown_from_snapshot(
    snapshot: &ProjectSnapshot,
) -> Vec<ProjectImportBreakdown> {
    let mut files_by_type: HashMap<RuleType, Vec<String>> = HashMap::new();
    for rule in &snapshot.rules {
        files_by_type
            .entry(rule.rule_type)
            .or_default()
            .push(rule.file_name.clone());
    }

    let mut breakdown: Vec<ProjectImportBreakdown> = snapshot
        .rule_breakdown()
        .into_iter()
        .map(|(rule_type, count)| {
            let mut files = files_by_type.remove(&rule_type).unwrap_or_default();
            files.sort();
            files.dedup();
            ProjectImportBreakdown {
                rule_type: rule_type.as_ref().to_string(),
                count,
                files,
            }
        })
        .collect();
    breakdown.sort_by(|a, b| a.rule_type.cmp(&b.rule_type));
    breakdown
}

/// 按目录导入项目。
///
/// 旧目录结构会按当前系统拆分到固定的 models / infra 仓库。
pub async fn import_project_from_files_logic(
    req: ProjectImportRequest,
) -> Result<ProjectImportResponse, AppError> {
    let layout = layout_for_system(req.system).as_repo_layout();
    let source_dir = normalize_source_dir(&req.source_dir)?;

    import_project_dir(req.system, &source_dir, &layout, "目录拆分覆盖并校验通过").await
}

/// 归档预检入口。
pub async fn preview_project_archive_logic(
    system: SystemKind,
    file_name: &str,
    bytes: Vec<u8>,
) -> Result<ProjectArchivePreviewResponse, AppError> {
    // 归档导入先预检，不直接覆盖真实目录，避免错误归档污染仓库。
    let import_id = new_archive_import_id();
    let staging_root = archive_import_staging_root();
    fs::create_dir_all(&staging_root).map_err(AppError::internal)?;
    let import_dir = staging_root.join(&import_id);
    fs::create_dir_all(&import_dir).map_err(AppError::internal)?;
    let archive_path = import_dir.join(sanitize_upload_name(file_name)?);
    fs::write(&archive_path, bytes).map_err(AppError::internal)?;
    fs::write(import_dir.join("file_name.txt"), file_name).map_err(AppError::internal)?;
    let extract_dir = import_dir.join("extract");
    fs::create_dir_all(&extract_dir).map_err(AppError::internal)?;
    extract_archive(file_name, &archive_path, &extract_dir)?;

    let project_dir = find_import_project_root(&extract_dir, system)?;
    let summary = validate_project_import_preview(system, &project_dir)?;

    Ok(ProjectArchivePreviewResponse {
        import_id,
        file_name: file_name.to_string(),
        summary,
        validation: ProjectImportValidation {
            passed: true,
            message: "归档校验通过，请确认导入".to_string(),
        },
    })
}

/// 归档确认导入入口。
pub async fn confirm_project_archive_import_logic(
    system: SystemKind,
    import_id: &str,
) -> Result<ProjectImportResponse, AppError> {
    // 二次确认阶段只消费预检产物，不重复读取原始上传流。
    let import_dir = archive_import_dir(import_id)?;
    let project_dir_file = import_dir.join("project_dir.txt");
    let project_dir = fs::read_to_string(&project_dir_file)
        .map_err(|e| AppError::validation(format!("导入暂存记录不存在或已过期: {}", e)))?;
    let project_dir = PathBuf::from(project_dir.trim());
    if !project_dir.is_dir() {
        return Err(AppError::validation(
            "导入暂存目录不存在或已过期".to_string(),
        ));
    }
    let file_name = fs::read_to_string(import_dir.join("file_name.txt"))
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let layout = layout_for_system(system).as_repo_layout();
    let result = import_project_archive_dir(
        system,
        &project_dir,
        &layout,
        "归档覆盖导入并校验通过",
        file_name.as_deref(),
    )
    .await?;

    if let Err(err) = fs::remove_dir_all(&import_dir) {
        warn!(
            "清理导入暂存目录失败: import_id={}, path={}, error={}",
            import_id,
            import_dir.display(),
            err
        );
    }

    Ok(result)
}

/// 导出当前系统的项目归档。
pub async fn export_project_archive_logic(
    system: SystemKind,
) -> Result<ProjectArchiveExport, AppError> {
    let layout = layout_for_system(system).as_repo_layout();
    let temp = tempdir().map_err(AppError::internal)?;
    let export_root = temp.path().join("wp-station-project");
    fs::create_dir_all(&export_root).map_err(AppError::internal)?;

    copy_named_entry(&layout.models_root, &export_root, DIR_MODELS)?;
    copy_named_entry(&layout.infra_root, &export_root, DIR_CONF)?;
    copy_named_entry(&layout.connectors_root, &export_root, DIR_CONNECTORS)?;
    copy_named_entry(&layout.infra_root, &export_root, DIR_TOPOLOGY)?;

    let archive_path = temp.path().join("wp-station-project.tar.gz");
    let archive_file = File::create(&archive_path).map_err(AppError::internal)?;
    let encoder = GzEncoder::new(archive_file, Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder
        .append_dir_all("wp-station-project", &export_root)
        .map_err(AppError::internal)?;
    let encoder = builder.into_inner().map_err(AppError::internal)?;
    encoder.finish().map_err(AppError::internal)?;

    let mut bytes = Vec::new();
    File::open(&archive_path)
        .map_err(AppError::internal)?
        .read_to_end(&mut bytes)
        .map_err(AppError::internal)?;
    let file_name = format!("{}-{}.tar.gz", system.as_ref(), Utc::now().timestamp());

    Ok(ProjectArchiveExport { file_name, bytes })
}

/// 执行传统目录导入。
async fn import_project_dir(
    system: SystemKind,
    source_dir: &Path,
    layout: &RepoLayout,
    validation_message: &str,
) -> Result<ProjectImportResponse, AppError> {
    validate_legacy_project_dir(source_dir)?;
    validate_project_in_dir(system, source_dir, ProjectCheckTarget::WholeProject)?;
    let scope = detect_import_scope(source_dir)?;
    let source_label = source_dir.to_string_lossy().to_string();
    let previous_snapshot = load_project_snapshot_from_repo_layout(layout).ok();
    overwrite_repo_layout_from_legacy_dir(source_dir, layout)?;

    finalize_import_side_effects(system, layout).await?;
    build_import_response_from_source_dir(
        source_dir,
        layout,
        validation_message,
        &source_label,
        &scope,
        previous_snapshot.as_ref(),
    )
}

/// 执行归档目录导入。
async fn import_project_archive_dir(
    system: SystemKind,
    source_dir: &Path,
    layout: &RepoLayout,
    validation_message: &str,
    source_label: Option<&str>,
) -> Result<ProjectImportResponse, AppError> {
    let scope = detect_import_scope(source_dir)?;
    validate_import_scope_with_repo_layout(system, source_dir, layout, &scope)?;
    let source_label = source_label
        .map(|value| value.to_string())
        .unwrap_or_else(|| source_dir.to_string_lossy().to_string());
    let previous_snapshot = load_project_snapshot_from_repo_layout(layout).ok();

    if scope.is_full_import() {
        overwrite_repo_layout_from_legacy_dir(source_dir, layout)?;
    } else {
        overwrite_repo_layout_from_partial_dir(source_dir, layout, &scope)?;
    }

    finalize_import_side_effects(system, layout).await?;
    build_import_response_from_source_dir(
        source_dir,
        layout,
        validation_message,
        &source_label,
        &scope,
        previous_snapshot.as_ref(),
    )
}

/// 导入成功后的公共副作用。
///
/// 这里统一处理知识库重载、Gitea 同步和草稿发布刷新。
async fn finalize_import_side_effects(
    system: SystemKind,
    layout: &RepoLayout,
) -> Result<(), AppError> {
    if let Err(err) = reload_knowledge(layout) {
        warn!("知识库重载失败（忽略）: {}", err);
    }

    let commit_message = format!("导入项目配置 {}", Utc::now().format("%Y-%m-%d %H:%M:%S"));
    sync_to_gitea(&commit_message, system, crate::db::ReleaseGroup::Models).await?;
    sync_shared_connectors_to_infra_gitea(&commit_message).await?;
    refresh_draft_release_logic(system, Some(&commit_message)).await?;
    let impacted_peer = match system {
        SystemKind::Wparse => SystemKind::Wfusion,
        SystemKind::Wfusion => SystemKind::Wparse,
    };
    let _ = refresh_draft_release_logic(impacted_peer, Some(&commit_message)).await;
    Ok(())
}
