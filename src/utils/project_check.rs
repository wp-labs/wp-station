//! 项目组件校验模块。
//!
//! 基于 wp_proj 对 `project_root` 中的项目组件（规则、配置等）进行完整性校验。

use wp_proj::project::{
    CheckComponents, WarpProject,
    checker::{self, CheckComponent, CheckOptions},
    init::PrjScope,
};

use crate::Setting;
use crate::error::AppError;

/// 校验项目组件（全局共享项目目录）。
///
/// 加载 `project_root` 并执行指定组件的校验规则，校验失败返回 `AppError::Validation`。
pub fn check_component(components: Vec<CheckComponent>) -> Result<(), AppError> {
    let setting = Setting::load();

    // 构建项目路径
    let project_root = std::path::PathBuf::from(&setting.project_root);
    let project_path = if project_root.is_absolute() {
        project_root
    } else {
        // 相对路径：基于服务启动时的工作目录
        Setting::workspace_root().join(&setting.project_root)
    };

    // 检查项目路径是否存在
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
