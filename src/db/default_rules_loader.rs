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

struct DefaultCopyMapping {
    source_prefix: &'static str,
    target_prefix: &'static str,
}

/// 将默认 models 配置补齐到 project_models，仅补缺失文件。
pub fn init_default_configs_to_models(project_models: &str) -> Result<(), AppError> {
    init_default_configs_with_mappings(
        project_models,
        "models",
        &[DefaultCopyMapping {
            source_prefix: "models",
            target_prefix: "models",
        }],
    )
}

/// 将默认 infra 配置补齐到 project_infra，仅补缺失文件。
pub fn init_default_configs_to_infra(project_infra: &str) -> Result<(), AppError> {
    init_default_configs_with_mappings(
        project_infra,
        "infra",
        &[
            DefaultCopyMapping {
                source_prefix: "conf",
                target_prefix: "conf",
            },
            DefaultCopyMapping {
                source_prefix: "topology",
                target_prefix: "topology",
            },
            DefaultCopyMapping {
                source_prefix: "connectors",
                target_prefix: "connectors",
            },
        ],
    )
}

fn init_default_configs_with_mappings(
    project_root: &str,
    scope: &str,
    mappings: &[DefaultCopyMapping],
) -> Result<(), AppError> {
    let project_dir = resolve_project_root(project_root);
    fs::create_dir_all(&project_dir).map_err(AppError::internal)?;

    if let Some(runtime_default_dir) = runtime_default_configs_dir() {
        return init_from_runtime_defaults(&project_dir, &runtime_default_dir, scope, mappings);
    }

    init_from_embedded_defaults(&project_dir, scope, mappings)
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

fn init_from_runtime_defaults(
    project_dir: &Path,
    default_root: &Path,
    scope: &str,
    mappings: &[DefaultCopyMapping],
) -> Result<(), AppError> {
    info!(
        "开始从运行时默认配置初始化项目目录: scope={}, default_configs_root={}",
        scope,
        default_root.display()
    );

    let mut written = 0usize;
    let mut skipped = 0usize;
    let mut matched_sources = 0usize;

    for mapping in mappings {
        let source_dir = default_root.join(mapping.source_prefix);
        if !source_dir.is_dir() {
            info!(
                "未找到运行时默认配置目录，跳过当前映射: scope={}, source={}, path={}",
                scope,
                mapping.source_prefix,
                source_dir.display()
            );
            continue;
        }

        matched_sources += 1;
        let target_root = project_dir.join(mapping.target_prefix);
        copy_default_dir(
            &source_dir,
            &source_dir,
            &target_root,
            &mut written,
            &mut skipped,
        )?;
    }

    if matched_sources == 0 {
        info!(
            "未找到任何可用运行时默认配置目录，跳过: scope={}, default_configs_root={}",
            scope,
            default_root.display()
        );
        return Ok(());
    }

    info!(
        "默认配置初始化完成: scope={}, project_dir={}, source=runtime, written={}, skipped={}",
        scope,
        project_dir.display(),
        written,
        skipped
    );
    Ok(())
}

fn copy_default_dir(
    root_dir: &Path,
    current_dir: &Path,
    target_root: &Path,
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
            copy_default_dir(root_dir, &path, target_root, written, skipped)?;
            continue;
        }

        if !path.is_file() {
            continue;
        }

        let relative_path = path
            .strip_prefix(root_dir)
            .map_err(|e| AppError::internal(format!("计算默认配置相对路径失败: {}", e)))?;
        let target_path = target_root.join(relative_path);

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

fn init_from_embedded_defaults(
    project_dir: &Path,
    scope: &str,
    mappings: &[DefaultCopyMapping],
) -> Result<(), AppError> {
    info!("开始从嵌入默认配置初始化项目目录: scope={}", scope);

    let mut written = 0usize;
    let mut skipped = 0usize;
    for file_path in DefaultConfigs::iter() {
        let path_str = file_path.as_ref();
        if should_skip_embedded_path(path_str) {
            continue;
        }

        let Some((mapping, relative)) = mappings.iter().find_map(|mapping| {
            strip_embedded_prefix(path_str, mapping.source_prefix)
                .map(|relative| (mapping, relative))
        }) else {
            continue;
        };

        let Some(content_file) = DefaultConfigs::get(path_str) else {
            continue;
        };

        let target_path = project_dir.join(mapping.target_prefix).join(relative);
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
        "默认配置初始化完成: scope={}, project_dir={}, source=embedded, written={}, skipped={}",
        scope,
        project_dir.display(),
        written,
        skipped
    );
    Ok(())
}

fn should_skip_embedded_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

fn strip_embedded_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if path == prefix {
        return Some("");
    }

    path.strip_prefix(prefix)?.strip_prefix('/')
}
