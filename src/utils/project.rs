//! 项目文件 I/O 模块。
//!
//! 负责 `project_root` 中规则、配置、知识库文件的读写、扫描和快照加载，
//! 是文件系统与业务层之间的桥梁。

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::common::{WPL_PARSE_FILENAME, WPL_SAMPLE_FILENAME};

#[derive(Debug, Clone)]
pub struct ProjectRuleFile {
    pub rule_type: RuleType,
    pub file_name: String,
    pub content: Option<String>,
    pub sample_content: Option<String>,
    pub display_name: Option<String>,
    pub file_size: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct KnowledgeFiles {
    pub file_name: String,
    pub config_content: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data_content: Option<String>,
    pub last_modified: Option<SystemTime>,
}

#[derive(Default)]
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

/// 统一将配置中的 project_root 解析成工作区下的稳定路径，避免相对路径受当前工作目录影响。
pub fn resolve_project_root(project_root: &str) -> PathBuf {
    let path = PathBuf::from(project_root);
    if path.is_absolute() {
        path
    } else {
        Setting::workspace_root().join(path)
    }
}

/// 扫描 project_root 中某一类规则的文件名列表。
pub fn list_rule_files(project_root: &str, rule_type: RuleType) -> Result<Vec<String>, AppError> {
    if matches!(rule_type, RuleType::Knowledge) {
        return list_knowledge_dirs(project_root);
    }

    let project_dir = resolve_project_root(project_root);
    if !project_dir.exists() {
        return Ok(Vec::new());
    }

    let snapshot = load_project_snapshot(&project_dir)?;
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
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = resolve_project_root(project_root);
    let path = rule_target_path(&project_dir, rule_type, file_name)?;
    read_file_with_mtime(&path)
}

/// 读取 WPL 的 sample.dat 内容和文件 mtime。
pub fn read_wpl_sample_content(
    project_root: &str,
    file_name: &str,
) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = resolve_project_root(project_root);
    let (_, sample_path) = wpl_rule_paths(&project_dir, file_name);
    read_file_with_mtime(&sample_path)
}

/// 直接写入单个规则文件，返回实际写入路径。
pub fn write_rule_content(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
    content: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let path = rule_target_path(&project_dir, rule_type, file_name)?;
    ensure_parent_dir(&path)?;
    fs::write(&path, content).map_err(AppError::internal)?;
    Ok(path.to_string_lossy().to_string())
}

/// 直接写入 WPL 的 sample.dat，返回实际写入路径。
pub fn write_wpl_sample_content(
    project_root: &str,
    file_name: &str,
    content: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let (_, sample_path) = wpl_rule_paths(&project_dir, file_name);
    ensure_parent_dir(&sample_path)?;
    fs::write(&sample_path, content).map_err(AppError::internal)?;
    Ok(sample_path.to_string_lossy().to_string())
}

/// 在项目目录中创建一个空的规则文件，返回实际写入路径。
pub fn touch_rule_in_project(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);

    if matches!(rule_type, RuleType::Wpl) {
        let (parse_path, sample_path) = wpl_rule_paths(&project_dir, file_name);
        write_empty_if_missing(&parse_path)?;
        write_empty_if_missing(&sample_path)?;
        return Ok(parse_path.to_string_lossy().to_string());
    }

    let path = rule_target_path(&project_dir, rule_type, file_name)?;
    write_empty_if_missing(&path)?;
    Ok(path.to_string_lossy().to_string())
}

/// 在项目目录中创建一个空的知识库目录和文件，返回知识库目录路径。
pub fn touch_knowledge_in_project(project_root: &str, file_name: &str) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let table_dir = project_dir.join("models").join("knowledge").join(file_name);
    ensure_dir(&table_dir)?;
    write_empty_if_missing(&table_dir.join("create.sql"))?;
    write_empty_if_missing(&table_dir.join("insert.sql"))?;
    write_empty_if_missing(&table_dir.join("data.csv"))?;
    Ok(table_dir.to_string_lossy().to_string())
}

/// 从项目目录中删除规则文件。
pub fn delete_rule_from_project(
    project_root: &str,
    rule_type: RuleType,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);

    if matches!(rule_type, RuleType::Wpl) {
        let dir = project_dir.join("models").join("wpl").join(file_name);
        if dir.exists() {
            fs::remove_dir_all(&dir).map_err(AppError::internal)?;
        }
        return Ok(dir.to_string_lossy().to_string());
    }

    let path = rule_target_path(&project_dir, rule_type, file_name)?;

    if path.exists() {
        if path.is_file() {
            fs::remove_file(&path).map_err(AppError::internal)?;
        } else if path.is_dir() {
            fs::remove_dir_all(&path).map_err(AppError::internal)?;
        }
    }

    Ok(path.to_string_lossy().to_string())
}

/// 从项目目录中删除知识库配置。knowdb.toml 是全局配置，不随单个表删除。
pub fn delete_knowledge_from_project(
    project_root: &str,
    file_name: &str,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let table_dir = project_dir.join("models").join("knowledge").join(file_name);

    if table_dir.exists() {
        fs::remove_dir_all(&table_dir).map_err(AppError::internal)?;
    }

    Ok(table_dir.to_string_lossy().to_string())
}

/// 扫描 models/knowledge/ 下的知识库表目录。
pub fn list_knowledge_dirs(project_root: &str) -> Result<Vec<String>, AppError> {
    let project_dir = resolve_project_root(project_root);
    let knowledge_root = project_dir.join("models").join("knowledge");
    let mut dirs = Vec::new();

    if !knowledge_root.exists() {
        return Ok(dirs);
    }

    for entry in fs::read_dir(&knowledge_root).map_err(AppError::internal)? {
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
    project_root: &str,
    file_name: &str,
    create_sql: Option<String>,
    insert_sql: Option<String>,
    data_content: Option<String>,
) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let table_dir = project_dir.join("models").join("knowledge").join(file_name);
    ensure_dir(&table_dir)?;

    write_if_some(table_dir.join("create.sql"), create_sql)?;
    write_if_some(table_dir.join("insert.sql"), insert_sql)?;
    write_if_some(table_dir.join("data.csv"), data_content)?;

    Ok(table_dir.to_string_lossy().to_string())
}

/// 读取知识库表文件，并附带全局 knowdb.toml 内容。
pub fn read_knowledge_files(
    project_root: &str,
    file_name: &str,
) -> Result<Option<KnowledgeFiles>, AppError> {
    let project_dir = resolve_project_root(project_root);
    let knowledge_root = project_dir.join("models").join("knowledge");
    let table_dir = knowledge_root.join(file_name);

    if !table_dir.is_dir() {
        return Ok(None);
    }

    let config_content = read_file_if_exists(&knowledge_root.join("knowdb.toml"))?;
    let create_path = table_dir.join("create.sql");
    let insert_path = table_dir.join("insert.sql");
    let data_path = table_dir.join("data.csv");

    let mut last_modified = None;
    update_last_modified(&mut last_modified, &knowledge_root.join("knowdb.toml"))?;
    update_last_modified(&mut last_modified, &create_path)?;
    update_last_modified(&mut last_modified, &insert_path)?;
    update_last_modified(&mut last_modified, &data_path)?;

    Ok(Some(KnowledgeFiles {
        file_name: file_name.to_string(),
        config_content,
        create_sql: read_file_if_exists(&create_path)?,
        insert_sql: read_file_if_exists(&insert_path)?,
        data_content: read_file_if_exists(&data_path)?,
        last_modified,
    }))
}

/// 读取全局 knowdb.toml。
pub fn read_knowdb_config(project_root: &str) -> Result<Option<(String, SystemTime)>, AppError> {
    let project_dir = resolve_project_root(project_root);
    let path = project_dir
        .join("models")
        .join("knowledge")
        .join("knowdb.toml");
    read_file_with_mtime(&path)
}

/// 写入全局 knowdb.toml。
pub fn write_knowdb_config(project_root: &str, content: &str) -> Result<String, AppError> {
    let project_dir = resolve_project_root(project_root);
    let path = project_dir
        .join("models")
        .join("knowledge")
        .join("knowdb.toml");
    ensure_parent_dir(&path)?;
    fs::write(&path, content).map_err(AppError::internal)?;
    Ok(path.to_string_lossy().to_string())
}

/// 根据 rule_type 和 file_name 计算规则配置文件的目标路径。
fn rule_target_path(
    project_dir: &Path,
    rule_type: RuleType,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    match rule_type {
        RuleType::Parse => Ok(project_dir.join("conf").join("wparse.toml")),
        RuleType::Wpgen => Ok(project_dir.join("conf").join("wpgen.toml")),
        RuleType::SourceConnect => connector_rule_path(project_dir, "source.d", file_name),
        RuleType::SinkConnect => connector_rule_path(project_dir, "sink.d", file_name),
        RuleType::Source => Ok(project_dir.join("topology").join("sources").join(file_name)),
        RuleType::Sink => Ok(project_dir.join("topology").join("sinks").join(file_name)),
        RuleType::Wpl => {
            let (parse_path, _) = wpl_rule_paths(project_dir, file_name);
            Ok(parse_path)
        }
        RuleType::Oml => Ok(project_dir
            .join("models")
            .join("oml")
            .join(file_name)
            .join("adm.oml")),
        RuleType::Knowledge => Err(AppError::validation("知识库配置请使用 knowledge 文件接口")),
        RuleType::All => Err(AppError::validation("all 类型不能映射到单个规则文件")),
    }
}

/// 计算 connectors/<folder>/<file_name>.toml 形式的路径。
fn connector_rule_path(
    project_dir: &Path,
    folder: &str,
    file_name: &str,
) -> Result<PathBuf, AppError> {
    Ok(project_dir
        .join("connectors")
        .join(folder)
        .join(with_extension(file_name, ".toml")))
}

fn wpl_rule_paths(project_dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let dir = project_dir.join("models").join("wpl").join(name);
    (dir.join(WPL_PARSE_FILENAME), dir.join(WPL_SAMPLE_FILENAME))
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

/// 从项目目录加载规则与知识库快照。
pub fn load_project_snapshot(project_root: &Path) -> Result<ProjectSnapshot, AppError> {
    if !project_root.exists() {
        return Err(AppError::validation(format!(
            "project_root 不存在: {}",
            project_root.display()
        )));
    }

    let mut snapshot = ProjectSnapshot::default();

    load_parse_and_wpgen(project_root, &mut snapshot)?;
    load_connector_rules(project_root, &mut snapshot)?;
    load_topology_rules(project_root, &mut snapshot)?;
    load_wpl_rules(project_root, &mut snapshot)?;
    load_oml_rules(project_root, &mut snapshot)?;
    load_knowledge_tables(project_root, &mut snapshot)?;

    Ok(snapshot)
}

fn load_parse_and_wpgen(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let conf_dir = project_root.join("conf");

    let wparse_path = conf_dir.join("wparse.toml");
    if let Some(content) = read_file_if_exists(&wparse_path)? {
        snapshot.add_rule(
            RuleType::Parse,
            "wparse.toml".to_string(),
            Some(content),
            None,
            None,
        );
    }

    let wpgen_path = conf_dir.join("wpgen.toml");
    if let Some(content) = read_file_if_exists(&wpgen_path)? {
        snapshot.add_rule(
            RuleType::Wpgen,
            "wpgen.toml".to_string(),
            Some(content),
            None,
            None,
        );
    }

    Ok(())
}

fn load_connector_rules(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let source_dir = project_root.join("connectors").join("source.d");
    if source_dir.exists() {
        for entry in fs::read_dir(&source_dir).map_err(AppError::internal)? {
            let entry = entry.map_err(AppError::internal)?;
            let path = entry.path();
            if path.is_file() && is_toml_file(&path) {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let content = fs::read_to_string(&path).map_err(|e| {
                    AppError::internal(format!("读取 {} 失败: {}", path.display(), e))
                })?;
                snapshot.add_rule(
                    RuleType::SourceConnect,
                    file_name,
                    Some(content),
                    None,
                    None,
                );
            }
        }
    }

    let sink_dir = project_root.join("connectors").join("sink.d");
    if sink_dir.exists() {
        for entry in fs::read_dir(&sink_dir).map_err(AppError::internal)? {
            let entry = entry.map_err(AppError::internal)?;
            let path = entry.path();
            if path.is_file() && is_toml_file(&path) {
                let file_name = entry.file_name().to_string_lossy().to_string();
                let content = fs::read_to_string(&path).map_err(|e| {
                    AppError::internal(format!("读取 {} 失败: {}", path.display(), e))
                })?;
                snapshot.add_rule(RuleType::SinkConnect, file_name, Some(content), None, None);
            }
        }
    }

    Ok(())
}

fn load_topology_rules(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let sources_dir = project_root.join("topology").join("sources");
    for (relative, file_path) in collect_relative_files(&sources_dir)? {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", file_path.display(), e)))?;
        snapshot.add_rule(RuleType::Source, relative, Some(content), None, None);
    }

    let sinks_dir = project_root.join("topology").join("sinks");
    for (relative, file_path) in collect_relative_files(&sinks_dir)? {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", file_path.display(), e)))?;
        snapshot.add_rule(RuleType::Sink, relative, Some(content), None, None);
    }

    Ok(())
}

fn load_wpl_rules(project_root: &Path, snapshot: &mut ProjectSnapshot) -> Result<(), AppError> {
    let wpl_dir = project_root.join("models").join("wpl");
    if !wpl_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&wpl_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if entry
            .file_name()
            .to_str()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        let rule_name = entry.file_name().to_string_lossy().to_string();
        let parse_path = path.join(WPL_PARSE_FILENAME);
        if !parse_path.exists() {
            snapshot.failed_files += 1;
            snapshot.warnings.push(format!(
                "WPL 规则 {} 缺少 {}",
                rule_name, WPL_PARSE_FILENAME
            ));
            continue;
        }

        let parse_content = fs::read_to_string(&parse_path).map_err(|e| {
            AppError::internal(format!("读取 {} 失败: {}", parse_path.display(), e))
        })?;
        let sample_path = path.join(WPL_SAMPLE_FILENAME);
        let sample_content = read_file_if_exists(&sample_path)?;

        snapshot.add_rule(
            RuleType::Wpl,
            rule_name,
            Some(parse_content),
            sample_content,
            None,
        );
    }

    Ok(())
}

fn load_oml_rules(project_root: &Path, snapshot: &mut ProjectSnapshot) -> Result<(), AppError> {
    let oml_dir = project_root.join("models").join("oml");
    if !oml_dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&oml_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry
            .file_name()
            .to_str()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let rule_name = entry.file_name().to_string_lossy().to_string();
        let found = load_oml_rule_dir(&path, &rule_name, snapshot)?;
        if !found {
            snapshot.failed_files += 1;
            snapshot
                .warnings
                .push(format!("OML 规则 {} 缺少 adm.oml", rule_name));
        }
    }

    Ok(())
}

fn load_oml_rule_dir(
    current_dir: &Path,
    relative_name: &str,
    snapshot: &mut ProjectSnapshot,
) -> Result<bool, AppError> {
    let mut found_rule = false;
    let adm_path = current_dir.join("adm.oml");
    if adm_path.exists() {
        let content = fs::read_to_string(&adm_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", adm_path.display(), e)))?;
        snapshot.add_rule(
            RuleType::Oml,
            relative_name.to_string(),
            Some(content),
            None,
            None,
        );
        found_rule = true;
    }

    for entry in fs::read_dir(current_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry
            .file_name()
            .to_str()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
        {
            continue;
        }

        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let child_name = entry.file_name().to_string_lossy().to_string();
        let relative = format!("{}/{}", relative_name, child_name);
        if load_oml_rule_dir(&path, &relative, snapshot)? {
            found_rule = true;
        }
    }

    Ok(found_rule)
}

fn load_knowledge_tables(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let project_root = project_root.to_string_lossy().to_string();
    for table_name in list_knowledge_dirs(&project_root)? {
        let Some(config) = read_knowledge_files(&project_root, &table_name)? else {
            continue;
        };

        if config.create_sql.is_none()
            && config.insert_sql.is_none()
            && config.data_content.is_none()
        {
            snapshot.failed_files += 1;
            snapshot.warnings.push(format!(
                "知识库 {} 未找到 create.sql/insert.sql/data.csv，已跳过",
                table_name
            ));
            continue;
        }

        snapshot.add_knowledge(config);
    }

    Ok(())
}

fn collect_relative_files(dir: &Path) -> Result<Vec<(String, PathBuf)>, AppError> {
    fn walk(
        base: &Path,
        current: &Path,
        result: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), AppError> {
        for entry in fs::read_dir(current).map_err(AppError::internal)? {
            let entry = entry.map_err(AppError::internal)?;
            let file_name = entry.file_name();
            if file_name
                .to_str()
                .map(|name| name.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }

            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, result)?;
            } else if path.is_file()
                && let Ok(relative) = path.strip_prefix(base)
            {
                let rel = relative
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                result.push((rel, path));
            }
        }
        Ok(())
    }

    let mut result = Vec::new();
    if !dir.exists() {
        return Ok(result);
    }

    walk(dir, dir, &mut result)?;
    Ok(result)
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
