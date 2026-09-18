//! 项目规则与知识库文件读写辅助。

use std::{fs, path::Path, time::SystemTime};

use crate::constants::project::{DIR_KNOWLEDGE, DIR_MODELS, FILE_KNOWDB};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::RepoLayout;

use super::{KnowledgeFiles, resolve_dir_for_rule};

/// 扫描项目中某一类规则的文件名列表。
pub fn list_rule_files(layout: &RepoLayout, rule_type: RuleType) -> Result<Vec<String>, AppError> {
    if matches!(rule_type, RuleType::Knowledge) {
        return list_knowledge_dirs(layout);
    }

    let project_dir = resolve_dir_for_rule(layout, rule_type);
    if !project_dir.exists() {
        return Ok(Vec::new());
    }

    let snapshot = super::load_project_snapshot(&project_dir)?;
    let mut files: Vec<String> = snapshot
        .rules
        .into_iter()
        .filter(|rule| matches!(rule_type, RuleType::All) || rule.rule_type == rule_type)
        .map(|rule| rule.file_name)
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

/// 读取单个规则文件内容和文件 mtime。
pub fn read_rule_content(
    layout: &RepoLayout,
    rule_type: RuleType,
    file_name: &str,
) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = resolve_dir_for_rule(layout, rule_type);
    let path = super::rule_target_path(&project_dir, rule_type, file_name)?;
    super::read_file_with_mtime(&path)
}

/// 读取 WPL 的 `sample.dat` 内容和文件 mtime。
pub fn read_wpl_sample_content(
    layout: &RepoLayout,
    file_name: &str,
) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = layout.models_root.clone();
    let (_, sample_path) = super::wpl_rule_paths(&project_dir, file_name);
    super::read_file_with_mtime(&sample_path)
}

/// 直接写入单个规则文件，返回实际写入路径。
pub fn write_rule_content(
    layout: &RepoLayout,
    rule_type: RuleType,
    file_name: &str,
    content: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_dir_for_rule(layout, rule_type);
    write_rule_content_in_project_dir(&project_dir, rule_type, file_name, content)
}

/// 直接写入 WPL 的 `sample.dat`，返回实际写入路径。
pub fn write_wpl_sample_content(
    layout: &RepoLayout,
    file_name: &str,
    content: &str,
) -> Result<String, AppError> {
    let project_dir = layout.models_root.clone();
    let (_, sample_path) = super::wpl_rule_paths(&project_dir, file_name);
    super::ensure_parent_dir(&sample_path)?;
    fs::write(&sample_path, content).map_err(AppError::internal)?;
    Ok(sample_path.to_string_lossy().to_string())
}

/// 直接写入合成后的单目录项目，用于编辑态校验前覆盖当前文件内容。
pub fn write_rule_content_in_project_dir(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
    content: &str,
) -> Result<String, AppError> {
    let path = super::validation_target_path(project_dir, rule_type, file_name)?;
    super::ensure_parent_dir(&path)?;
    fs::write(&path, content).map_err(AppError::internal)?;
    Ok(path.to_string_lossy().to_string())
}

/// 在项目目录中创建一个空的规则文件，返回实际写入路径。
pub fn touch_rule_in_project(
    layout: &RepoLayout,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_dir_for_rule(layout, rule_type);

    if matches!(rule_type, RuleType::Wpl) {
        let (parse_path, sample_path) = super::wpl_rule_paths(&project_dir, file_name);
        super::write_empty_if_missing(&parse_path)?;
        super::write_empty_if_missing(&sample_path)?;
        return Ok(parse_path.to_string_lossy().to_string());
    }

    let path = super::rule_target_path(&project_dir, rule_type, file_name)?;
    super::write_empty_if_missing(&path)?;
    Ok(path.to_string_lossy().to_string())
}

/// 在项目目录中创建一个空的知识库目录和文件，返回知识库目录路径。
pub fn touch_knowledge_in_project(
    layout: &RepoLayout,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = layout.models_root.clone();
    let table_dir = project_dir
        .join(DIR_MODELS)
        .join(DIR_KNOWLEDGE)
        .join(file_name);
    super::ensure_dir(&table_dir)?;
    super::write_empty_if_missing(&table_dir.join("create.sql"))?;
    super::write_empty_if_missing(&table_dir.join("insert.sql"))?;
    super::write_empty_if_missing(&table_dir.join("data.csv"))?;
    Ok(table_dir.to_string_lossy().to_string())
}

/// 从项目目录中删除规则文件。
pub fn delete_rule_from_project(
    layout: &RepoLayout,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_dir_for_rule(layout, rule_type);

    if matches!(rule_type, RuleType::Wpl) {
        let dir = project_dir
            .join(DIR_MODELS)
            .join(crate::constants::project::DIR_WPL)
            .join(file_name);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(AppError::internal)?;
        }
        return Ok(dir.to_string_lossy().to_string());
    }

    let path = super::validation_target_path(&project_dir, rule_type, file_name)?;
    remove_path_if_exists(&path)?;

    if matches!(
        rule_type,
        RuleType::Schema | RuleType::Rule | RuleType::Scenarios
    ) && let Some(legacy_path) =
        super::legacy_wfusion_rule_delete_path(&project_dir, rule_type, file_name)?
        && legacy_path != path
    {
        remove_path_if_exists(&legacy_path)?;
    }

    cleanup_empty_parent_dirs(&project_dir, &path)?;

    Ok(path.to_string_lossy().to_string())
}

fn remove_path_if_exists(path: &Path) -> Result<(), AppError> {
    if path.exists() {
        if path.is_file() {
            fs::remove_file(path).map_err(AppError::internal)?;
        } else if path.is_dir() {
            fs::remove_dir_all(path).map_err(AppError::internal)?;
        }
    }
    Ok(())
}

fn cleanup_empty_parent_dirs(project_root: &Path, path: &Path) -> Result<(), AppError> {
    let mut current = path.parent();
    while let Some(dir) = current {
        if dir == project_root || dir == project_root.parent().unwrap_or(project_root) {
            break;
        }

        if fs::read_dir(dir)
            .map_err(AppError::internal)?
            .next()
            .is_some()
        {
            break;
        }

        fs::remove_dir(dir).map_err(AppError::internal)?;
        current = dir.parent();
    }

    Ok(())
}

/// 从项目目录中删除知识库配置。`knowdb.toml` 是全局配置，不随单个表删除。
pub fn delete_knowledge_from_project(
    layout: &RepoLayout,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = layout.models_root.clone();
    let table_dir = project_dir
        .join(DIR_MODELS)
        .join(DIR_KNOWLEDGE)
        .join(file_name);

    if table_dir.exists() {
        fs::remove_dir_all(&table_dir).map_err(AppError::internal)?;
    }

    Ok(table_dir.to_string_lossy().to_string())
}

/// 扫描 `models/knowledge/` 下的知识库表目录。
pub fn list_knowledge_dirs(layout: &RepoLayout) -> Result<Vec<String>, AppError> {
    let project_dir = layout.models_root.clone();
    let knowledge_root = project_dir.join(DIR_MODELS).join(DIR_KNOWLEDGE);
    list_knowledge_dirs_in_dir(&knowledge_root)
}

/// 从指定知识库根目录扫描表目录。
pub(super) fn list_knowledge_dirs_in_dir(knowledge_root: &Path) -> Result<Vec<String>, AppError> {
    let mut dirs = Vec::new();

    if !knowledge_root.exists() {
        return Ok(dirs);
    }

    for entry in fs::read_dir(knowledge_root).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry.path().is_dir()
            && let Some(name) = entry.file_name().to_str()
            && !name.starts_with('.')
        {
            dirs.push(name.to_string());
        }
    }

    dirs.sort();
    dirs.dedup();
    Ok(dirs)
}

/// 写入知识库表相关文件，返回知识库目录路径。
pub fn write_knowledge_files(
    layout: &RepoLayout,
    file_name: &str,
    create_sql: Option<String>,
    insert_sql: Option<String>,
    data_content: Option<String>,
) -> Result<String, AppError> {
    let project_dir = layout.models_root.clone();
    let table_dir = project_dir
        .join(DIR_MODELS)
        .join(DIR_KNOWLEDGE)
        .join(file_name);
    super::ensure_dir(&table_dir)?;

    super::write_if_some(table_dir.join("create.sql"), create_sql)?;
    super::write_if_some(table_dir.join("insert.sql"), insert_sql)?;
    super::write_if_some(table_dir.join("data.csv"), data_content)?;

    Ok(table_dir.to_string_lossy().to_string())
}

/// 读取知识库表文件，并附带全局 `knowdb.toml` 内容。
pub fn read_knowledge_files(
    layout: &RepoLayout,
    file_name: &str,
) -> Result<Option<KnowledgeFiles>, AppError> {
    let project_dir = layout.models_root.clone();
    read_knowledge_files_in_dir(&project_dir, file_name)
}

/// 从指定项目目录读取知识库表文件。
pub(super) fn read_knowledge_files_in_dir(
    project_dir: &Path,
    file_name: &str,
) -> Result<Option<KnowledgeFiles>, AppError> {
    let knowledge_root = project_dir.join(DIR_MODELS).join(DIR_KNOWLEDGE);
    let table_dir = knowledge_root.join(file_name);

    if !table_dir.is_dir() {
        return Ok(None);
    }

    let config_content = super::read_file_if_exists(&knowledge_root.join(FILE_KNOWDB))?;
    let create_path = table_dir.join("create.sql");
    let insert_path = table_dir.join("insert.sql");
    let data_path = table_dir.join("data.csv");

    let mut last_modified = None;
    super::update_last_modified(&mut last_modified, &knowledge_root.join(FILE_KNOWDB))?;
    super::update_last_modified(&mut last_modified, &create_path)?;
    super::update_last_modified(&mut last_modified, &insert_path)?;
    super::update_last_modified(&mut last_modified, &data_path)?;

    Ok(Some(KnowledgeFiles {
        file_name: file_name.to_string(),
        config_content,
        create_sql: super::read_file_if_exists(&create_path)?,
        insert_sql: super::read_file_if_exists(&insert_path)?,
        data_content: super::read_file_if_exists(&data_path)?,
        last_modified,
    }))
}

/// 读取全局 `knowdb.toml`。
pub fn read_knowdb_config(layout: &RepoLayout) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = layout.models_root.clone();
    let path = project_dir
        .join(DIR_MODELS)
        .join(DIR_KNOWLEDGE)
        .join(FILE_KNOWDB);
    super::read_file_with_mtime(&path)
}

/// 写入全局 `knowdb.toml`。
pub fn write_knowdb_config(layout: &RepoLayout, content: &str) -> Result<String, AppError> {
    let project_dir = layout.models_root.clone();
    let path = project_dir
        .join(DIR_MODELS)
        .join(DIR_KNOWLEDGE)
        .join(FILE_KNOWDB);
    super::ensure_parent_dir(&path)?;
    fs::write(&path, content).map_err(AppError::internal)?;
    Ok(path.to_string_lossy().to_string())
}
