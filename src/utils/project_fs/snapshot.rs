//! 项目快照加载辅助。

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::constants::project::{
    DIR_CONF, DIR_CONNECTORS, DIR_KNOWLEDGE, DIR_MODELS, DIR_OML, DIR_RULES, DIR_SCENARIOS,
    DIR_SCHEMAS, DIR_SINK_D, DIR_SINKS, DIR_SOURCE_D, DIR_SOURCES, DIR_TOPOLOGY, DIR_WPL,
    FILE_OML_ADM, FILE_WFUSION, FILE_WINDOWS, FILE_WPARSE, FILE_WPGEN, FILE_WPL_PARSE,
    FILE_WPL_SAMPLE,
};
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::RepoLayout;

use super::ProjectSnapshot;

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
    load_wfusion_windows(project_root, &mut snapshot)?;
    load_wfusion_schema_rules(project_root, &mut snapshot)?;
    load_wfusion_rules(project_root, &mut snapshot)?;
    load_wfusion_scenarios(project_root, &mut snapshot)?;
    load_knowledge_tables(project_root, &mut snapshot)?;
    Ok(snapshot)
}

/// 从双仓库布局加载规则与知识库快照，并在内存中合并成统一视图。
pub fn load_project_snapshot_from_repo_layout(
    layout: &RepoLayout,
) -> Result<ProjectSnapshot, AppError> {
    let mut snapshot = ProjectSnapshot::default();
    load_parse_and_wpgen(&layout.infra_root, &mut snapshot)?;
    load_connector_rules(&layout.connectors_root, &mut snapshot)?;
    load_topology_rules(&layout.infra_root, &mut snapshot)?;
    load_wpl_rules(&layout.models_root, &mut snapshot)?;
    load_oml_rules(&layout.models_root, &mut snapshot)?;
    load_wfusion_windows(&layout.models_root, &mut snapshot)?;
    load_wfusion_schema_rules(&layout.models_root, &mut snapshot)?;
    load_wfusion_rules(&layout.models_root, &mut snapshot)?;
    load_wfusion_scenarios(&layout.models_root, &mut snapshot)?;
    load_knowledge_tables(&layout.models_root, &mut snapshot)?;
    Ok(snapshot)
}

fn load_parse_and_wpgen(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let conf_dir = project_root.join(DIR_CONF);

    let wparse_path = conf_dir.join(FILE_WPARSE);
    if let Some(content) = super::read_file_if_exists(&wparse_path)? {
        snapshot.add_rule(
            RuleType::Parse,
            FILE_WPARSE.to_string(),
            Some(content),
            None,
            None,
        );
    }

    let wfusion_path = conf_dir.join(FILE_WFUSION);
    if let Some(content) = super::read_file_if_exists(&wfusion_path)? {
        snapshot.add_rule(
            RuleType::Parse,
            FILE_WFUSION.to_string(),
            Some(content),
            None,
            None,
        );
    }

    let wpgen_path = conf_dir.join(FILE_WPGEN);
    if let Some(content) = super::read_file_if_exists(&wpgen_path)? {
        snapshot.add_rule(
            RuleType::Wpgen,
            FILE_WPGEN.to_string(),
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
    let source_dir = project_root.join(DIR_CONNECTORS).join(DIR_SOURCE_D);
    if source_dir.exists() {
        for entry in fs::read_dir(&source_dir).map_err(AppError::internal)? {
            let entry = entry.map_err(AppError::internal)?;
            let path = entry.path();
            if path.is_file() && super::is_toml_file(&path) {
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

    let sink_dir = project_root.join(DIR_CONNECTORS).join(DIR_SINK_D);
    if sink_dir.exists() {
        for entry in fs::read_dir(&sink_dir).map_err(AppError::internal)? {
            let entry = entry.map_err(AppError::internal)?;
            let path = entry.path();
            if path.is_file() && super::is_toml_file(&path) {
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
    let sources_dir = project_root.join(DIR_TOPOLOGY).join(DIR_SOURCES);
    for (relative, file_path) in collect_relative_files(&sources_dir)? {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", file_path.display(), e)))?;
        snapshot.add_rule(RuleType::Source, relative, Some(content), None, None);
    }

    let sinks_dir = project_root.join(DIR_TOPOLOGY).join(DIR_SINKS);
    for (relative, file_path) in collect_relative_files(&sinks_dir)? {
        let content = fs::read_to_string(&file_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", file_path.display(), e)))?;
        snapshot.add_rule(RuleType::Sink, relative, Some(content), None, None);
    }

    Ok(())
}

fn load_wpl_rules(project_root: &Path, snapshot: &mut ProjectSnapshot) -> Result<(), AppError> {
    let wpl_dir = project_root.join(DIR_MODELS).join(DIR_WPL);
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
        let parse_path = path.join(FILE_WPL_PARSE);
        if !parse_path.exists() {
            snapshot.failed_files += 1;
            snapshot
                .warnings
                .push(format!("WPL 规则 {} 缺少 {}", rule_name, FILE_WPL_PARSE));
            continue;
        }

        let parse_content = fs::read_to_string(&parse_path).map_err(|e| {
            AppError::internal(format!("读取 {} 失败: {}", parse_path.display(), e))
        })?;
        let sample_path = path.join(FILE_WPL_SAMPLE);
        let sample_content = super::read_file_if_exists(&sample_path)?;

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
    let oml_dir = project_root.join(DIR_MODELS).join(DIR_OML);
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
                .push(format!("OML 规则 {} 缺少 {}", rule_name, FILE_OML_ADM));
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
    let adm_path = current_dir.join(FILE_OML_ADM);
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
    let knowledge_root = project_root.join(DIR_MODELS).join(DIR_KNOWLEDGE);
    for table_name in super::files::list_knowledge_dirs_in_dir(&knowledge_root)? {
        let Some(config) = super::files::read_knowledge_files_in_dir(project_root, &table_name)?
        else {
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

fn load_wfusion_schema_rules(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    load_named_model_rules(
        project_root,
        DIR_SCHEMAS,
        ".wfs",
        RuleType::Schema,
        snapshot,
    )
}

fn load_wfusion_windows(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let windows_path = project_root.join(DIR_MODELS).join(FILE_WINDOWS);
    if let Some(content) = super::read_file_if_exists(&windows_path)? {
        snapshot.add_rule(
            RuleType::Windows,
            FILE_WINDOWS.to_string(),
            Some(content),
            None,
            None,
        );
    }
    Ok(())
}

fn load_wfusion_rules(project_root: &Path, snapshot: &mut ProjectSnapshot) -> Result<(), AppError> {
    load_named_model_rules(project_root, DIR_RULES, ".wfl", RuleType::Rule, snapshot)
}

fn load_wfusion_scenarios(
    project_root: &Path,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    load_named_model_rules(
        project_root,
        DIR_SCENARIOS,
        ".wfg",
        RuleType::Scenarios,
        snapshot,
    )
}

fn load_named_model_rules(
    project_root: &Path,
    category_dir: &str,
    extension: &str,
    rule_type: RuleType,
    snapshot: &mut ProjectSnapshot,
) -> Result<(), AppError> {
    let root = project_root.join(DIR_MODELS).join(category_dir);
    for (relative, file_path) in collect_relative_files(&root)? {
        if !relative.ends_with(extension) {
            continue;
        }

        let content = fs::read_to_string(&file_path)
            .map_err(|e| AppError::internal(format!("读取 {} 失败: {}", file_path.display(), e)))?;
        snapshot.add_rule(rule_type, relative, Some(content), None, None);
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
