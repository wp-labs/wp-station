//! 项目组件校验模块。
//!
//! 基于 wp_proj 对项目组件（规则、配置等）进行完整性校验。

use wp_proj::project::{
    CheckComponents, WarpProject,
    checker::{self, CheckComponent, CheckOptions},
    init::PrjScope,
};

use crate::Setting;
use crate::error::AppError;
use crate::utils::compose_project_layout_into;
use std::path::Path;

/// 校验项目组件（全局共享项目目录）。
///
/// 合成双仓库到临时目录并执行指定组件的校验规则，校验失败返回 `AppError::Validation`。
pub fn check_component(components: Vec<CheckComponent>) -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = setting.project_layout();
    let tmp_dir = Setting::workspace_root()
        .join("tmp")
        .join("project-check")
        .join(format!("{}", chrono::Utc::now().timestamp_millis()));
    std::fs::create_dir_all(&tmp_dir).map_err(AppError::internal)?;
    compose_project_layout_into(&layout, &tmp_dir)?;
    let result = check_component_in_dir(&tmp_dir, components);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

/// 对指定目录执行组件校验。
pub fn check_component_in_dir(
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
