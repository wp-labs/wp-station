//! 沙盒工作区准备、文件覆盖与产物裁剪。

use std::fs::{self};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::constants::project::{
    DIR_BUSINESS_D, DIR_CONF, DIR_INFRA_D, DIR_MODELS, DIR_RUNTIME, DIR_SINKS, DIR_SOURCES,
    DIR_TOPOLOGY, FILE_WFUSION,
};
use crate::constants::sandbox::{
    OUTPUT_PATHS, RUNTIME_ARTIFACT_RETENTION_RUNS, RUNTIME_HEADER_MODE, RUNTIME_OUTPUT_ADDR,
    RUNTIME_OUTPUT_CONNECTOR, RUNTIME_PROTOCOL, RUNTIME_SOURCE_ADDR, RUNTIME_SOURCE_CONNECTOR,
    RUNTIME_SOURCE_KEY, RUNTIME_UDP_PORT, WFUSION_RUNTIME_SOURCE_CONNECTOR,
    WFUSION_RUNTIME_SOURCE_KEY, WFUSION_RUNTIME_TCP_PORT,
};
use crate::error::AppError;
use crate::server::{FileOverride, Setting, sandbox::OutputFileStatus};
use crate::utils::{
    SystemKind, compose_repo_layout_into, layout_for_system, runtime_default_configs_dir,
};

/// 管理沙盒运行时的临时项目目录与日志目录。
///
/// 每次沙盒任务会将双仓库合成到临时目录，并应用用户覆盖文件及沙盒运行时必需配置。
#[derive(Debug, Clone)]
pub struct SandboxWorkspace {
    /// 沙盒根目录（包含 project/ 和 logs/）。
    pub root: PathBuf,
    /// 沙盒项目目录（由 models + infra 合成而来）。
    pub project_dir: PathBuf,
    /// 日志目录。
    pub logs_dir: PathBuf,
    /// 源 models 仓库目录。
    pub source_models_root: PathBuf,
    /// 源 infra 仓库目录。
    pub source_infra_root: PathBuf,
    /// 共享 connectors 仓库目录。
    pub source_connectors_root: PathBuf,
}

impl SandboxWorkspace {
    /// 准备沙盒目录：复制项目模板并应用覆盖文件。
    pub fn prepare(
        task_id: &str,
        system: SystemKind,
        overrides: &[FileOverride],
    ) -> Result<Self, AppError> {
        let workspace_root = Setting::workspace_root().clone();
        let base_dir = workspace_root.join("tmp").join("sandbox").join(task_id);
        if base_dir.exists() {
            fs::remove_dir_all(&base_dir).map_err(AppError::internal)?;
        }

        let project_dir = base_dir.join("project");
        let logs_dir = base_dir.join("logs");

        fs::create_dir_all(&project_dir).map_err(AppError::internal)?;
        fs::create_dir_all(&logs_dir).map_err(AppError::internal)?;

        let layout = layout_for_system(system).as_repo_layout();
        compose_repo_layout_into(&layout, &project_dir)?;

        apply_overrides(&project_dir, overrides)?;
        ensure_sandbox_runtime_dir(&project_dir, system)?;
        ensure_wfusion_sandbox_scenario(&project_dir, system)?;
        normalize_wfusion_scenario_use_paths(&project_dir, system)?;
        apply_sandbox_runtime_overrides(&project_dir, system)?;
        apply_sandbox_default_infra_sink_overrides(&project_dir, system)?;
        apply_sandbox_default_business_sink_overrides(&project_dir, system)?;
        harden_admin_api_token_permissions(&project_dir)?;

        Ok(SandboxWorkspace {
            root: base_dir,
            project_dir,
            logs_dir,
            source_models_root: layout.models_root,
            source_infra_root: layout.infra_root,
            source_connectors_root: layout.connectors_root,
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

    /// 任务完成后裁剪历史 project 目录，所有阶段日志均保留。
    pub fn cleanup_after_run(&self, _keep_workspace: bool) -> Result<(), AppError> {
        if let Some(sandbox_root) = self.root.parent() {
            prune_sandbox_runtime_artifacts(sandbox_root)?;
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

/// 收集 wparse 输出目录中的关键文件状态，供分析阶段判断是否有数据产出。
pub fn collect_output_checks(project_dir: &Path) -> Result<Vec<OutputFileStatus>, AppError> {
    let mut results = Vec::new();
    for (relative, meaning, affects_pass) in OUTPUT_PATHS {
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
            affects_pass,
        });
    }
    Ok(results)
}

/// 统计文件行数，用于判断输出文件是否为空。
fn count_lines(path: &Path) -> Result<usize, AppError> {
    let mut file = fs::File::open(path).map_err(AppError::internal)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).map_err(AppError::internal)?;
    Ok(buf.lines().count())
}

/// 应用用户提交的文件覆盖，写入选定文件到沙盒项目目录。
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

/// 沙盒覆盖动作类型。
#[derive(Clone, Copy)]
enum SandboxOverrideKind {
    PatchWparseAdminApi,
    PatchWpsrcRuntime,
    PatchWpgenRuntime,
    PatchWfusionConfig,
    RewriteWfusionSource,
}

/// 单个沙盒覆盖规则定义。
#[derive(Clone, Copy)]
struct SandboxOverrideSpec {
    relative_path: &'static str,
    kind: SandboxOverrideKind,
}

const WPARSE_SANDBOX_OVERRIDE_SPECS: [SandboxOverrideSpec; 3] = [
    SandboxOverrideSpec {
        relative_path: "conf/wparse.toml",
        kind: SandboxOverrideKind::PatchWparseAdminApi,
    },
    SandboxOverrideSpec {
        relative_path: "topology/sources/wpsrc.toml",
        kind: SandboxOverrideKind::PatchWpsrcRuntime,
    },
    SandboxOverrideSpec {
        relative_path: "conf/wpgen.toml",
        kind: SandboxOverrideKind::PatchWpgenRuntime,
    },
];

const WFUSION_SANDBOX_OVERRIDE_SPECS: [SandboxOverrideSpec; 2] = [
    SandboxOverrideSpec {
        relative_path: "conf/wfusion.toml",
        kind: SandboxOverrideKind::PatchWfusionConfig,
    },
    SandboxOverrideSpec {
        relative_path: "topology/sources",
        kind: SandboxOverrideKind::RewriteWfusionSource,
    },
];

impl SandboxOverrideSpec {
    fn summary(self) -> String {
        match self.kind {
            SandboxOverrideKind::PatchWparseAdminApi => {
                "admin_api.enabled=false, 移除 admin_api.auth, admin_api.tls.enabled=false"
                    .to_string()
            }
            SandboxOverrideKind::PatchWpsrcRuntime => format!(
                "仅保留沙盒 UDP 输入: connect={}, addr={}, port={}, protocol={}, header_mode={}, 其他 source 全部 disable",
                RUNTIME_SOURCE_CONNECTOR,
                RUNTIME_SOURCE_ADDR,
                RUNTIME_UDP_PORT,
                RUNTIME_PROTOCOL,
                RUNTIME_HEADER_MODE
            ),
            SandboxOverrideKind::PatchWpgenRuntime => format!(
                "connect={}, addr={}, port={}",
                RUNTIME_OUTPUT_CONNECTOR, RUNTIME_OUTPUT_ADDR, RUNTIME_UDP_PORT
            ),
            SandboxOverrideKind::PatchWfusionConfig => {
                "保留当前 wfusion.toml，仅关闭 admin_api/auth/tls 以适配沙盒".to_string()
            }
            SandboxOverrideKind::RewriteWfusionSource => format!(
                "仅保留沙盒 TCP 输入: connect={}, addr=0.0.0.0, port=\"{}\", framing=len, data_format=arrow_framed",
                WFUSION_RUNTIME_SOURCE_CONNECTOR, WFUSION_RUNTIME_TCP_PORT
            ),
        }
    }

    fn apply(self, project_dir: &Path) -> Result<(), AppError> {
        match self.kind {
            SandboxOverrideKind::PatchWparseAdminApi => patch_override_file(
                project_dir,
                self.relative_path,
                patch_wparse_admin_api_runtime,
            ),
            SandboxOverrideKind::PatchWpsrcRuntime => {
                patch_override_file(project_dir, self.relative_path, patch_wpsrc_runtime)
            }
            SandboxOverrideKind::PatchWpgenRuntime => {
                patch_override_file(project_dir, self.relative_path, patch_wpgen_runtime)
            }
            SandboxOverrideKind::PatchWfusionConfig => patch_or_seed_wfusion_conf(project_dir),
            SandboxOverrideKind::RewriteWfusionSource => {
                rewrite_wfusion_source_runtime(project_dir)
            }
        }
    }
}

/// 统一应用沙盒运行时文件覆盖，避免覆盖逻辑分散在多个函数中。
fn apply_sandbox_runtime_overrides(project_dir: &Path, system: SystemKind) -> Result<(), AppError> {
    let specs = match system {
        SystemKind::Wparse => &WPARSE_SANDBOX_OVERRIDE_SPECS[..],
        SystemKind::Wfusion => &WFUSION_SANDBOX_OVERRIDE_SPECS[..],
    };
    for spec in specs {
        spec.apply(project_dir)?;
    }
    Ok(())
}

/// 沙盒环境中按系统覆盖默认 sink，避免用户自定义外部输出影响模拟。
fn apply_sandbox_default_infra_sink_overrides(
    project_dir: &Path,
    system: SystemKind,
) -> Result<(), AppError> {
    if matches!(system, SystemKind::Wparse) {
        return replace_wparse_sandbox_sinks_with_defaults(project_dir);
    }

    let Some(default_root) = runtime_default_configs_dir() else {
        return Err(AppError::internal(
            "沙盒覆盖 infra sink 失败: 未找到 default_configs 目录".to_string(),
        ));
    };

    let source_dir = resolve_sandbox_default_sink_dir(&default_root, system, DIR_INFRA_D);
    if !source_dir.is_dir() {
        return Err(AppError::internal(format!(
            "沙盒覆盖 infra sink 失败: 默认目录不存在 {}",
            source_dir.display()
        )));
    }

    let target_dir = project_dir
        .join(DIR_TOPOLOGY)
        .join(DIR_SINKS)
        .join(DIR_INFRA_D);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(AppError::internal)?;
    }
    fs::create_dir_all(&target_dir).map_err(AppError::internal)?;

    copy_dir_replace_all(&source_dir, &target_dir)
}

/// 沙盒环境中强制使用仓库 default_configs 的 business sink，避免用户自定义外部输出影响模拟。
fn apply_sandbox_default_business_sink_overrides(
    project_dir: &Path,
    system: SystemKind,
) -> Result<(), AppError> {
    if matches!(system, SystemKind::Wparse) {
        return Ok(());
    }

    let target_dir = project_dir
        .join(DIR_TOPOLOGY)
        .join(DIR_SINKS)
        .join(DIR_BUSINESS_D);
    fs::create_dir_all(&target_dir).map_err(AppError::internal)?;

    if !contains_toml_files(&target_dir)? {
        let Some(default_root) = runtime_default_configs_dir() else {
            return Err(AppError::internal(
                "沙盒覆盖 business sink 失败: 未找到 default_configs 目录".to_string(),
            ));
        };

        let source_dir = resolve_sandbox_default_sink_dir(&default_root, system, DIR_BUSINESS_D);
        if !source_dir.is_dir() {
            return Err(AppError::internal(format!(
                "沙盒覆盖 business sink 失败: 默认目录不存在 {}",
                source_dir.display()
            )));
        }

        copy_dir_replace_all(&source_dir, &target_dir)?;
    }

    rewrite_wfusion_business_sink_runtime(&target_dir)
}

/// 将 wparse 沙盒的整个 topology/sinks 目录还原为仓库默认内容。
fn replace_wparse_sandbox_sinks_with_defaults(project_dir: &Path) -> Result<(), AppError> {
    let Some(default_root) = runtime_default_configs_dir() else {
        return Err(AppError::internal(
            "沙盒覆盖 wparse sinks 失败: 未找到 default_configs 目录".to_string(),
        ));
    };

    let source_dir = default_root
        .join(SystemKind::Wparse.as_ref())
        .join(DIR_TOPOLOGY)
        .join(DIR_SINKS);
    if !source_dir.is_dir() {
        return Err(AppError::internal(format!(
            "沙盒覆盖 wparse sinks 失败: 默认目录不存在 {}，期望路径 default_configs/wparse/topology/sinks",
            source_dir.display()
        )));
    }

    let target_dir = project_dir.join(DIR_TOPOLOGY).join(DIR_SINKS);
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(AppError::internal)?;
    }
    fs::create_dir_all(&target_dir).map_err(AppError::internal)?;
    copy_dir_replace_all(&source_dir, &target_dir)
}

/// 解析沙盒默认 infra sink 目录。
///
/// 兼容两套目录结构：
/// 1. 新结构：`default_configs/<system>/topology/sinks/infra.d`
/// 2. 旧结构：`default_configs/topology/sinks/infra.d`
fn resolve_sandbox_default_sink_root(default_root: &Path, system: SystemKind) -> PathBuf {
    let new_layout_dir = default_root
        .join(system.as_ref())
        .join(DIR_TOPOLOGY)
        .join(DIR_SINKS);
    if new_layout_dir.is_dir() {
        return new_layout_dir;
    }

    default_root.join(DIR_TOPOLOGY).join(DIR_SINKS)
}

/// 解析沙盒默认 infra / business sink 子目录。
///
/// 兼容两套目录结构：
/// 1. 新结构：`default_configs/<system>/topology/sinks/<sink_dir>`
/// 2. 旧结构：`default_configs/topology/sinks/<sink_dir>`
fn resolve_sandbox_default_sink_dir(
    default_root: &Path,
    system: SystemKind,
    sink_dir: &str,
) -> PathBuf {
    resolve_sandbox_default_sink_root(default_root, system).join(sink_dir)
}

/// 递归复制目录，目标存在时直接覆盖同名文件。
fn copy_dir_replace_all(source_dir: &Path, target_dir: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(source_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());

        if source_path.is_dir() {
            fs::create_dir_all(&target_path).map_err(AppError::internal)?;
            copy_dir_replace_all(&source_path, &target_path)?;
            continue;
        }

        if !source_path.is_file() {
            continue;
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        copy_file_preserve_permissions(&source_path, &target_path)?;
    }

    Ok(())
}

/// 复制单个文件并尽量保留源文件权限，避免 admin_api.token 等敏感文件权限被放宽。
fn copy_file_preserve_permissions(source_path: &Path, target_path: &Path) -> Result<(), AppError> {
    fs::copy(source_path, target_path).map_err(AppError::internal)?;
    let permissions = fs::metadata(source_path)
        .map_err(AppError::internal)?
        .permissions();
    fs::set_permissions(target_path, permissions).map_err(AppError::internal)?;
    Ok(())
}

/// 收紧 admin_api token 文件权限，满足 wfusion/wparse 对 owner-only 的要求。
fn harden_admin_api_token_permissions(project_dir: &Path) -> Result<(), AppError> {
    let token_path = project_dir.join(DIR_RUNTIME).join("admin_api.token");
    if !token_path.is_file() {
        return Ok(());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&token_path)
            .map_err(AppError::internal)?
            .permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(&token_path, permissions).map_err(AppError::internal)?;
    }

    Ok(())
}

/// 沙盒中的 wfusion 配置优先保留当前仓库内容，仅补丁 admin_api 相关项。
/// 若工作区中不存在该文件，再回退到 default_configs 模板。
fn patch_or_seed_wfusion_conf(project_dir: &Path) -> Result<(), AppError> {
    let target_path = project_dir.join(DIR_CONF).join(FILE_WFUSION);
    if !target_path.is_file() {
        let Some(default_root) = runtime_default_configs_dir() else {
            return Err(AppError::internal(
                "沙盒覆盖 wfusion.toml 失败: 未找到 default_configs 目录".to_string(),
            ));
        };

        let source_path =
            resolve_sandbox_default_conf_file(&default_root, SystemKind::Wfusion, FILE_WFUSION);
        if !source_path.is_file() {
            return Err(AppError::internal(format!(
                "沙盒覆盖 wfusion.toml 失败: 默认文件不存在 {}",
                source_path.display()
            )));
        }

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        fs::copy(&source_path, &target_path).map_err(AppError::internal)?;
    }

    let content = fs::read_to_string(&target_path).map_err(AppError::internal)?;
    let patched = patch_admin_api_runtime_disabled(&content);
    fs::write(&target_path, patched).map_err(AppError::internal)?;
    Ok(())
}

/// 解析沙盒默认 conf 文件路径，兼容 `default_configs/<system>/conf/*` 与旧结构。
fn resolve_sandbox_default_conf_file(
    default_root: &Path,
    system: SystemKind,
    file_name: &str,
) -> PathBuf {
    let system_name = system.as_ref();
    let new_layout_file = default_root
        .join(system_name)
        .join(DIR_CONF)
        .join(file_name);
    if new_layout_file.is_file() {
        return new_layout_file;
    }

    default_root.join(DIR_CONF).join(file_name)
}

/// 确保沙盒项目目录中存在 runtime 目录。
fn ensure_sandbox_runtime_dir(project_dir: &Path, system: SystemKind) -> Result<(), AppError> {
    let runtime_dir = project_dir.join(DIR_RUNTIME);
    if runtime_dir.is_dir() {
        return Ok(());
    }

    let Some(default_root) = runtime_default_configs_dir() else {
        return Ok(());
    };
    let source_dir = default_root.join(system.as_ref()).join(DIR_RUNTIME);
    if source_dir.is_dir() {
        copy_dir_replace_all(&source_dir, &runtime_dir)?;
    }
    Ok(())
}

/// 若 wfusion 工作区缺少场景文件，则从 default_configs 注入默认场景。
fn ensure_wfusion_sandbox_scenario(project_dir: &Path, system: SystemKind) -> Result<(), AppError> {
    if !matches!(system, SystemKind::Wfusion) {
        return Ok(());
    }

    let scenarios_dir = project_dir.join("models/scenarios");
    if contains_wfg_files(&scenarios_dir)? {
        return Ok(());
    }

    let Some(default_root) = runtime_default_configs_dir() else {
        return Ok(());
    };
    let source_dir = default_root.join(system.as_ref()).join("models/scenarios");
    if source_dir.is_dir() {
        copy_dir_replace_all(&source_dir, &scenarios_dir)?;
    }
    Ok(())
}

fn contains_wfg_files(dir: &Path) -> Result<bool, AppError> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() && contains_wfg_files(&path)? {
            return Ok(true);
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wfg"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn contains_toml_files(dir: &Path) -> Result<bool, AppError> {
    if !dir.is_dir() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() && contains_toml_files(&path)? {
            return Ok(true);
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 将 wfusion 业务 sink 强制改写为本地文件输出，避免真实下游副作用。
fn rewrite_wfusion_business_sink_runtime(target_dir: &Path) -> Result<(), AppError> {
    let mut files = Vec::new();
    collect_toml_files(target_dir, &mut files)?;
    for file in files {
        let content = fs::read_to_string(&file).map_err(AppError::internal)?;
        let patched = patch_wfusion_business_sink_runtime(&content)?;
        fs::write(&file, patched).map_err(AppError::internal)?;
    }
    Ok(())
}

fn collect_toml_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), AppError> {
    if !dir.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() {
            collect_toml_files(&path, files)?;
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// 归一化 wfusion 场景中的 `use "*.wfs|*.wfl"` 路径，兼容当前 models 嵌套目录布局。
fn normalize_wfusion_scenario_use_paths(
    project_dir: &Path,
    system: SystemKind,
) -> Result<(), AppError> {
    if !matches!(system, SystemKind::Wfusion) {
        return Ok(());
    }

    let scenarios_dir = project_dir.join(DIR_MODELS).join("scenarios");
    let mut scenario_files = Vec::new();
    collect_wfg_files(&scenarios_dir, &mut scenario_files)?;
    for scenario_path in scenario_files {
        normalize_single_wfusion_scenario_use_paths(project_dir, &scenario_path)?;
    }

    Ok(())
}

fn normalize_single_wfusion_scenario_use_paths(
    project_dir: &Path,
    scenario_path: &Path,
) -> Result<(), AppError> {
    let content = fs::read_to_string(scenario_path).map_err(AppError::internal)?;
    let scenario_dir = scenario_path.parent().ok_or_else(|| {
        AppError::internal(format!("场景文件缺少父目录: {}", scenario_path.display()))
    })?;
    let scenario_group = scenario_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let mut changed = false;
    let mut output = Vec::new();

    for line in content.lines() {
        let Some((indent, raw_path, suffix)) = parse_wfg_use_line(line) else {
            output.push(line.to_string());
            continue;
        };

        let ext = Path::new(&raw_path)
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if ext != "wfs" && ext != "wfl" {
            output.push(line.to_string());
            continue;
        }

        let current_target = scenario_dir.join(&raw_path);
        if current_target.exists() {
            output.push(line.to_string());
            continue;
        }

        let resolved = resolve_wfusion_use_target(project_dir, scenario_group, &raw_path, &ext)?;
        if let Some(target_path) = resolved {
            let relative = relative_path_from_dir(scenario_dir, &target_path);
            let relative = relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            output.push(format!("{indent}use \"{relative}\"{suffix}"));
            changed = true;
        } else {
            output.push(line.to_string());
        }
    }

    if !changed {
        return Ok(());
    }

    let mut normalized = output.join("\n");
    if content.ends_with('\n') {
        normalized.push('\n');
    }
    fs::write(scenario_path, normalized).map_err(AppError::internal)?;
    Ok(())
}

fn parse_wfg_use_line(line: &str) -> Option<(String, String, String)> {
    let trimmed_start = line.trim_start();
    if !trimmed_start.starts_with("use ") {
        return None;
    }

    let indent_len = line.len().saturating_sub(trimmed_start.len());
    let indent = line[..indent_len].to_string();
    let quote_start = line[indent_len..].find('"')? + indent_len;
    let quote_end = line[quote_start + 1..].find('"')? + quote_start + 1;
    let raw_path = line[quote_start + 1..quote_end].to_string();
    let suffix = line[quote_end + 1..].to_string();
    Some((indent, raw_path, suffix))
}

fn resolve_wfusion_use_target(
    project_dir: &Path,
    scenario_group: &str,
    raw_path: &str,
    ext: &str,
) -> Result<Option<PathBuf>, AppError> {
    let (category_dir, preferred_file) = match ext {
        "wfs" => (
            "schemas",
            project_dir
                .join(DIR_MODELS)
                .join("schemas")
                .join(scenario_group),
        ),
        "wfl" => (
            "rules",
            project_dir
                .join(DIR_MODELS)
                .join("rules")
                .join(scenario_group),
        ),
        _ => return Ok(None),
    };

    let file_name = Path::new(raw_path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if file_name.is_empty() {
        return Ok(None);
    }

    let preferred_path = preferred_file.join(file_name);
    if preferred_path.is_file() {
        return Ok(Some(preferred_path));
    }

    let search_root = project_dir.join(DIR_MODELS).join(category_dir);
    let mut matches = Vec::new();
    collect_named_files(&search_root, file_name, &mut matches)?;
    if matches.len() == 1 {
        return Ok(matches.into_iter().next());
    }

    Ok(None)
}

fn collect_named_files(
    dir: &Path,
    file_name: &str,
    acc: &mut Vec<PathBuf>,
) -> Result<(), AppError> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() {
            collect_named_files(&path, file_name, acc)?;
            continue;
        }
        if path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value == file_name)
        {
            acc.push(path);
        }
    }

    Ok(())
}

fn relative_path_from_dir(from_dir: &Path, target: &Path) -> PathBuf {
    let from_components: Vec<_> = from_dir.components().collect();
    let target_components: Vec<_> = target.components().collect();
    let mut common_len = 0usize;

    while common_len < from_components.len()
        && common_len < target_components.len()
        && from_components[common_len] == target_components[common_len]
    {
        common_len += 1;
    }

    let mut relative = PathBuf::new();
    for _ in common_len..from_components.len() {
        relative.push("..");
    }
    for component in target_components.iter().skip(common_len) {
        relative.push(component.as_os_str());
    }
    relative
}

/// 重写 wfusion source 目录，仅保留沙盒 TCP 输入源。
fn rewrite_wfusion_source_runtime(project_dir: &Path) -> Result<(), AppError> {
    let sources_dir = project_dir.join(DIR_TOPOLOGY).join(DIR_SOURCES);
    if sources_dir.exists() {
        fs::remove_dir_all(&sources_dir).map_err(AppError::internal)?;
    }
    fs::create_dir_all(&sources_dir).map_err(AppError::internal)?;

    let stream =
        detect_wfusion_scenario_stream(project_dir).unwrap_or_else(|| "conn_events".to_string());
    let source_content = format!(
        "connect = \"{connector}\"\nkey = \"{key}\"\nstream = \"{stream}\"\naddr = \"0.0.0.0\"\nport = \"{port}\"\nframing = \"len\"\ndata_format = \"arrow_framed\"\nmode = \"daemon\"\nsources_dir = \"topology/sources\"\nsinks = \"topology/sinks\"\n",
        connector = WFUSION_RUNTIME_SOURCE_CONNECTOR,
        key = WFUSION_RUNTIME_SOURCE_KEY,
        stream = stream,
        port = WFUSION_RUNTIME_TCP_PORT,
    );
    fs::write(sources_dir.join("sandbox.toml"), source_content).map_err(AppError::internal)
}

/// 从首个场景文件中提取 `traffic { stream xxx gen ... }` 的流名称，供 wfusion source 复写复用。
fn detect_wfusion_scenario_stream(project_dir: &Path) -> Option<String> {
    let scenarios_dir = project_dir.join("models/scenarios");
    let first = first_wfg_file(&scenarios_dir).ok().flatten()?;
    let content = fs::read_to_string(first).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("traffic") && !trimmed.contains("stream ") {
            continue;
        }
        if let Some(stream_pos) = trimmed.find("stream ") {
            let tail = &trimmed[stream_pos + "stream ".len()..];
            let stream = tail
                .split(|ch: char| ch.is_whitespace() || ch == '{' || ch == '}')
                .find(|part| !part.is_empty())?;
            return Some(stream.to_string());
        }
    }
    None
}

fn first_wfg_file(dir: &Path) -> Result<Option<PathBuf>, AppError> {
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut files = Vec::new();
    collect_wfg_files(dir, &mut files)?;
    files.sort();
    Ok(files.into_iter().next())
}

fn collect_wfg_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), AppError> {
    for entry in fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() {
            collect_wfg_files(&path, files)?;
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wfg"))
        {
            files.push(path);
        }
    }
    Ok(())
}

/// 输出沙盒运行时覆盖摘要，供 prepare.log 与前端诊断展示复用。
pub(crate) fn sandbox_runtime_override_log_lines(
    workspace: &SandboxWorkspace,
    system: SystemKind,
) -> Vec<String> {
    let specs = match system {
        SystemKind::Wparse => &WPARSE_SANDBOX_OVERRIDE_SPECS[..],
        SystemKind::Wfusion => &WFUSION_SANDBOX_OVERRIDE_SPECS[..],
    };
    let mut lines: Vec<String> = specs
        .iter()
        .map(|spec| {
            format!(
                "{} -> {}",
                workspace.display_relative(&workspace.project_dir.join(spec.relative_path)),
                spec.summary()
            )
        })
        .collect();
    if matches!(system, SystemKind::Wparse) {
        lines.push(
            "topology/sinks/** -> 强制回退到 default_configs/wparse/topology/sinks".to_string(),
        );
    } else {
        lines.push(
            "topology/sinks/infra.d/*.toml -> 强制使用 default_configs 默认配置覆盖".to_string(),
        );
        lines.push(
            "topology/sinks/business.d/*.toml -> 强制使用 default_configs 默认配置覆盖".to_string(),
        );
    }
    lines
}

/// 读取并补丁 project_dir 内现有文件，保留未修改部分内容。
fn patch_override_file(
    project_dir: &Path,
    relative: &str,
    patcher: fn(&str) -> Result<String, AppError>,
) -> Result<(), AppError> {
    validate_override_path(relative)?;
    let target = project_dir.join(relative);
    let content = fs::read_to_string(&target).map_err(AppError::internal)?;
    let patched = patcher(&content)?;
    fs::write(&target, patched).map_err(AppError::internal)?;
    Ok(())
}

/// 将沙盒中的 wparse admin_api 整体关闭，并移除鉴权配置。
fn patch_wparse_admin_api_runtime(content: &str) -> Result<String, AppError> {
    Ok(patch_admin_api_runtime_disabled(content))
}

/// 将 wpsrc.toml 中 gen_udp source 切到沙盒运行时值，并关闭其他所有输入源。
fn patch_wpsrc_runtime(content: &str) -> Result<String, AppError> {
    patch_wpsrc_source_runtime(content, RUNTIME_SOURCE_CONNECTOR, RUNTIME_UDP_PORT)
}

/// 将 wpgen.toml 的输出 connector、addr 与端口切到沙盒运行时值。
fn patch_wpgen_runtime(content: &str) -> Result<String, AppError> {
    patch_wpgen_output_runtime(
        content,
        RUNTIME_OUTPUT_CONNECTOR,
        RUNTIME_OUTPUT_ADDR,
        RUNTIME_UDP_PORT,
    )
}

/// 将 wparse.toml 中 [admin_api] 节的 enabled 设为 false。
/// 若该节不存在则追加 [admin_api] + enabled = false。
fn patch_admin_api_enabled_false(content: &str) -> String {
    patch_section_enabled_false(content, "[admin_api]")
}

/// 将沙盒中的 admin_api 整体关闭。
///
/// 这里不再删除 `[admin_api.auth]` 段，避免沙盒文件和仓库原文件的行号错位。
fn patch_admin_api_runtime_disabled(content: &str) -> String {
    patch_admin_api_tls_enabled_false(&patch_admin_api_enabled_false(content))
}

/// 将 admin_api.tls.enabled 设为 false，避免沙盒按 HTTPS 启动。
fn patch_admin_api_tls_enabled_false(content: &str) -> String {
    patch_section_enabled_false(content, "[admin_api.tls]")
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

/// 在保留原格式与注释的前提下，仅保留沙盒 UDP source，其他 source 全部改为关闭。
fn patch_wpsrc_source_runtime(content: &str, connect: &str, port: u16) -> Result<String, AppError> {
    let mut lines = Vec::new();
    let mut block_lines: Vec<String> = Vec::new();
    let mut block_key_value: Option<String> = None;
    let mut block_key_index: Option<usize> = None;
    let mut block_enable_index: Option<usize> = None;
    let mut block_connect_index: Option<usize> = None;
    let mut block_port_index: Option<usize> = None;
    let mut block_has_params_section = false;
    let mut in_sources_block = false;
    let mut in_source_params = false;
    let mut found_target = false;
    let mut patched_connect = false;
    let mut patched_port = false;

    let flush_source_block = |lines: &mut Vec<String>,
                              block_lines: &mut Vec<String>,
                              block_key_value: &mut Option<String>,
                              block_key_index: &mut Option<usize>,
                              block_enable_index: &mut Option<usize>,
                              block_connect_index: &mut Option<usize>,
                              block_port_index: &mut Option<usize>,
                              block_has_params_section: &mut bool,
                              found_target: &mut bool,
                              patched_connect: &mut bool,
                              patched_port: &mut bool| {
        let is_target = block_key_value.as_deref() == Some(RUNTIME_SOURCE_KEY);

        if is_target {
            *found_target = true;
            block_lines.clear();
            block_lines.push("[[sources]]".to_string());
            block_lines.push(format!("key = \"{}\"", RUNTIME_SOURCE_KEY));
            block_lines.push("enable = true".to_string());
            block_lines.push(format!("connect = \"{connect}\""));
            block_lines.push("tags = []".to_string());
            block_lines.push(String::new());
            block_lines.push("[sources.params]".to_string());
            block_lines.push(format!("addr = \"{}\"", RUNTIME_SOURCE_ADDR));
            block_lines.push(format!("port = {port}"));
            block_lines.push(format!("protocol = \"{}\"", RUNTIME_PROTOCOL));
            block_lines.push(format!("header_mode = \"{}\"", RUNTIME_HEADER_MODE));
            *patched_connect = true;
            *patched_port = true;
        } else {
            patch_or_insert_source_enable(block_lines, block_enable_index, *block_key_index, false);
        }

        lines.append(block_lines);
        *block_key_value = None;
        *block_key_index = None;
        *block_enable_index = None;
        *block_connect_index = None;
        *block_port_index = None;
        *block_has_params_section = false;
    };

    for line in content.lines() {
        let trimmed = line.trim();
        let is_array_section = trimmed.starts_with("[[") && trimmed.ends_with("]]");
        let is_section = !is_array_section && trimmed.starts_with('[') && trimmed.ends_with(']');

        if is_array_section {
            if in_sources_block {
                flush_source_block(
                    &mut lines,
                    &mut block_lines,
                    &mut block_key_value,
                    &mut block_key_index,
                    &mut block_enable_index,
                    &mut block_connect_index,
                    &mut block_port_index,
                    &mut block_has_params_section,
                    &mut found_target,
                    &mut patched_connect,
                    &mut patched_port,
                );
            }

            in_sources_block = trimmed == "[[sources]]";
            in_source_params = false;
            if in_sources_block {
                block_lines.push(line.to_string());
            } else {
                lines.push(line.to_string());
            }
            continue;
        }

        if in_sources_block {
            if is_section {
                in_source_params = trimmed == "[sources.params]";
                if in_source_params {
                    block_has_params_section = true;
                }
            }

            if block_key_value.is_none()
                && let Some(value) = parse_toml_string_assignment(trimmed, "key")
            {
                block_key_value = Some(value);
            }

            if !in_source_params && trimmed.starts_with("key") {
                block_key_index = Some(block_lines.len());
            }

            if !in_source_params && trimmed.starts_with("enable") {
                block_enable_index = Some(block_lines.len());
            }

            if !in_source_params && trimmed.starts_with("connect") {
                block_connect_index = Some(block_lines.len());
            }

            if in_source_params && trimmed.starts_with("port") {
                block_port_index = Some(block_lines.len());
            }

            block_lines.push(line.to_string());
            continue;
        }

        lines.push(line.to_string());
    }

    if in_sources_block {
        flush_source_block(
            &mut lines,
            &mut block_lines,
            &mut block_key_value,
            &mut block_key_index,
            &mut block_enable_index,
            &mut block_connect_index,
            &mut block_port_index,
            &mut block_has_params_section,
            &mut found_target,
            &mut patched_connect,
            &mut patched_port,
        );
    }

    if !found_target {
        if !lines.last().is_none_or(|line| line.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push("[[sources]]".to_string());
        lines.push(format!("key = \"{}\"", RUNTIME_SOURCE_KEY));
        lines.push("enable = true".to_string());
        lines.push(format!("connect = \"{connect}\""));
        lines.push("tags = []".to_string());
        lines.push(String::new());
        lines.push("[sources.params]".to_string());
        lines.push(format!("addr = \"{}\"", RUNTIME_SOURCE_ADDR));
        lines.push(format!("port = {port}"));
        lines.push(format!("protocol = \"{}\"", RUNTIME_PROTOCOL));
        lines.push(format!("header_mode = \"{}\"", RUNTIME_HEADER_MODE));
        patched_connect = true;
        patched_port = true;
    }

    if !patched_connect {
        return Err(AppError::validation(
            "wpsrc.toml 未能应用沙盒输入 connector 覆盖".to_string(),
        ));
    }

    if !patched_port {
        return Err(AppError::validation(
            "wpsrc.toml 未能应用沙盒输入端口覆盖".to_string(),
        ));
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// 计算 source 级别字段的默认插入位置，优先放在 `key` 后面。
fn source_block_insert_after_key(block_lines: &[String], key_index: Option<usize>) -> usize {
    key_index
        .map(|index| index.saturating_add(1))
        .unwrap_or(1)
        .min(block_lines.len())
}

/// 将 source block 的 `enable` 字段改写为指定值；若不存在则插入。
fn patch_or_insert_source_enable(
    block_lines: &mut Vec<String>,
    block_enable_index: &mut Option<usize>,
    block_key_index: Option<usize>,
    enabled: bool,
) {
    if let Some(index) = *block_enable_index
        && let Some(line) = block_lines.get(index)
    {
        let indent = line
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .collect::<String>();
        block_lines[index] = format!("{indent}enable = {enabled}");
        return;
    }

    let insert_at = source_block_insert_after_key(block_lines, block_key_index);
    block_lines.insert(insert_at, format!("enable = {enabled}"));
    *block_enable_index = Some(insert_at);
}

/// 在保留原格式与注释的前提下，仅更新 wpgen.toml 的输出 connector、addr 和端口。
fn patch_wpgen_output_runtime(
    content: &str,
    connect: &str,
    addr: &str,
    port: u16,
) -> Result<String, AppError> {
    let mut lines = Vec::new();
    let mut in_output = false;
    let mut in_output_params = false;
    let mut found_output = false;
    let mut found_output_params = false;
    let mut patched_connect = false;
    let mut rewritten_output_params = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_section = trimmed.starts_with('[') && trimmed.ends_with(']');

        if in_output_params && !is_section {
            continue;
        }

        if is_section {
            in_output = trimmed == "[output]";
            in_output_params = trimmed == "[output.params]";
            found_output |= in_output;
            found_output_params |= in_output_params;
            if in_output_params {
                lines.push(line.to_string());
                lines.push(format!("addr = \"{addr}\""));
                lines.push(format!("port = {port}"));
                rewritten_output_params = true;
                continue;
            }
        }

        if in_output && trimmed.starts_with("connect") {
            let indent = line
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .collect::<String>();
            lines.push(format!("{indent}connect = \"{connect}\""));
            patched_connect = true;
            continue;
        }

        lines.push(line.to_string());
    }

    if !found_output || !patched_connect {
        return Err(AppError::validation(
            "wpgen.toml 缺少 [output] 或 connect 配置，无法应用沙盒输出覆盖".to_string(),
        ));
    }

    if !found_output_params || !rewritten_output_params {
        return Err(AppError::validation(
            "wpgen.toml 缺少 [output.params] 或 addr/port 配置，无法应用沙盒输出覆盖".to_string(),
        ));
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// 在保留原配置主体的前提下，将 wfusion 业务 sink 改写为本地 file_json 输出。
fn patch_wfusion_business_sink_runtime(content: &str) -> Result<String, AppError> {
    let mut lines = Vec::new();
    let mut in_sink = false;
    let mut in_sink_params = false;
    let mut found_sink = false;
    let mut found_sink_params = false;
    let mut patched_connect = false;
    let mut rewritten_params = false;

    for line in content.lines() {
        let trimmed = line.trim();
        let is_array_section = trimmed.starts_with("[[") && trimmed.ends_with("]]");
        let is_section = !is_array_section && trimmed.starts_with('[') && trimmed.ends_with(']');

        if in_sink_params && !is_section && !is_array_section {
            continue;
        }

        if is_array_section {
            in_sink = trimmed == "[[sink_group.sinks]]";
            in_sink_params = false;
            found_sink |= in_sink;
            lines.push(line.to_string());
            continue;
        }

        if is_section {
            in_sink_params = trimmed == "[sink_group.sinks.params]";
            found_sink_params |= in_sink_params;
            if in_sink_params {
                lines.push(line.to_string());
                lines.push("file = \"alert.json\"".to_string());
                rewritten_params = true;
                continue;
            }
        }

        if in_sink && !in_sink_params && trimmed.starts_with("connect") {
            let indent = line
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .collect::<String>();
            lines.push(format!("{indent}connect = \"file_json_sink\""));
            patched_connect = true;
            continue;
        }

        lines.push(line.to_string());
    }

    if !found_sink || !patched_connect {
        return Err(AppError::validation(
            "wfusion 业务 sink 缺少 [[sink_group.sinks]] 或 connect 配置，无法应用沙盒输出覆盖"
                .to_string(),
        ));
    }

    if !found_sink_params || !rewritten_params {
        return Err(AppError::validation(
            "wfusion 业务 sink 缺少 [sink_group.sinks.params] 配置，无法应用沙盒输出覆盖"
                .to_string(),
        ));
    }

    let mut output = lines.join("\n");
    if content.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// 解析形如 `key = "value"` 的 TOML 字符串赋值。
fn parse_toml_string_assignment(line: &str, key: &str) -> Option<String> {
    let value = line.strip_prefix(key)?.trim_start();
    let value = value.strip_prefix('=')?.trim_start();
    let value = value.strip_prefix('"')?;
    let end = value.find('"')?;
    Some(value[..end].to_string())
}

/// 校验用户提交的文件覆盖路径为 project_root 内的安全相对路径，
/// 防止路径穿越攻击。
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

/// 将路径转换为相对于工作区根目录的路径，超出根目录时返回 None。
fn relative_to_workspace_root(path: &Path) -> Option<PathBuf> {
    path.strip_prefix(Setting::workspace_root())
        .map(|p| p.to_path_buf())
        .ok()
}

/// 历史沙盒仅保留最近 3 次的 project 目录，所有任务的 logs 目录均保留。
fn prune_sandbox_runtime_artifacts(sandbox_root: &Path) -> Result<(), AppError> {
    if !sandbox_root.exists() {
        return Ok(());
    }

    let mut workspaces = list_sandbox_workspaces(sandbox_root)?;
    if workspaces.len() <= RUNTIME_ARTIFACT_RETENTION_RUNS {
        return Ok(());
    }

    workspaces.sort_by(|left, right| right.cmp(left));
    for workspace in workspaces.into_iter().skip(RUNTIME_ARTIFACT_RETENTION_RUNS) {
        prune_workspace_runtime_artifacts(&workspace.path)?;
    }
    Ok(())
}

/// 列出当前沙盒目录下所有任务工作区，用于历史运行裁剪。
fn list_sandbox_workspaces(sandbox_root: &Path) -> Result<Vec<SandboxWorkspaceEntry>, AppError> {
    let mut workspaces = Vec::new();
    for entry in fs::read_dir(sandbox_root).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().to_string();
        workspaces.push(SandboxWorkspaceEntry {
            sort_key: sandbox_workspace_sort_key(&name, &path)?,
            path,
        });
    }
    Ok(workspaces)
}

/// 生成工作区排序键，优先按 task_id 中的时间戳，再按文件修改时间排序。
fn sandbox_workspace_sort_key(
    task_id: &str,
    workspace_dir: &Path,
) -> Result<SandboxWorkspaceSortKey, AppError> {
    let modified_ms = fs::metadata(workspace_dir)
        .map_err(AppError::internal)?
        .modified()
        .map_err(AppError::internal)?
        .duration_since(UNIX_EPOCH)
        .map_err(AppError::internal)?
        .as_millis();

    Ok(SandboxWorkspaceSortKey {
        task_timestamp_ms: parse_sandbox_task_timestamp(task_id).unwrap_or_default(),
        modified_ms,
        task_id: task_id.to_string(),
    })
}

/// 从新格式 `sandbox-<system>-<timestamp>-<suffix>` 或旧格式
/// `sandbox-<timestamp>-<suffix>` 的任务 ID 中提取时间戳。
fn parse_sandbox_task_timestamp(task_id: &str) -> Option<i64> {
    let mut segments = task_id.split('-');
    if segments.next()? != "sandbox" {
        return None;
    }
    let second = segments.next()?;
    if let Ok(timestamp) = second.parse::<i64>() {
        return Some(timestamp);
    }
    segments.next()?.parse::<i64>().ok()
}

/// 删除历史工作区中的 project 目录，保留所有阶段日志供长期回看。
fn prune_workspace_runtime_artifacts(workspace_dir: &Path) -> Result<(), AppError> {
    let project_dir = workspace_dir.join("project");
    if project_dir.exists() {
        fs::remove_dir_all(&project_dir).map_err(AppError::internal)?;
    }
    Ok(())
}

/// 工作区清理排序键。
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct SandboxWorkspaceSortKey {
    task_timestamp_ms: i64,
    modified_ms: u128,
    task_id: String,
}

/// 待清理工作区条目。
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct SandboxWorkspaceEntry {
    sort_key: SandboxWorkspaceSortKey,
    path: PathBuf,
}

/// 渲染目录树结构文本，深度和条目数有上限防止输出过大。
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

/// 递归构建树状目录的每一行输出。
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
