//! 项目文件 I/O 模块。
//!
//! 负责双仓库项目中的规则、配置、知识库文件读写、扫描和快照加载，
//! 是文件系统与业务层之间的桥梁。

mod defaults;
mod files;
mod snapshot;

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::constants::project::{
    DIR_CONF, DIR_CONNECTORS, DIR_MODELS, DIR_OML, DIR_RULES, DIR_RUNTIME, DIR_SCENARIOS,
    DIR_SCHEMAS, DIR_SINK_D, DIR_SINKS, DIR_SOURCE_D, DIR_SOURCES, DIR_TOPOLOGY, DIR_WPL,
    FILE_OML_ADM, FILE_WFUSION, FILE_WINDOWS, FILE_WPARSE, FILE_WPGEN, FILE_WPL_PARSE,
    FILE_WPL_SAMPLE,
};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::{RepoLayout, Setting};

pub use self::defaults::{
    init_default_configs_to_infra, init_default_configs_to_infra_for_system,
    init_default_configs_to_models, init_default_configs_to_models_for_system,
    runtime_default_configs_dir,
};
pub use self::files::{
    delete_knowledge_from_project, delete_rule_from_project, list_knowledge_dirs, list_rule_files,
    read_knowdb_config, read_knowledge_files, read_rule_content, read_wpl_sample_content,
    touch_knowledge_in_project, touch_rule_in_project, write_knowdb_config, write_knowledge_files,
    write_rule_content, write_rule_content_in_project_dir, write_wpl_sample_content,
};
pub use self::snapshot::{load_project_snapshot, load_project_snapshot_from_repo_layout};

/// 项目规则文件快照。
#[derive(Debug, Clone)]
pub struct ProjectRuleFile {
    pub rule_type: RuleType,
    pub file_name: String,
    pub content: Option<String>,
    pub sample_content: Option<String>,
    pub display_name: Option<String>,
    pub file_size: Option<i32>,
}

/// 知识库目录快照。
#[derive(Debug, Clone)]
pub struct KnowledgeFiles {
    pub file_name: String,
    pub config_content: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data_content: Option<String>,
    pub last_modified: Option<SystemTime>,
}

/// 项目整体快照。
#[derive(Default, Clone)]
pub struct ProjectSnapshot {
    pub rules: Vec<ProjectRuleFile>,
    pub knowledge: Vec<KnowledgeFiles>,
    pub rule_stats: HashMap<RuleType, usize>,
    pub warnings: Vec<String>,
    pub failed_files: usize,
}

impl ProjectSnapshot {
    fn add_rule(
        &mut self,
        rule_type: RuleType,
        file_name: String,
        content: Option<String>,
        sample_content: Option<String>,
        display_name: Option<String>,
    ) {
        let file_size = content.as_ref().map(|c| c.len() as i32);
        self.rules.push(ProjectRuleFile {
            rule_type,
            file_name,
            display_name,
            content,
            sample_content,
            file_size,
        });
        *self.rule_stats.entry(rule_type).or_insert(0) += 1;
    }

    fn add_knowledge(&mut self, config: KnowledgeFiles) {
        self.knowledge.push(config);
    }

    /// 合并另一个项目快照。
    pub fn merge(&mut self, mut other: ProjectSnapshot) {
        self.rules.append(&mut other.rules);
        self.knowledge.append(&mut other.knowledge);
        for (rule_type, count) in other.rule_stats {
            *self.rule_stats.entry(rule_type).or_insert(0) += count;
        }
        self.warnings.append(&mut other.warnings);
        self.failed_files += other.failed_files;
    }

    /// 返回规则类型维度的数量拆分。
    pub fn rule_breakdown(&self) -> Vec<(RuleType, usize)> {
        let mut items: Vec<(RuleType, usize)> = self
            .rule_stats
            .iter()
            .map(|(ty, count)| (*ty, *count))
            .collect();
        items.sort_by_key(|(ty, _)| ty.as_ref().to_string());
        items
    }
}

/// 统一将配置中的目录解析成工作区下的稳定路径，避免相对路径受当前工作目录影响。
pub fn resolve_project_root(project_root: &str) -> PathBuf {
    let path = PathBuf::from(project_root);
    if path.is_absolute() {
        path
    } else {
        Setting::workspace_root().join(path)
    }
}

/// 根据规则类型确定应访问的物理仓库目录。
pub fn resolve_dir_for_rule(layout: &RepoLayout, rule_type: RuleType) -> PathBuf {
    match rule_type {
        RuleType::Wpl
        | RuleType::Oml
        | RuleType::Windows
        | RuleType::Schema
        | RuleType::Rule
        | RuleType::Scenarios
        | RuleType::Knowledge => layout.models_root.clone(),
        RuleType::SourceConnect | RuleType::SinkConnect => layout.connectors_root.clone(),
        _ => layout.infra_root.clone(),
    }
}

/// 将双仓库的逻辑目录合成到一个目标目录中。
pub fn compose_repo_layout_into(layout: &RepoLayout, target_dir: &Path) -> Result<(), AppError> {
    copy_named_entry(&layout.models_root, target_dir, DIR_MODELS)?;
    copy_named_entry(&layout.infra_root, target_dir, DIR_CONF)?;
    copy_named_entry(&layout.infra_root, target_dir, DIR_RUNTIME)?;
    copy_named_entry(&layout.infra_root, target_dir, DIR_TOPOLOGY)?;
    copy_named_entry(&layout.connectors_root, target_dir, DIR_CONNECTORS)?;
    Ok(())
}

/// 根据 rule_type 和 file_name 计算规则配置文件的读取路径。
///
/// 对 `wfusion` 命名规则同时兼容：
/// - 新布局：`rules/foo.wfl`
/// - 旧布局：`rules/foo/foo.wfl`
/// - 显式旧路径读取新布局：`foo/foo.wfl` -> `foo.wfl`
fn rule_target_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    match rule_type {
        RuleType::Parse => Ok(project_dir
            .join(DIR_CONF)
            .join(resolve_parse_file_name(project_dir, file_name))),
        RuleType::Wpgen => Ok(project_dir.join(DIR_CONF).join(FILE_WPGEN)),
        RuleType::SourceConnect => connector_rule_path(project_dir, DIR_SOURCE_D, file_name),
        RuleType::SinkConnect => connector_rule_path(project_dir, DIR_SINK_D, file_name),
        RuleType::Source => Ok(project_dir
            .join(DIR_TOPOLOGY)
            .join(DIR_SOURCES)
            .join(file_name)),
        RuleType::Sink => Ok(project_dir
            .join(DIR_TOPOLOGY)
            .join(DIR_SINKS)
            .join(file_name)),
        RuleType::Wpl => {
            let (parse_path, _) = wpl_rule_paths(project_dir, file_name);
            Ok(parse_path)
        }
        RuleType::Oml => Ok(project_dir
            .join(DIR_MODELS)
            .join(DIR_OML)
            .join(file_name)
            .join(FILE_OML_ADM)),
        RuleType::Windows => wfusion_windows_path(project_dir, file_name),
        RuleType::Schema => wfusion_schema_read_path(project_dir, file_name),
        RuleType::Rule => wfusion_rule_read_path(project_dir, file_name),
        RuleType::Scenarios => wfusion_scenario_read_path(project_dir, file_name),
        RuleType::Knowledge => Err(AppError::validation("知识库配置请使用 knowledge 文件接口")),
        RuleType::All => Err(AppError::validation("all 类型不能映射到单个规则文件")),
    }
}

/// 根据 rule_type 和 file_name 计算规则配置文件的写入路径。
///
/// `wfusion` 的 schema / rule / scenario 在写入时统一落到新布局，避免继续生成旧目录。
fn storage_rule_target_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    match rule_type {
        RuleType::Schema => wfusion_schema_storage_path(project_dir, file_name),
        RuleType::Rule => wfusion_rule_storage_path(project_dir, file_name),
        RuleType::Scenarios => wfusion_scenario_storage_path(project_dir, file_name),
        _ => rule_target_path(project_dir, rule_type, file_name),
    }
}

fn validation_target_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    if matches!(rule_type, RuleType::Wpl) {
        let trimmed = file_name.trim().trim_matches('/');
        if trimmed.is_empty() {
            return Err(AppError::validation("wpl 文件名不能为空"));
        }

        if let Some(base) = trimmed.strip_suffix(&format!("/{}", FILE_WPL_SAMPLE)) {
            let (_, sample_path) = wpl_rule_paths(project_dir, base.trim_matches('/'));
            return Ok(sample_path);
        }

        if let Some(base) = trimmed.strip_suffix(&format!("/{}", FILE_WPL_PARSE)) {
            let (parse_path, _) = wpl_rule_paths(project_dir, base.trim_matches('/'));
            return Ok(parse_path);
        }
    }

    storage_rule_target_path(project_dir, rule_type, file_name)
}

/// 计算 connectors/<folder>/<file_name>.toml 形式的路径。
fn connector_rule_path(
    project_dir: &Path,
    folder: &str,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    Ok(project_dir
        .join(DIR_CONNECTORS)
        .join(folder)
        .join(with_extension(file_name, ".toml")))
}

fn wfusion_schema_read_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_read_path(project_dir, DIR_SCHEMAS, file_name, ".wfs")
}

fn wfusion_schema_storage_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_storage_path(project_dir, DIR_SCHEMAS, file_name, ".wfs")
}

fn wfusion_windows_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    let trimmed = file_name.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }

    let file = trimmed.rsplit('/').next().unwrap_or(trimmed);
    if file != FILE_WINDOWS {
        return Err(AppError::validation(format!(
            "wfusion windows 配置文件固定为 {}",
            FILE_WINDOWS
        )));
    }

    Ok(project_dir.join(DIR_MODELS).join(FILE_WINDOWS))
}

fn wfusion_rule_read_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_read_path(project_dir, DIR_RULES, file_name, ".wfl")
}

fn wfusion_rule_storage_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_storage_path(project_dir, DIR_RULES, file_name, ".wfl")
}

fn wfusion_scenario_read_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_read_path(project_dir, DIR_SCENARIOS, file_name, ".wfg")
}

fn wfusion_scenario_storage_path(project_dir: &Path, file_name: &str) -> Result<PathBuf, AppError> {
    wfusion_model_rule_storage_path(project_dir, DIR_SCENARIOS, file_name, ".wfg")
}

fn wfusion_model_rule_read_path(
    project_dir: &Path,
    category_dir: &str,
    file_name: &str,
    extension: &str,
) -> Result<PathBuf, AppError> {
    let root = project_dir.join(DIR_MODELS).join(category_dir);
    let storage_path =
        wfusion_model_rule_storage_path(project_dir, category_dir, file_name, extension)?;

    if storage_path.exists() {
        return Ok(storage_path);
    }

    if let Some(flat_relative) = flat_wfusion_rule_virtual_file(file_name, extension)? {
        let flat_path = root.join(flat_relative);
        if flat_path.exists() {
            return Ok(flat_path);
        }
    }

    if let Some(legacy_relative) = legacy_wfusion_rule_virtual_file(file_name, extension)? {
        let legacy_path = root.join(legacy_relative);
        if legacy_path.exists() {
            return Ok(legacy_path);
        }
    }

    Ok(storage_path)
}

fn wfusion_model_rule_storage_path(
    project_dir: &Path,
    category_dir: &str,
    file_name: &str,
    extension: &str,
) -> Result<PathBuf, AppError> {
    let root = project_dir.join(DIR_MODELS).join(category_dir);
    let normalized = normalize_wfusion_rule_virtual_file(file_name, extension)?;
    Ok(root.join(normalized))
}

fn normalize_wfusion_rule_virtual_file(
    file_name: &str,
    extension: &str,
) -> Result<String, AppError> {
    let trimmed = file_name.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }

    if trimmed.contains('/') {
        return Ok(with_extension(trimmed, extension));
    }

    Ok(with_extension(trimmed, extension))
}

fn legacy_wfusion_rule_virtual_file(
    file_name: &str,
    extension: &str,
) -> Result<Option<String>, AppError> {
    let trimmed = file_name.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }
    if trimmed.contains('/') {
        return Ok(None);
    }

    let base = trimmed
        .strip_suffix(extension)
        .unwrap_or(trimmed)
        .trim_matches('/');
    if base.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }

    Ok(Some(format!("{base}/{}{}", base, extension)))
}

fn flat_wfusion_rule_virtual_file(
    file_name: &str,
    extension: &str,
) -> Result<Option<String>, AppError> {
    let trimmed = file_name.trim().trim_matches('/');
    if trimmed.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }
    if !trimmed.contains('/') {
        return Ok(None);
    }

    let file = trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .trim_matches('/');
    if file.is_empty() {
        return Err(AppError::validation("规则文件名不能为空"));
    }

    Ok(Some(with_extension(file, extension)))
}

fn legacy_wfusion_rule_delete_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<Option<PathBuf>, AppError> {
    let (category_dir, extension) = match rule_type {
        RuleType::Schema => (DIR_SCHEMAS, ".wfs"),
        RuleType::Rule => (DIR_RULES, ".wfl"),
        RuleType::Scenarios => (DIR_SCENARIOS, ".wfg"),
        _ => return Ok(None),
    };

    let Some(relative) = legacy_wfusion_rule_virtual_file(file_name, extension)? else {
        return Ok(None);
    };

    Ok(Some(
        project_dir
            .join(DIR_MODELS)
            .join(category_dir)
            .join(relative),
    ))
}

fn resolve_parse_file_name(project_dir: &Path, file_name: &str) -> String {
    let trimmed = file_name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }

    let conf_dir = project_dir.join(DIR_CONF);
    if conf_dir.join(FILE_WFUSION).exists() {
        FILE_WFUSION.to_string()
    } else {
        FILE_WPARSE.to_string()
    }
}

fn wpl_rule_paths(project_dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let dir = project_dir.join(DIR_MODELS).join(DIR_WPL).join(name);
    (dir.join(FILE_WPL_PARSE), dir.join(FILE_WPL_SAMPLE))
}

/// 若文件名未包含指定扩展名则自动追加扩展名。
fn with_extension(file_name: &str, extension: &str) -> String {
    if file_name.ends_with(extension) {
        file_name.to_string()
    } else {
        format!("{file_name}{extension}")
    }
}

/// 确保目录存在，不存在则创建。
fn ensure_dir(path: impl AsRef<Path>) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(AppError::internal)
}

/// 确保给定路径的父目录存在。
fn ensure_parent_dir(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)
    } else {
        Ok(())
    }
}

fn write_empty_if_missing(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        return Ok(());
    }
    ensure_parent_dir(path)?;
    fs::write(path, "").map_err(AppError::internal)
}

/// 若 content 为 Some，则写入文件并返回写入路径；否则直接返回 None。
fn write_if_some(path: PathBuf, content: Option<String>) -> Result<Option<PathBuf>, AppError> {
    if let Some(content) = content {
        ensure_parent_dir(&path)?;
        fs::write(&path, content).map_err(AppError::internal)?;
        return Ok(Some(path));
    }

    Ok(None)
}

fn read_file_with_mtime(path: &Path) -> Result<Option<(String, SystemTime)>, AppError> {
    if !path.is_file() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", path.display(), e)))?;
    let modified = fs::metadata(path)
        .map_err(AppError::internal)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);

    Ok(Some((content, modified)))
}

fn update_last_modified(
    last_modified: &mut Option<SystemTime>,
    path: &Path,
) -> Result<(), AppError> {
    if !path.exists() {
        return Ok(());
    }

    let modified = fs::metadata(path)
        .map_err(AppError::internal)?
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH);
    if last_modified
        .map(|current| modified > current)
        .unwrap_or(true)
    {
        *last_modified = Some(modified);
    }

    Ok(())
}

fn copy_named_entry(source_root: &Path, target_root: &Path, name: &str) -> Result<(), AppError> {
    let source = source_root.join(name);
    if !source.exists() {
        return Ok(());
    }
    let target = target_root.join(name);
    copy_dir_recursive(&source, &target)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), AppError> {
    if source.is_file() {
        ensure_parent_dir(target)?;
        copy_file_preserve_permissions(source, target)?;
        return Ok(());
    }

    ensure_dir(target)?;
    for entry in fs::read_dir(source).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let file_name = entry.file_name();
        if file_name
            .to_str()
            .map(|name| name == ".git" || name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        let source_path = entry.path();
        let target_path = target.join(file_name);
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if source_path.is_file() {
            ensure_parent_dir(&target_path)?;
            copy_file_preserve_permissions(&source_path, &target_path)?;
        }
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

fn read_file_if_exists(path: &Path) -> Result<Option<String>, AppError> {
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(path)
        .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", path.display(), e)))?;
    Ok(Some(content))
}

fn is_toml_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("toml"))
        .unwrap_or(false)
}
