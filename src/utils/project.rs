use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::db::{
    KnowledgeConfig, RuleConfig, RuleType, find_all_knowledge_configs,
    find_knowledge_config_by_file_name, find_rule_by_type_and_name, find_rules_by_type,
};
use crate::error::AppError;
use crate::utils::constants::{WPL_PARSE_FILENAME, WPL_SAMPLE_FILENAME};
use sea_orm::DatabaseConnection;

/// 从数据库导出配置到 project_root 目录（全局共享，无连接隔离）
pub async fn export_project_from_db(
    _db: &DatabaseConnection,
    project_root: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);

    export_project_to_dir(&project_dir).await?;

    Ok(project_dir.to_string_lossy().to_string())
}

/// 增量导出单个规则配置（包括连接配置和 WPL/OML），返回实际写入的文件路径
pub async fn export_rule_to_project(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);

    let rule = find_rule_by_type_and_name(rule_type.as_ref(), file_name)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::not_found("规则配置不存在"))?;

    let RuleConfig {
        file_name,
        content,
        sample_content,
        ..
    } = rule;

    if matches!(rule_type, RuleType::Wpl) {
        let parse_content =
            content.ok_or_else(|| AppError::validation("规则内容为空，无法导出"))?;
        let (parse_path, sample_path) = wpl_rule_paths(&project_dir, &file_name)?;

        ensure_parent_dir(&parse_path)?;
        fs::write(&parse_path, &parse_content).map_err(AppError::internal)?;

        ensure_parent_dir(&sample_path)?;
        let sample_data = sample_content.unwrap_or_default();
        fs::write(&sample_path, sample_data).map_err(AppError::internal)?;

        return Ok(parse_path.to_string_lossy().to_string());
    }

    let content = content.ok_or_else(|| AppError::validation("规则内容为空，无法导出"))?;

    let path = rule_target_path(&project_dir, rule_type, &file_name)?;
    fs::write(&path, content).map_err(AppError::internal)?;

    Ok(path.to_string_lossy().to_string())
}

/// 在项目目录中创建一个空的规则文件，返回实际写入路径
pub fn touch_rule_in_project(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);

    if matches!(rule_type, RuleType::Wpl) {
        let (parse_path, sample_path) = wpl_rule_paths(&project_dir, file_name)?;
        ensure_parent_dir(&parse_path)?;
        fs::write(&parse_path, "").map_err(AppError::internal)?;
        ensure_parent_dir(&sample_path)?;
        fs::write(&sample_path, "").map_err(AppError::internal)?;
        return Ok(parse_path.to_string_lossy().to_string());
    }

    let path = rule_target_path(&project_dir, rule_type, file_name)?;

    ensure_parent_dir(&path)?;
    fs::write(&path, "").map_err(AppError::internal)?;

    Ok(path.to_string_lossy().to_string())
}

/// 增量导出单个知识库配置到 models/knowledge 目录，并维护 knowdb.toml，返回主文件路径
pub async fn export_knowledge_to_project(
    project_root: &str,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);
    let knowledge_root = project_dir.join("models").join("knowledge");
    let table_dir = knowledge_root.join(file_name);

    let config = find_knowledge_config_by_file_name(file_name)
        .await
        .map_err(AppError::internal)?
        .ok_or_else(|| AppError::not_found("知识库配置不存在"))?;

    ensure_dir(&table_dir)?;
    let mut primary_path: Option<PathBuf> = None;

    let KnowledgeConfig {
        create_sql,
        insert_sql,
        data_content,
        ..
    } = config;

    if let Some(path) = write_if_some(table_dir.join("create.sql"), create_sql)? {
        primary_path = Some(path);
    }
    if let Some(path) = write_if_some(table_dir.join("insert.sql"), insert_sql)? {
        primary_path.get_or_insert(path);
    }
    if let Some(path) = write_if_some(table_dir.join("data.csv"), data_content)? {
        primary_path.get_or_insert(path);
    }

    if let Some(path) = rebuild_knowdb_file(&knowledge_root).await? {
        primary_path.get_or_insert(path);
    }

    let result_path = primary_path.unwrap_or(table_dir);
    Ok(result_path.to_string_lossy().to_string())
}

/// 从项目目录中删除规则文件
pub fn delete_rule_from_project(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);

    if matches!(rule_type, RuleType::Wpl) {
        let dir = project_dir.join("models").join("wpl").join(file_name);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(AppError::internal)?;
        }
        return Ok(dir.to_string_lossy().to_string());
    }

    let path = rule_target_path(&project_dir, rule_type, file_name)?;

    // 如果文件存在则删除
    if path.exists() {
        if path.is_file() {
            fs::remove_file(&path).map_err(AppError::internal)?;
        } else if path.is_dir() {
            // 对于目录（如 oml/wpl），删除整个目录
            fs::remove_dir_all(&path).map_err(AppError::internal)?;
        }
    }

    Ok(path.to_string_lossy().to_string())
}

/// 从项目目录中删除知识库配置
pub async fn delete_knowledge_from_project(
    project_root: &str,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = PathBuf::from(project_root);
    let knowledge_root = project_dir.join("models").join("knowledge");
    let table_dir = knowledge_root.join(file_name);

    // 删除知识库目录
    if table_dir.exists() {
        fs::remove_dir_all(&table_dir).map_err(AppError::internal)?;
    }

    // 重建 knowdb.toml
    rebuild_knowdb_file(&knowledge_root).await?;

    Ok(table_dir.to_string_lossy().to_string())
}

/// 内部方法：从数据库全量导出所有规则与知识库到给定目录（全局共享）
async fn export_project_to_dir(project_dir: &Path) -> Result<(), AppError> {
    ensure_dir(project_dir)?;

    export_rules(project_dir, RuleType::Parse).await?;
    export_rules(project_dir, RuleType::Wpgen).await?;
    export_rules(project_dir, RuleType::SourceConnect).await?;
    export_rules(project_dir, RuleType::SinkConnect).await?;
    export_rules(project_dir, RuleType::Oml).await?;
    export_rules(project_dir, RuleType::Wpl).await?;
    export_rules(project_dir, RuleType::Source).await?;
    export_rules(project_dir, RuleType::Sink).await?;

    export_all_knowledge_configs(project_dir).await?;

    Ok(())
}

/// 内部方法：按 rule_type 批量导出所有激活规则
async fn export_rules(project_dir: &Path, rule_type: RuleType) -> Result<(), AppError> {
    let rules = find_rules_by_type(rule_type.as_ref())
        .await
        .map_err(AppError::internal)?;

    for rule in rules {
        write_rule_entry(project_dir, rule_type, rule)?;
    }

    Ok(())
}

/// 内部方法：将单条规则配置写入到对应的导出文件
fn write_rule_entry(
    project_dir: &Path,
    rule_type: RuleType,
    rule: RuleConfig,
) -> Result<(), AppError> {
    let RuleConfig {
        file_name,
        content,
        sample_content,
        ..
    } = rule;

    // parse 和 wpgen 只处理特定文件名
    if matches!(rule_type, RuleType::Parse) && file_name != "wparse.toml" {
        return Ok(());
    }
    if matches!(rule_type, RuleType::Wpgen) && file_name != "wpgen.toml" {
        return Ok(());
    }

    if matches!(rule_type, RuleType::Wpl) {
        let parse_content =
            content.ok_or_else(|| AppError::validation("WPL 规则缺少 parse 内容"))?;
        let (parse_path, sample_path) = wpl_rule_paths(project_dir, &file_name)?;
        ensure_parent_dir(&parse_path)?;
        fs::write(&parse_path, parse_content).map_err(AppError::internal)?;
        ensure_parent_dir(&sample_path)?;
        fs::write(&sample_path, sample_content.unwrap_or_default()).map_err(AppError::internal)?;
        return Ok(());
    }

    let path = rule_target_path(project_dir, rule_type, &file_name)?;
    write_if_some(path, content)?;

    Ok(())
}

/// 内部方法：导出全部知识库配置并生成 knowdb.toml
async fn export_all_knowledge_configs(project_dir: &Path) -> Result<(), AppError> {
    let knowledge_configs = find_all_knowledge_configs()
        .await
        .map_err(AppError::internal)?;

    let knowledge_root = project_dir.join("models").join("knowledge");
    ensure_dir(&knowledge_root)?;

    let mut knowdb_content = String::new();
    for config in knowledge_configs {
        write_knowledge_config(&knowledge_root, config, &mut knowdb_content)?;
    }

    if !knowdb_content.is_empty() {
        fs::write(knowledge_root.join("knowdb.toml"), knowdb_content)
            .map_err(AppError::internal)?;
    }

    Ok(())
}

/// 内部方法：导出单个知识库表的文件并拼接 knowdb.toml 内容
fn write_knowledge_config(
    knowledge_root: &Path,
    config: KnowledgeConfig,
    knowdb_content: &mut String,
) -> Result<(), AppError> {
    let KnowledgeConfig {
        file_name,
        create_sql,
        insert_sql,
        data_content,
        config_content,
        ..
    } = config;

    let table_dir = knowledge_root.join(&file_name);
    ensure_dir(&table_dir)?;

    write_if_some(table_dir.join("create.sql"), create_sql)?;
    write_if_some(table_dir.join("insert.sql"), insert_sql)?;
    write_if_some(table_dir.join("data.csv"), data_content)?;

    if let Some(content) = config_content {
        knowdb_content.push_str(&content);
        knowdb_content.push_str("\n\n");
    }

    Ok(())
}

/// 内部方法：根据当前知识库配置重新构建 knowdb.toml 文件
async fn rebuild_knowdb_file(knowledge_root: &Path) -> Result<Option<PathBuf>, AppError> {
    let knowledge_configs = find_all_knowledge_configs()
        .await
        .map_err(AppError::internal)?;

    let mut knowdb_content = String::new();
    for config in knowledge_configs {
        if let Some(content) = config.config_content {
            knowdb_content.push_str(&content);
            knowdb_content.push_str("\n\n");
        }
    }

    if knowdb_content.is_empty() {
        return Ok(None);
    }

    let knowdb_path = knowledge_root.join("knowdb.toml");
    ensure_parent_dir(&knowdb_path)?;
    fs::write(&knowdb_path, knowdb_content).map_err(AppError::internal)?;

    Ok(Some(knowdb_path))
}

/// 根据 rule_type 和 file_name 计算规则配置导出文件的目标路径
fn rule_target_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    match rule_type {
        RuleType::Parse => {
            let path = project_dir.join("conf").join("wparse.toml");
            ensure_parent_dir(&path)?;
            Ok(path)
        }
        RuleType::Wpgen => {
            let path = project_dir.join("conf").join("wpgen.toml");
            ensure_parent_dir(&path)?;
            Ok(path)
        }
        RuleType::SourceConnect => connector_rule_path(project_dir, "source.d", file_name),
        RuleType::SinkConnect => connector_rule_path(project_dir, "sink.d", file_name),
        RuleType::Source => topology_rule_path(project_dir, "sources", file_name),
        RuleType::Sink => topology_rule_path(project_dir, "sinks", file_name),
        RuleType::Wpl => nested_model_rule_path(project_dir, "wpl", file_name, "parse.wpl"),
        RuleType::Oml => nested_model_rule_path(project_dir, "oml", file_name, "adm.oml"),
        RuleType::Knowledge => Err(AppError::validation(
            "知识库配置请使用 export_knowledge_to_project",
        )),
        _ => Ok(PathBuf::new()),
    }
}

/// 计算 connectors/<folder>/<file_name>.toml 形式的导出路径
fn connector_rule_path(
    project_dir: &Path,
    folder: &str,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    let dir = project_dir.join("connectors").join(folder);
    ensure_dir(&dir)?;
    Ok(dir.join(with_extension(file_name, ".toml")))
}

/// 计算 topology/<folder>/<file_name> 形式的导出路径
fn topology_rule_path(
    project_dir: &Path,
    folder: &str,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    let path = project_dir.join("topology").join(folder).join(file_name);
    ensure_parent_dir(&path)?;
    Ok(path)
}

/// 计算 models/<folder>/<name>/<file_name> 形式的嵌套导出路径
fn nested_model_rule_path(
    project_dir: &Path,
    folder: &str,
    name: &str,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    let dir = project_dir.join("models").join(folder).join(name);
    ensure_dir(&dir)?;
    Ok(dir.join(file_name))
}

fn wpl_rule_paths(project_dir: &Path, name: &str) -> Result<(PathBuf, PathBuf), AppError> {
    let dir = project_dir.join("models").join("wpl").join(name);
    ensure_dir(&dir)?;
    Ok((dir.join(WPL_PARSE_FILENAME), dir.join(WPL_SAMPLE_FILENAME)))
}

/// 若文件名未包含指定扩展名则自动追加扩展名
fn with_extension(file_name: &str, extension: &str) -> String {
    if file_name.ends_with(extension) {
        file_name.to_string()
    } else {
        format!("{file_name}{extension}")
    }
}

/// 确保目录存在，不存在则创建
fn ensure_dir(path: impl AsRef<Path>) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(AppError::internal)
}

/// 确保给定路径的父目录存在
fn ensure_parent_dir(path: &Path) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)
    } else {
        Ok(())
    }
}

/// 若 content 为 Some，则写入文件并返回写入路径；否则直接返回 None
fn write_if_some(path: PathBuf, content: Option<String>) -> Result<Option<PathBuf>, AppError> {
    if let Some(content) = content {
        ensure_parent_dir(&path)?;
        fs::write(&path, content).map_err(AppError::internal)?;
        return Ok(Some(path));
    }

    Ok(None)
}
