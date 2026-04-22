use crate::error::AppError;
use crate::utils::project::resolve_project_root;
use rust_embed::RustEmbed;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(RustEmbed)]
#[folder = "default_configs/"]
struct DefaultConfigs;

/// 从默认配置目录初始化 project_root。优先读取运行时 default_configs，缺失时回退到嵌入配置。
pub fn init_default_configs_to_project(project_root: &str) -> Result<(), AppError> {
    let project_dir = resolve_project_root(project_root);
    fs::create_dir_all(&project_dir).map_err(AppError::internal)?;

    if let Some(runtime_default_dir) = runtime_default_configs_dir() {
        return init_from_runtime_defaults(&project_dir, &runtime_default_dir);
    }

    init_from_embedded_defaults(&project_dir)
}

fn runtime_default_configs_dir() -> Option<PathBuf> {
    let candidate = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("default_configs");

    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

fn init_from_runtime_defaults(project_dir: &Path, default_dir: &Path) -> Result<(), AppError> {
    info!(
        "开始从运行时默认配置初始化 project_root: default_configs={}",
        default_dir.display()
    );

    let mut written = 0usize;
    let mut skipped = 0usize;
    copy_default_dir(
        default_dir,
        default_dir,
        project_dir,
        &mut written,
        &mut skipped,
    )?;

    info!(
        "默认配置初始化完成: project_root={}, source=runtime, written={}, skipped={}",
        project_dir.display(),
        written,
        skipped
    );
    Ok(())
}

fn copy_default_dir(
    root_dir: &Path,
    current_dir: &Path,
    project_dir: &Path,
    written: &mut usize,
    skipped: &mut usize,
) -> Result<(), AppError> {
    for entry in fs::read_dir(current_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };

        if file_name.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            copy_default_dir(root_dir, &path, project_dir, written, skipped)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative_path = path
            .strip_prefix(root_dir)
            .map_err(|e| AppError::internal(format!("计算默认配置相对路径失败: {}", e)))?;
        let target_path = project_dir.join(relative_path);

        if target_path.exists() {
            *skipped += 1;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }

        fs::copy(&path, &target_path).map_err(AppError::internal)?;
        *written += 1;
        debug!("写入默认配置文件: path={}", target_path.display());
    }

    Ok(())
}

fn init_from_embedded_defaults(project_dir: &Path) -> Result<(), AppError> {
    info!("开始从嵌入的默认配置初始化 project_root");

    let mut written = 0usize;
    let mut skipped = 0usize;
    for file_path in DefaultConfigs::iter() {
        let path_str = file_path.as_ref();
        if should_skip_embedded_path(path_str) {
            continue;
        }

        let Some(content_file) = DefaultConfigs::get(path_str) else {
            continue;
        };

        let target_path = project_dir.join(path_str);
        if target_path.exists() {
            skipped += 1;
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        fs::write(&target_path, content_file.data.as_ref()).map_err(AppError::internal)?;
        written += 1;
        debug!("写入默认配置文件: path={}", target_path.display());
    }

    info!(
        "默认配置初始化完成: project_root={}, source=embedded, written={}, skipped={}",
        project_dir.display(),
        written,
        skipped
    );
    Ok(())
}

fn should_skip_embedded_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}
