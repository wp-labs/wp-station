//! 项目目录覆盖与复制辅助。

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants::project::{
    DIR_CONF, DIR_CONNECTORS, DIR_MODELS, DIR_TOPOLOGY, IMPORTABLE_ROOT_DIRS,
};
use crate::error::AppError;
use crate::server::{RepoLayout, Setting};

use super::ImportScope;

/// 规范化并校验目录导入路径。
pub(super) fn normalize_source_dir(raw: &str) -> Result<PathBuf, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("请输入待导入的源目录".to_string()));
    }

    let path = PathBuf::from(trimmed);
    let normalized = if path.is_absolute() {
        path
    } else {
        Setting::workspace_root().join(path)
    };

    if !normalized.is_dir() {
        return Err(AppError::validation(format!(
            "源目录不存在或不是目录: {}",
            normalized.display()
        )));
    }

    Ok(normalized)
}

/// 校验传统目录导入必须包含四个核心顶层目录。
pub(super) fn validate_legacy_project_dir(source_dir: &Path) -> Result<(), AppError> {
    let missing_dirs: Vec<String> = IMPORTABLE_ROOT_DIRS
        .iter()
        .filter_map(|name| {
            let path = source_dir.join(name);
            if path.is_dir() {
                None
            } else {
                Some((*name).to_string())
            }
        })
        .collect();

    if !missing_dirs.is_empty() {
        return Err(AppError::validation(format!(
            "源目录结构不完整，缺少必要目录: {}",
            missing_dirs.join(", ")
        )));
    }

    Ok(())
}

/// 按旧项目目录结构覆盖到当前双仓库布局。
pub(super) fn overwrite_repo_layout_from_legacy_dir(
    source_dir: &Path,
    layout: &RepoLayout,
) -> Result<(), AppError> {
    info!(
        "开始按旧目录结构拆分覆盖仓库: source_dir={}, models_dir={}, infra_dir={}, connectors_dir={}",
        source_dir.display(),
        layout.models_root.display(),
        layout.infra_root.display(),
        layout.connectors_root.display()
    );

    recreate_dir_preserving_git(&layout.models_root)?;
    recreate_dir_preserving_git(&layout.infra_root)?;

    copy_named_entry(source_dir, &layout.infra_root, DIR_CONF)?;
    copy_named_entry(source_dir, &layout.infra_root, DIR_CONNECTORS)?;
    copy_named_entry(source_dir, &layout.infra_root, DIR_TOPOLOGY)?;
    copy_named_entry(source_dir, &layout.models_root, DIR_MODELS)?;

    info!(
        "旧目录拆分覆盖完成: source_dir={}, models_dir={}, infra_dir={}, connectors_dir={}",
        source_dir.display(),
        layout.models_root.display(),
        layout.infra_root.display(),
        layout.connectors_root.display()
    );
    Ok(())
}

/// 按部分目录覆盖到当前双仓库布局。
pub(super) fn overwrite_repo_layout_from_partial_dir(
    source_dir: &Path,
    layout: &RepoLayout,
    scope: &ImportScope,
) -> Result<(), AppError> {
    info!(
        "开始按归档子集覆盖双仓库: source_dir={}, imported_dirs={}",
        source_dir.display(),
        scope.imported_dirs.join(", ")
    );

    for name in &scope.imported_dirs {
        match *name {
            DIR_MODELS => {
                // models 仓库只承载 models 目录，导入时应整体覆盖，避免旧规则残留。
                recreate_dir_preserving_git(&layout.models_root)?;
                copy_named_entry(source_dir, &layout.models_root, name)?;
            }
            DIR_CONNECTORS => {
                // connectors 属于 infra，只覆盖该子目录，不能清空 conf/topology。
                remove_named_entry(&layout.infra_root, name)?;
                copy_named_entry(source_dir, &layout.infra_root, name)?;
            }
            DIR_CONF | DIR_TOPOLOGY => copy_named_entry(source_dir, &layout.infra_root, name)?,
            _ => {}
        }
    }

    info!(
        "归档子集覆盖完成: source_dir={}, imported_dirs={}",
        source_dir.display(),
        scope.imported_dirs.join(", ")
    );
    Ok(())
}

fn remove_named_entry(target_root: &Path, name: &str) -> Result<(), AppError> {
    let target = target_root.join(name);
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(AppError::internal)?;
    } else if target.exists() {
        fs::remove_file(target).map_err(AppError::internal)?;
    }
    Ok(())
}

/// 仅把本次导入涉及的目录应用到目标目录。
pub(super) fn apply_import_scope_to_dir(
    source_dir: &Path,
    target_dir: &Path,
    scope: &ImportScope,
) -> Result<(), AppError> {
    for name in &scope.imported_dirs {
        copy_named_entry(source_dir, target_dir, name)?;
    }
    Ok(())
}

/// 清空目标目录，但保留 `.git` 目录。
fn recreate_dir_preserving_git(target_dir: &Path) -> Result<(), AppError> {
    fs::create_dir_all(target_dir).map_err(AppError::internal)?;

    for entry in fs::read_dir(target_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if entry.file_name() == OsStr::new(".git") {
            continue;
        }

        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(AppError::internal)?;
        } else {
            fs::remove_file(&path).map_err(AppError::internal)?;
        }
    }

    Ok(())
}

/// 将命名目录或文件从源目录复制到目标目录。
pub(super) fn copy_named_entry(
    source_root: &Path,
    target_root: &Path,
    name: &str,
) -> Result<(), AppError> {
    let source = source_root.join(name);
    if !source.exists() {
        return Err(AppError::validation(format!(
            "源目录缺少必要内容: {}",
            source.display()
        )));
    }

    let target = target_root.join(name);
    if target.exists() {
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(AppError::internal)?;
        } else {
            fs::remove_file(&target).map_err(AppError::internal)?;
        }
    }

    if source.is_dir() {
        copy_dir_recursive(&source, &target)?;
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        fs::copy(&source, &target).map_err(AppError::internal)?;
    }

    Ok(())
}

/// 递归复制目录。
fn copy_dir_recursive(source_dir: &Path, target_dir: &Path) -> Result<(), AppError> {
    fs::create_dir_all(target_dir).map_err(AppError::internal)?;

    for entry in fs::read_dir(source_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if super::archive::should_skip_archive_path(&PathBuf::from(entry.file_name())) {
            continue;
        }
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(AppError::internal)?;
            }
            fs::copy(&source_path, &target_path).map_err(AppError::internal)?;
        }
    }

    Ok(())
}
