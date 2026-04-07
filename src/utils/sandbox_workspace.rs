use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::error::AppError;
use crate::server::{FileOverride, Setting, sandbox::OutputFileStatus};
use crate::utils::constants::{BUSINESS_SINK_OVERRIDE, OUTPUT_PATHS, WPSRC_SOURCE_OVERRIDE};

/// 管理沙盒运行时的临时项目目录与日志目录。
#[derive(Debug, Clone)]
pub struct SandboxWorkspace {
    pub root: PathBuf,
    pub project_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub source_project_root: PathBuf,
}

impl SandboxWorkspace {
    /// 准备沙盒目录：复制项目模板并应用覆盖文件。
    pub fn prepare(task_id: &str, overrides: &[FileOverride]) -> Result<Self, AppError> {
        let workspace_root = Setting::workspace_root().clone();
        let base_dir = workspace_root.join("tmp").join("sandbox").join(task_id);
        if base_dir.exists() {
            fs::remove_dir_all(&base_dir).map_err(AppError::internal)?;
        }

        let project_dir = base_dir.join("project");
        let logs_dir = base_dir.join("logs");

        fs::create_dir_all(&project_dir).map_err(AppError::internal)?;
        fs::create_dir_all(&logs_dir).map_err(AppError::internal)?;

        let setting = Setting::load();
        let project_root = resolve_project_root(&setting);
        copy_dir_recursive(&project_root, &project_dir)?;

        apply_static_overrides(&project_dir)?;
        apply_overrides(&project_dir, overrides)?;

        Ok(SandboxWorkspace {
            root: base_dir,
            project_dir,
            logs_dir,
            source_project_root: project_root,
        })
    }

    /// 返回指定日志文件的绝对路径。
    pub fn log_path(&self, name: &str) -> PathBuf {
        self.logs_dir.join(name)
    }

    /// 写入文本日志，返回生成的路径。
    pub fn write_text_log(&self, name: &str, content: &str) -> Result<PathBuf, AppError> {
        let target = self.log_path(name);
        fs::write(&target, content).map_err(AppError::internal)?;
        Ok(target)
    }

    /// 把多份日志按段落打包成单个文件，方便前端下载。
    pub fn bundle_logs(&self, name: &str, sections: &[(&str, &Path)]) -> Result<PathBuf, AppError> {
        let target = self.log_path(name);
        let mut file = fs::File::create(&target).map_err(AppError::internal)?;
        for (label, source) in sections {
            writeln!(file, "===== {} =====", label).map_err(AppError::internal)?;
            if source.exists() {
                let metadata = fs::metadata(source).map_err(AppError::internal)?;
                if metadata.len() == 0 {
                    writeln!(file, "(日志为空)\n").map_err(AppError::internal)?;
                } else {
                    writeln!(file).map_err(AppError::internal)?;
                    let mut src = fs::File::open(source).map_err(AppError::internal)?;
                    io::copy(&mut src, &mut file).map_err(AppError::internal)?;
                    writeln!(file).map_err(AppError::internal)?;
                }
            } else {
                writeln!(file, "(文件不存在: {})\n", source.display())
                    .map_err(AppError::internal)?;
            }
        }
        Ok(target)
    }

    /// 任务完成后清理由工具生成的 project 目录，可选保留。
    pub fn cleanup_after_run(&self, keep_workspace: bool) -> Result<(), AppError> {
        if keep_workspace {
            return Ok(());
        }
        if self.project_dir.exists() {
            fs::remove_dir_all(&self.project_dir).map_err(AppError::internal)?;
        }
        Ok(())
    }

    /// 以树结构形式渲染目录概览，便于调试日志展示。
    pub fn render_tree_listing(
        &self,
        max_depth: usize,
        max_entries: usize,
    ) -> Result<String, AppError> {
        let root_label = self.display_relative(&self.project_dir);
        render_tree(&self.project_dir, &root_label, max_depth, max_entries)
    }

    /// 将路径转换为相对 base 目录的可读字符串。
    pub fn display_relative(&self, path: &Path) -> String {
        relative_to_workspace_root(path)
            .or_else(|| path.strip_prefix(&self.root).ok().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }
}

/// 收集 wparse 输出目录中的关键文件状态。
pub fn collect_output_checks(project_dir: &Path) -> Result<Vec<OutputFileStatus>, AppError> {
    let mut results = Vec::new();
    for (relative, meaning) in OUTPUT_PATHS {
        let path = project_dir.join(relative);
        let line_count = if path.exists() {
            count_lines(&path)?
        } else {
            0
        };
        results.push(OutputFileStatus {
            relative_path: relative.to_string(),
            is_empty: line_count == 0,
            line_count,
            meaning: meaning.to_string(),
        });
    }
    Ok(results)
}

fn count_lines(path: &Path) -> Result<usize, AppError> {
    let mut file = fs::File::open(path).map_err(AppError::internal)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(AppError::internal)?;
    Ok(buf.lines().count())
}

fn apply_overrides(project_dir: &Path, overrides: &[FileOverride]) -> Result<(), AppError> {
    for override_file in overrides {
        let relative = override_file.file.trim();
        if relative.is_empty() {
            continue;
        }
        validate_override_path(relative)?;
        let target = project_dir.join(relative);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        let mut file = fs::File::create(&target).map_err(AppError::internal)?;
        file.write_all(override_file.content.as_bytes())
            .map_err(AppError::internal)?;
    }
    Ok(())
}

fn apply_static_overrides(project_dir: &Path) -> Result<(), AppError> {
    write_override_file(
        project_dir,
        "topology/sinks/business.d/sink.toml",
        BUSINESS_SINK_OVERRIDE,
    )?;
    write_override_file(
        project_dir,
        "topology/sources/wpsrc.toml",
        WPSRC_SOURCE_OVERRIDE,
    )?;
    Ok(())
}

fn write_override_file(project_dir: &Path, relative: &str, content: &str) -> Result<(), AppError> {
    validate_override_path(relative)?;
    let target = project_dir.join(relative);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(AppError::internal)?;
    }
    fs::write(&target, content).map_err(AppError::internal)?;
    Ok(())
}

fn resolve_project_root(setting: &Setting) -> PathBuf {
    let project_root = PathBuf::from(&setting.project_root);
    if project_root.is_absolute() {
        project_root
    } else {
        Setting::workspace_root().join(project_root)
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(src).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry.file_name() == ".git" {
            continue;
        }
        let file_type = entry.file_type().map_err(AppError::internal)?;
        let target_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            fs::create_dir_all(&target_path).map_err(AppError::internal)?;
            copy_dir_recursive(&entry.path(), &target_path)?;
        } else if file_type.is_file() {
            copy_file(&entry.path(), &target_path)?;
        }
    }
    Ok(())
}

fn copy_file(src: &Path, dst: &Path) -> Result<(), AppError> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(AppError::internal)?;
    }
    fs::copy(src, dst).map_err(AppError::internal)?;
    Ok(())
}

/// 确保日志目录存在。
pub fn ensure_logs_dir(path: &Path) -> Result<(), AppError> {
    fs::create_dir_all(path).map_err(AppError::internal)
}

fn validate_override_path(path_str: &str) -> Result<(), AppError> {
    let path = Path::new(path_str);
    if path.is_absolute() {
        return Err(AppError::validation(format!(
            "override 文件路径必须是 project_root 内的相对路径: {}",
            path_str
        )));
    }

    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(AppError::validation(format!(
            "override 文件路径不允许包含 .. 或根路径: {}",
            path_str
        )));
    }

    Ok(())
}

fn relative_to_workspace_root(path: &Path) -> Option<PathBuf> {
    path.strip_prefix(Setting::workspace_root())
        .map(|p| p.to_path_buf())
        .ok()
}

fn render_tree(
    root: &Path,
    root_label: &str,
    max_depth: usize,
    max_entries: usize,
) -> Result<String, AppError> {
    let mut lines = Vec::new();
    lines.push(root_label.to_string());
    let mut counter = 0;
    build_tree_lines(
        root,
        "",
        0,
        max_depth,
        max_entries,
        &mut counter,
        &mut lines,
    )?;
    Ok(lines.join("\n"))
}

fn build_tree_lines(
    path: &Path,
    prefix: &str,
    depth: usize,
    max_depth: usize,
    max_entries: usize,
    counter: &mut usize,
    lines: &mut Vec<String>,
) -> Result<(), AppError> {
    if depth >= max_depth {
        return Ok(());
    }
    let mut entries = fs::read_dir(path)
        .map_err(AppError::internal)?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());

    for (idx, entry) in entries.iter().enumerate() {
        if *counter >= max_entries {
            lines.push(format!("{}└── ...", prefix));
            break;
        }
        let is_last = idx == entries.len() - 1;
        let branch = if is_last { "└──" } else { "├──" };
        let name = entry.file_name().to_string_lossy().to_string();
        lines.push(format!("{}{} {}", prefix, branch, name));
        *counter += 1;
        if entry.file_type().map_err(AppError::internal)?.is_dir() {
            let next_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
            build_tree_lines(
                &entry.path(),
                &next_prefix,
                depth + 1,
                max_depth,
                max_entries,
                counter,
                lines,
            )?;
        }
    }
    Ok(())
}
