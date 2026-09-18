//! 默认配置初始化模块。
//!
//! 负责把内置或运行时 `default_configs` 目录中的默认文件补齐到项目目录。
//! 只补缺失文件，不覆盖用户已有内容。

use crate::constants::project::{
    DIR_CONF, DIR_CONNECTORS, DIR_MODELS, DIR_RULES, DIR_RUNTIME, DIR_TOPOLOGY,
    FILE_WFUSION_GLOBAL_RULE,
};
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::SystemKind;
use rust_embed::RustEmbed;
use std::{
    fs,
    path::{Path, PathBuf},
};

use super::resolve_project_root;

/// 编译时嵌入的默认配置资源。
#[derive(RustEmbed)]
#[folder = "default_configs/"]
struct DefaultConfigs;

/// 默认配置复制映射。
///
/// `source_prefix` 指向默认配置来源目录前缀，
/// `target_prefix` 指向项目目录中的目标前缀。
struct DefaultCopyMapping {
    source_prefix: &'static str,
    target_prefix: &'static str,
}

/// 将默认 models 配置补齐到指定 models 仓库，仅补缺失文件。
pub fn init_default_configs_to_models(models_root: &str) -> Result<(), AppError> {
    init_default_configs_to_models_for_system(SystemKind::Wparse, models_root)
}

/// 将默认 infra 配置补齐到指定 infra 仓库，仅补缺失文件。
pub fn init_default_configs_to_infra(infra_root: &str) -> Result<(), AppError> {
    init_default_configs_to_infra_for_system(SystemKind::Wparse, infra_root)
}

/// 按系统将默认 models 配置补齐到指定 models 仓库，仅补缺失文件。
pub fn init_default_configs_to_models_for_system(
    system: SystemKind,
    models_root: &str,
) -> Result<(), AppError> {
    let result = match system {
        SystemKind::Wparse => init_default_configs_with_mappings(
            models_root,
            "wparse/models",
            &[DefaultCopyMapping {
                source_prefix: "wparse/models",
                target_prefix: DIR_MODELS,
            }],
        ),
        SystemKind::Wfusion => init_default_configs_with_mappings(
            models_root,
            "wfusion/models",
            &[DefaultCopyMapping {
                source_prefix: "wfusion/models",
                target_prefix: DIR_MODELS,
            }],
        ),
    };
    result?;

    // 全局规则是 WFusion 规则编辑器的固定入口，已有项目也必须补齐，但绝不覆盖现有内容。
    if matches!(system, SystemKind::Wfusion) {
        let global_rule = resolve_project_root(models_root)
            .join(DIR_MODELS)
            .join(DIR_RULES)
            .join(FILE_WFUSION_GLOBAL_RULE);
        if let Some(parent) = global_rule.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        if !global_rule.exists() {
            let default_path = "wfusion/models/rules/_global.wfl";
            let content = runtime_default_configs_dir()
                .map(|root| root.join(default_path))
                .filter(|path| path.is_file())
                .map(fs::read)
                .transpose()
                .map_err(AppError::internal)?
                .or_else(|| DefaultConfigs::get(default_path).map(|file| file.data.into_owned()))
                .unwrap_or_default();
            fs::write(&global_rule, content).map_err(AppError::internal)?;
            info!("补齐 WFusion 全局规则文件: path={}", global_rule.display());
        }
    }

    Ok(())
}

/// 按系统将默认 infra 配置补齐到指定 infra 仓库，仅补缺失文件。
pub fn init_default_configs_to_infra_for_system(
    system: SystemKind,
    infra_root: &str,
) -> Result<(), AppError> {
    let result = match system {
        SystemKind::Wparse => init_default_configs_with_mappings(
            infra_root,
            "wparse/infra",
            &[
                DefaultCopyMapping {
                    source_prefix: "wparse/conf",
                    target_prefix: DIR_CONF,
                },
                DefaultCopyMapping {
                    source_prefix: "wparse/connectors",
                    target_prefix: DIR_CONNECTORS,
                },
                DefaultCopyMapping {
                    source_prefix: "wparse/topology",
                    target_prefix: DIR_TOPOLOGY,
                },
                DefaultCopyMapping {
                    source_prefix: "wparse/runtime",
                    target_prefix: DIR_RUNTIME,
                },
            ],
        ),
        SystemKind::Wfusion => init_default_configs_with_mappings(
            infra_root,
            "wfusion/infra",
            &[
                DefaultCopyMapping {
                    source_prefix: "wfusion/conf",
                    target_prefix: DIR_CONF,
                },
                DefaultCopyMapping {
                    source_prefix: "wfusion/connectors",
                    target_prefix: DIR_CONNECTORS,
                },
                DefaultCopyMapping {
                    source_prefix: "wfusion/topology",
                    target_prefix: DIR_TOPOLOGY,
                },
                DefaultCopyMapping {
                    source_prefix: "wfusion/runtime",
                    target_prefix: DIR_RUNTIME,
                },
            ],
        ),
    };
    result?;

    // connectors 属于各系统自己的 infra。即使仓库已有 conf/topology，也要逐文件
    // 补齐缺失的默认 connector，但绝不覆盖用户已修改内容。
    let connector_source = match system {
        SystemKind::Wparse => "wparse/connectors",
        SystemKind::Wfusion => "wfusion/connectors",
    };
    ensure_default_configs_with_mappings(
        infra_root,
        connector_source,
        &[DefaultCopyMapping {
            source_prefix: connector_source,
            target_prefix: DIR_CONNECTORS,
        }],
    )?;

    // infra 目录已有其他配置时，主初始化会保护用户内容并跳过默认补齐；
    // business.d 新增模板仍需逐文件补齐，不能因为 sink.toml 已存在而遗漏新文件。
    if matches!(system, SystemKind::Wparse) {
        ensure_default_configs_with_mappings(
            infra_root,
            "wparse/topology/sinks/business.d",
            &[DefaultCopyMapping {
                source_prefix: "wparse/topology/sinks/business.d",
                target_prefix: "topology/sinks/business.d",
            }],
        )?;
    }

    Ok(())
}

/// 按映射规则补齐默认配置。
///
/// 优先使用工作区中的 `default_configs` 目录；
/// 如果不存在，则回退到编译时嵌入的默认配置。
fn init_default_configs_with_mappings(
    project_root: &str,
    scope: &str,
    mappings: &[DefaultCopyMapping],
) -> Result<(), AppError> {
    let project_dir = resolve_project_root(project_root);
    fs::create_dir_all(&project_dir).map_err(AppError::internal)?;

    // 仓库已有实际内容时，不再补齐默认样例，避免导入覆盖后又把旧样例补回。
    if has_existing_managed_files(&project_dir, mappings)? {
        info!(
            "项目目录已存在实际配置文件，跳过默认配置补齐: scope={}, project_dir={}",
            scope,
            project_dir.display()
        );
        return Ok(());
    }

    if let Some(runtime_default_dir) = runtime_default_configs_dir() {
        return init_from_runtime_defaults(&project_dir, &runtime_default_dir, scope, mappings);
    }

    init_from_embedded_defaults(&project_dir, scope, mappings)
}

/// 补齐一组默认文件，但不因目标目录已有其他文件而整体跳过。
///
/// 用于运行中新增的受控模板：只写入缺失文件，绝不覆盖用户已经修改的内容。
fn ensure_default_configs_with_mappings(
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

fn has_existing_managed_files(
    project_dir: &Path,
    mappings: &[DefaultCopyMapping],
) -> Result<bool, AppError> {
    for mapping in mappings {
        if dir_contains_visible_files(&project_dir.join(mapping.target_prefix))? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn dir_contains_visible_files(dir: &Path) -> Result<bool, AppError> {
    if !dir.is_dir() {
        return Ok(false);
    }

    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        let Some(name) = entry.file_name().to_str().map(|value| value.to_string()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        if path.is_file() {
            return Ok(true);
        }
        if path.is_dir() && dir_contains_visible_files(&path)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// 返回运行时默认配置目录。
///
/// 该目录存在时优先于嵌入资源，便于开发和测试时按文件覆盖默认配置。
pub fn runtime_default_configs_dir() -> Option<PathBuf> {
    let candidate = Setting::workspace_root().join("default_configs");

    if candidate.is_dir() {
        Some(candidate)
    } else {
        None
    }
}

/// 从运行时 `default_configs` 目录补齐默认配置。
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

/// 递归复制默认配置目录，只写入目标中尚不存在的文件。
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

        copy_file_preserve_permissions(&path, &target_path)?;
        *written += 1;
        debug!("写入默认配置文件: path={}", target_path.display());
    }

    Ok(())
}

fn copy_file_preserve_permissions(source: &Path, target: &Path) -> Result<(), AppError> {
    fs::copy(source, target).map_err(AppError::internal)?;
    let permissions = fs::metadata(source)
        .map_err(AppError::internal)?
        .permissions();
    fs::set_permissions(target, permissions).map_err(AppError::internal)?;
    Ok(())
}

/// 从嵌入资源补齐默认配置。
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

/// 判断嵌入资源路径是否应跳过。
fn should_skip_embedded_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

/// 去掉嵌入资源路径前缀，返回相对路径。
fn strip_embedded_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if path == prefix {
        return Some("");
    }

    path.strip_prefix(prefix)?.strip_prefix('/')
}
