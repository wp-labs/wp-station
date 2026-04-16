use crate::error::AppError;
use crate::utils::project::resolve_project_root;
use rust_embed::RustEmbed;
use std::fs;

#[derive(RustEmbed)]
#[folder = "default_configs/"]
struct DefaultConfigs;

/// 从嵌入的默认配置目录初始化 project_root。已存在文件不覆盖，保证用户编辑结果优先。
pub fn init_default_configs_to_project(project_root: &str) -> Result<(), AppError> {
    info!("开始从嵌入的默认配置初始化 project_root");

    let project_dir = resolve_project_root(project_root);
    fs::create_dir_all(&project_dir).map_err(AppError::internal)?;

    let mut written = 0usize;
    let mut skipped = 0usize;

    for file_path in DefaultConfigs::iter() {
        let path_str = file_path.as_ref();
        if should_skip_embedded_path(path_str) {
            continue;
        }

        let Some(content_file) = DefaultConfigs::get(path_str) else {
            continue;
        };

        let target_path = project_dir.join(path_str);
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
        "默认配置初始化完成: project_root={}, written={}, skipped={}",
        project_dir.display(),
        written,
        skipped
    );
    Ok(())
}

fn should_skip_embedded_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}
