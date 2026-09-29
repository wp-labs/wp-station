//! 项目归档导入相关的文件处理辅助。
//!
//! 这一层只负责：
//! - 归档暂存目录管理
//! - 上传文件名和格式校验
//! - 解压与安全路径处理
//! - 识别解压后的真实项目根目录

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use flate2::read::GzDecoder;
use zip::ZipArchive;

use crate::constants::project::{
    ARCHIVE_IMPORT_STAGING_DIR, DIR_CONF, DIR_CONNECTORS, DIR_MODELS, DIR_TOPOLOGY,
    IMPORTABLE_ROOT_DIRS,
};
use crate::error::AppError;
use crate::utils::{ProjectArea, SystemKind, repo_name};

/// 返回归档导入暂存根目录。
pub(super) fn archive_import_staging_root() -> PathBuf {
    std::env::temp_dir().join(ARCHIVE_IMPORT_STAGING_DIR)
}

/// 生成导入任务 ID。
pub(super) fn new_archive_import_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("import-{}-{}", std::process::id(), millis)
}

/// 解析并校验导入任务目录。
pub(super) fn archive_import_dir(import_id: &str) -> Result<PathBuf, AppError> {
    let valid = !import_id.is_empty()
        && import_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
    if !valid {
        return Err(AppError::validation("导入任务 ID 无效".to_string()));
    }
    Ok(archive_import_staging_root().join(import_id))
}

/// 规范化上传文件名并限制为支持的压缩格式。
pub(super) fn sanitize_upload_name(file_name: &str) -> Result<String, AppError> {
    let name = Path::new(file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .ok_or_else(|| AppError::validation("上传文件名无效".to_string()))?;

    if is_supported_archive(&name) {
        Ok(name)
    } else {
        Err(AppError::validation(
            "仅支持 tar、tar.gz、tgz、zip 格式".to_string(),
        ))
    }
}

/// 判断上传文件是否属于支持的归档格式。
pub(super) fn is_supported_archive(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".tar")
        || lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".zip")
}

/// 根据文件扩展名分发归档解压逻辑。
pub(super) fn extract_archive(
    file_name: &str,
    archive_path: &Path,
    target_dir: &Path,
) -> Result<(), AppError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".zip") {
        extract_zip_archive(archive_path, target_dir)
    } else if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        let file = File::open(archive_path).map_err(AppError::internal)?;
        let decoder = GzDecoder::new(file);
        extract_tar_reader(decoder, target_dir)
    } else if lower.ends_with(".tar") {
        let file = File::open(archive_path).map_err(AppError::internal)?;
        extract_tar_reader(file, target_dir)
    } else {
        Err(AppError::validation(
            "仅支持 tar、tar.gz、tgz、zip 格式".to_string(),
        ))
    }
}

/// 解压 tar/tar.gz/tgz 归档。
fn extract_tar_reader<R: Read>(reader: R, target_dir: &Path) -> Result<(), AppError> {
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(AppError::internal)? {
        let mut entry = entry.map_err(AppError::internal)?;
        let entry_path = entry.path().map_err(AppError::internal)?.to_path_buf();
        if should_skip_archive_path(&entry_path) {
            continue;
        }
        let safe_path = safe_join(target_dir, &entry_path)?;
        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        entry.unpack(&safe_path).map_err(AppError::internal)?;
    }
    Ok(())
}

/// 解压 zip 归档。
fn extract_zip_archive(archive_path: &Path, target_dir: &Path) -> Result<(), AppError> {
    let file = File::open(archive_path).map_err(AppError::internal)?;
    let mut archive = ZipArchive::new(file).map_err(AppError::internal)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(AppError::internal)?;
        let Some(enclosed_name) = entry.enclosed_name().map(|path| path.to_path_buf()) else {
            continue;
        };
        if should_skip_archive_path(&enclosed_name) {
            continue;
        }
        let out_path = safe_join(target_dir, &enclosed_name)?;
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(AppError::internal)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(AppError::internal)?;
            }
            let mut out_file = File::create(&out_path).map_err(AppError::internal)?;
            std::io::copy(&mut entry, &mut out_file).map_err(AppError::internal)?;
        }
    }
    Ok(())
}

/// 跳过归档中的系统垃圾文件。
pub(super) fn should_skip_archive_path(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy();
        name == "__MACOSX" || name == ".DS_Store" || name.starts_with("._")
    })
}

/// 安全拼接归档内相对路径，防止路径穿越。
fn safe_join(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(AppError::validation(format!(
            "压缩包包含不安全路径: {}",
            relative.display()
        )));
    }
    Ok(root.join(relative))
}

/// 在解压目录中定位真正的项目根目录。
pub(super) fn find_import_project_root(
    extract_dir: &Path,
    system: SystemKind,
) -> Result<PathBuf, AppError> {
    if has_supported_import_layout(extract_dir, system) {
        let normalized = normalize_import_root(extract_dir, system)?;
        persist_preview_project_dir(extract_dir, &normalized)?;
        return Ok(normalized);
    }

    let mut dirs = Vec::new();
    for entry in fs::read_dir(extract_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry.path().is_dir() {
            dirs.push(entry.path());
        }
    }

    if dirs.len() == 1 && has_supported_import_layout(&dirs[0], system) {
        let normalized = normalize_import_root(&dirs[0], system)?;
        persist_preview_project_dir(extract_dir, &normalized)?;
        Ok(normalized)
    } else {
        Err(AppError::validation(
            "压缩包结构无效。解压后应直接包含 conf、connectors、topology、models 中的一个或多个目录"
                .to_string(),
        ))
    }
}

/// 持久化预检时解析出的项目目录位置，供确认导入阶段复用。
fn persist_preview_project_dir(extract_dir: &Path, project_dir: &Path) -> Result<(), AppError> {
    let Some(import_dir) = extract_dir.parent() else {
        return Ok(());
    };
    fs::write(
        import_dir.join("project_dir.txt"),
        project_dir.to_string_lossy().as_ref(),
    )
    .map_err(AppError::internal)
}

/// 判断目录是否具有可识别的导入结构。
fn has_supported_import_layout(dir: &Path, system: SystemKind) -> bool {
    has_flat_import_dirs(dir) || has_split_import_dirs(dir, system)
}

/// 判断目录是否采用扁平导入结构。
fn has_flat_import_dirs(dir: &Path) -> bool {
    IMPORTABLE_ROOT_DIRS
        .iter()
        .any(|name| dir.join(name).is_dir())
}

/// 判断目录是否采用分仓导入结构。
fn has_split_import_dirs(dir: &Path, system: SystemKind) -> bool {
    let models_repo = repo_name(system, ProjectArea::Models);
    let infra_repo = repo_name(system, ProjectArea::Infra);

    dir.join(models_repo).join(DIR_MODELS).is_dir()
        || dir.join(infra_repo).join(DIR_CONF).is_dir()
        || dir.join(infra_repo).join(DIR_CONNECTORS).is_dir()
        || dir.join(infra_repo).join(DIR_TOPOLOGY).is_dir()
}

/// 将分仓导入结构规范化为扁平目录，供后续复用同一套导入逻辑。
fn normalize_import_root(dir: &Path, system: SystemKind) -> Result<PathBuf, AppError> {
    if has_flat_import_dirs(dir) {
        return Ok(dir.to_path_buf());
    }

    let models_repo = repo_name(system, ProjectArea::Models);
    let infra_repo = repo_name(system, ProjectArea::Infra);
    let normalized = dir.join("__normalized_default_configs");
    fs::create_dir_all(&normalized).map_err(AppError::internal)?;
    if dir.join(models_repo).join(DIR_MODELS).is_dir() {
        super::copy_named_entry(&dir.join(models_repo), &normalized, DIR_MODELS)?;
    }
    for name in [DIR_CONF, DIR_CONNECTORS, DIR_TOPOLOGY] {
        if dir.join(infra_repo).join(name).is_dir() {
            super::copy_named_entry(&dir.join(infra_repo), &normalized, name)?;
        }
    }
    Ok(normalized)
}
