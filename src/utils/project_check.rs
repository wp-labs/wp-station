//! 项目组件校验模块。
//!
//! 按系统分发项目组件（规则、配置等）的完整性校验。

use crate::constants::project::{DIR_CONF, FILE_WFUSION, FILE_WPARSE};
use crate::db::RuleType;
use wp_proj::project::{
    CheckComponents, WarpProject,
    checker::{self, CheckComponent, CheckOptions},
    init::PrjScope,
};

use crate::Setting;
use crate::error::AppError;
use crate::utils::{SystemKind, compose_repo_layout_into, layout_for_system};
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// 命令查找的优先搜索路径，兼容容器内 toolchain 安装目录。
const TOOLCHAIN_SEARCH_PATHS: [&str; 2] = ["/app", "/app/toolchain"];

/// 系统级项目校验目标。
#[derive(Debug, Clone, Copy)]
pub enum ProjectCheckTarget {
    /// 校验整个项目。
    WholeProject,
    /// 校验某一种规则/配置类型。
    RuleType(RuleType),
}

/// 在预设搜索路径中定位命令二进制，若找不到则回退到 PATH 查找。
fn resolve_toolchain_command(cmd: &str) -> PathBuf {
    for base in TOOLCHAIN_SEARCH_PATHS {
        let candidate = Path::new(base).join(cmd);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(cmd)
}

impl ProjectCheckTarget {
    fn to_wparse_components(self) -> Vec<CheckComponent> {
        match self {
            ProjectCheckTarget::WholeProject => RuleType::All.to_wparse_check_components(),
            ProjectCheckTarget::RuleType(rule_type) => rule_type.to_wparse_check_components(),
        }
    }

    fn to_wfusion_what(self) -> &'static str {
        match self {
            ProjectCheckTarget::WholeProject => "all",
            ProjectCheckTarget::RuleType(rule_type) => match rule_type {
                RuleType::All => "all",
                RuleType::Parse => "conf",
                RuleType::Source => "sources",
                RuleType::Sink => "sinks",
                RuleType::SourceConnect | RuleType::SinkConnect => "connectors",
                RuleType::Schema => "schemas",
                RuleType::Rule => "rules",
                RuleType::Scenarios => "scenarios",
                // `windows.toml` 暂无独立的 what 选项，继续回退到整体校验。
                RuleType::Windows => "all",
                RuleType::Wpl | RuleType::Oml | RuleType::Wpgen | RuleType::Knowledge => "all",
            },
        }
    }
}

/// 校验项目组件（全局共享项目目录）。
///
/// `wparse` 会合成双仓库到临时目录后调用 `wp_proj`；
/// `wfusion` 先保留入口，避免误调用 `wproj check`。
pub fn validate_project(system: SystemKind, target: ProjectCheckTarget) -> Result<(), AppError> {
    let layout = layout_for_system(system).as_repo_layout();
    let tmp_dir = Setting::workspace_root()
        .join("tmp")
        .join("project-check")
        .join(format!("{}", chrono::Utc::now().timestamp_millis()));
    std::fs::create_dir_all(&tmp_dir).map_err(AppError::internal)?;
    compose_repo_layout_into(&layout, &tmp_dir)?;
    let result = validate_project_in_dir(system, &tmp_dir, target);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

/// 对指定目录执行系统级项目校验。
pub fn validate_project_in_dir(
    system: SystemKind,
    project_path: &Path,
    target: ProjectCheckTarget,
) -> Result<(), AppError> {
    disable_admin_api_for_local_validation(system, project_path)?;
    match system {
        SystemKind::Wparse => {
            check_wparse_components_in_dir(project_path, target.to_wparse_components())
        }
        SystemKind::Wfusion => check_wfusion_in_dir(project_path, target),
    }
}

/// 对指定目录执行 `wparse/wproj` 组件校验。
fn check_wparse_components_in_dir(
    project_path: &Path,
    components: Vec<CheckComponent>,
) -> Result<(), AppError> {
    if !project_path.exists() {
        return Err(AppError::Validation(format!(
            "项目路径不存在: {}",
            project_path.display()
        )));
    }

    // 转换为绝对路径（规范化路径，去除 ./ ../ 等）
    let project_path = project_path.canonicalize().map_err(|e| {
        AppError::Validation(format!(
            "无法规范化项目路径: {} ({})",
            project_path.display(),
            e
        ))
    })?;

    let project_path_str = project_path
        .to_str()
        .ok_or_else(|| AppError::Validation("项目路径包含无效字符".to_string()))?
        .to_string();

    let dict = Default::default();
    let project = WarpProject::load(&project_path_str, PrjScope::Normal, &dict)
        .map_err(|e| AppError::Validation(format!("加载项目失败: {}", e)))?;

    let mut opts = CheckOptions::new(project_path_str);
    opts.console = true;
    opts.fail_fast = true;

    let components = CheckComponents::default().with_only(components);

    checker::check_with(&project, &opts, &components, &dict)
        .map_err(|e| AppError::Validation(format!("组件校验失败: {}", e)))?;

    Ok(())
}

/// 对指定目录执行 `wfusion/wfadm` 项目校验。
///
/// 编辑态校验会先把当前内容覆盖到临时项目，再按 `what` 只校验当前类型；
/// 发布页、导入预检和沙盒仍显式走 `WholeProject` 做整体校验。
fn check_wfusion_in_dir(project_path: &Path, target: ProjectCheckTarget) -> Result<(), AppError> {
    if !project_path.exists() {
        return Err(AppError::Validation(format!(
            "项目路径不存在: {}",
            project_path.display()
        )));
    }

    let what = target.to_wfusion_what();
    let binary = resolve_toolchain_command("wfadm");

    let output = Command::new(&binary)
        .arg("check")
        .arg("--what")
        .arg(what)
        .arg("--fail-fast")
        .current_dir(project_path)
        .output()
        .map_err(|e| {
            AppError::internal(format!(
                "执行 wfadm check 失败 ({}): {}",
                binary.display(),
                e
            ))
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let detail = if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "未提供错误输出".to_string()
    };

    Err(AppError::validation(format!(
        "wfadm check 校验失败(what={}): {}",
        what, detail
    )))
}

/// 本地校验前关闭项目中的 admin_api，避免容器专用证书与 token 路径影响校验结果。
fn disable_admin_api_for_local_validation(
    system: SystemKind,
    project_path: &Path,
) -> Result<(), AppError> {
    let file_name = match system {
        SystemKind::Wparse => FILE_WPARSE,
        SystemKind::Wfusion => FILE_WFUSION,
    };
    let conf_path = project_path.join(DIR_CONF).join(file_name);
    if !conf_path.is_file() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&conf_path).map_err(AppError::internal)?;
    let patched = patch_admin_api_runtime_disabled(&content);
    if patched != content {
        std::fs::write(&conf_path, patched).map_err(AppError::internal)?;
    }

    Ok(())
}

/// 将本地校验时不需要的 admin_api 整体关闭，并移除 auth 配置段。
fn patch_admin_api_runtime_disabled(content: &str) -> String {
    let without_auth = remove_toml_section(content, "[admin_api.auth]");
    patch_admin_api_tls_enabled_false(&patch_admin_api_enabled_false(&without_auth))
}

/// 将指定 TOML 节中的 enabled 统一设为 false；若该节不存在则自动追加。
fn patch_section_enabled_false(content: &str, section_name: &str) -> String {
    let mut lines = Vec::new();
    let mut in_section = false;
    let mut found_section = false;
    let mut patched_enabled = false;
    let mut pending_enabled_insert = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');

        if in_section
            && pending_enabled_insert
            && !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && !trimmed.starts_with("enabled")
        {
            lines.push("enabled = false".to_string());
            pending_enabled_insert = false;
            patched_enabled = true;
        }

        if in_section && is_section && trimmed != section_name {
            in_section = false;
        }

        if trimmed == section_name {
            in_section = true;
            found_section = true;
            patched_enabled = false;
            pending_enabled_insert = true;
            lines.push(line.to_string());
            continue;
        }

        if in_section && trimmed.starts_with("enabled") {
            let indent = line
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .collect::<String>();
            lines.push(format!("{indent}enabled = false"));
            patched_enabled = true;
            pending_enabled_insert = false;
            continue;
        }

        lines.push(line.to_string());
    }

    if found_section {
        if in_section && !patched_enabled {
            lines.push("enabled = false".to_string());
        }
    } else {
        if !lines.last().is_none_or(|line| line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(section_name.to_string());
        lines.push("enabled = false".to_string());
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// 将 [admin_api] 节的 enabled 设为 false；若该节不存在则自动追加。
fn patch_admin_api_enabled_false(content: &str) -> String {
    patch_section_enabled_false(content, "[admin_api]")
}

/// 将 [admin_api.tls] 节的 enabled 设为 false；若该节不存在则自动追加。
fn patch_admin_api_tls_enabled_false(content: &str) -> String {
    patch_section_enabled_false(content, "[admin_api.tls]")
}

/// 删除指定 TOML 节及其内容，直到下一个节头为止。
fn remove_toml_section(content: &str, section_name: &str) -> String {
    let mut lines = Vec::new();
    let mut in_target_section = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');

        if is_section {
            if in_target_section && trimmed != section_name {
                in_target_section = false;
            }
            if trimmed == section_name {
                in_target_section = true;
                continue;
            }
        }

        if !in_target_section {
            lines.push(line.to_string());
        }
    }

    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    output
}
