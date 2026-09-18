//! 沙盒命令执行与进程管理。

use std::fs::File;
use std::io::Write;
use std::net::{TcpListener, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::constants::sandbox::WFUSION_RUNTIME_TCP_PORT;
use crate::error::AppError;
use crate::utils::SystemKind;

/// 命令查找的优先搜索路径，沙盒环境中的 toolchain 安装目录。
const TOOLCHAIN_SEARCH_PATHS: [&str; 2] = ["/app", "/app/toolchain"];

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

/// 对 wparse daemon 进程的轻量封装，方便查询与终止。
pub struct DaemonProcess {
    child: Child,
    log_path: PathBuf,
}

/// 生成器命令执行的输出摘要。
pub struct GeneratorOutput {
    /// 进程退出码，正常退出为 Some(0)。多场景时为首个非零退出码。
    pub exit_code: Option<i32>,
    /// 输出日志文件路径。
    pub log_path: PathBuf,
    /// 实际执行的所有命令，供阶段日志展示。
    pub command_lines: Vec<String>,
}

impl DaemonProcess {
    /// 创建新的进程句柄。
    pub fn new(child: Child, log_path: PathBuf) -> Self {
        Self { child, log_path }
    }

    /// 等待日志中出现指定标记，常用于探测 daemon 是否就绪。
    /// 超时或进程提前退出时返回错误。
    pub async fn wait_for_marker(
        &mut self,
        marker: &str,
        timeout: Duration,
    ) -> Result<(), AppError> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().map_err(AppError::internal)? {
                return Err(AppError::internal(format!(
                    "daemon 已退出，状态: {}",
                    status
                )));
            }

            if read_file_contains(&self.log_path, marker).await? {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(AppError::internal("等待 daemon 就绪超时"));
            }

            sleep(Duration::from_millis(500)).await;
        }
    }

    /// 终止进程，先尝试 SIGTERM（Unix）再 wait 回收。
    /// 终止失败不影响流程。
    pub async fn terminate(mut self) -> Result<(), AppError> {
        if let Some(id) = self.child.id() {
            #[cfg(unix)]
            {
                use nix::sys::signal::{Signal, kill};
                use nix::unistd::Pid;

                let _ = kill(Pid::from_raw(id as i32), Signal::SIGTERM);
            }
            #[cfg(windows)]
            {
                let _ = self.child.start_kill();
            }
        }

        let _ = self.child.wait().await;
        Ok(())
    }
}

/// 启动系统对应的 daemon 并将 stdout/stderr 重定向到日志文件。
pub async fn spawn_daemon(
    system: SystemKind,
    project_dir: &Path,
    log_path: &Path,
) -> Result<DaemonProcess, AppError> {
    let daemon_cmd = daemon_command(system);
    let binary = resolve_toolchain_command(daemon_cmd);
    let mut cmd = Command::new(&binary);
    let command_line = match system {
        SystemKind::Wparse => format!("{} daemon", binary.display()),
        SystemKind::Wfusion => format!("{} daemon --work-dir .", binary.display()),
    };
    let log_file = File::create(log_path).map_err(AppError::internal)?;
    writeln!(&log_file, "执行命令: {}", command_line).map_err(AppError::internal)?;
    writeln!(&log_file).map_err(AppError::internal)?;
    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;
    cmd.arg("daemon")
        .current_dir(project_dir)
        .stdout(stdout)
        .stderr(stderr);
    if matches!(system, SystemKind::Wfusion) {
        cmd.args(["--work-dir", "."]);
    }

    let child = cmd.spawn().map_err(|err| {
        AppError::internal(format!(
            "启动 {} daemon 失败: {}。请检查可执行文件 {} 是否可用",
            daemon_cmd,
            err,
            binary.display()
        ))
    })?;

    Ok(DaemonProcess::new(child, log_path.to_path_buf()))
}

/// 执行系统对应的生成器，并将输出写入日志文件。
/// 支持超时控制，超时后返回错误。
///
/// 对于 Wfusion：遍历 `models/scenarios` 下所有 `.wfg` 场景文件，
/// 对每个场景依次执行 wfgen，输出追加到同一日志。
pub async fn run_generator(
    system: SystemKind,
    project_dir: &Path,
    log_path: &Path,
    sample_count: u32,
    timeout: Duration,
) -> Result<GeneratorOutput, AppError> {
    let generator_cmd = generator_command(system);
    let binary = resolve_toolchain_command(generator_cmd);

    match system {
        SystemKind::Wparse => {
            run_generator_wparse(
                &binary,
                generator_cmd,
                project_dir,
                log_path,
                sample_count,
                timeout,
            )
            .await
        }
        SystemKind::Wfusion => {
            let scenarios = collect_wfusion_scenarios(project_dir)?;
            run_generator_wfusion(
                &binary,
                generator_cmd,
                project_dir,
                log_path,
                &scenarios,
                timeout,
            )
            .await
        }
    }
}

/// 执行 wpgen 采样生成。
async fn run_generator_wparse(
    binary: &Path,
    generator_cmd: &str,
    project_dir: &Path,
    log_path: &Path,
    sample_count: u32,
    timeout: Duration,
) -> Result<GeneratorOutput, AppError> {
    let command_line = format!(
        "{} sample -w . -n {} --print_stat",
        binary.display(),
        sample_count
    );
    let log_file = File::create(log_path).map_err(AppError::internal)?;
    writeln!(&log_file, "执行命令: {}", command_line).map_err(AppError::internal)?;
    writeln!(&log_file).map_err(AppError::internal)?;
    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;

    let mut cmd = Command::new(binary);
    cmd.args([
        "sample",
        "-w",
        ".",
        "-n",
        &sample_count.to_string(),
        "--print_stat",
    ]);
    cmd.current_dir(project_dir).stdout(stdout).stderr(stderr);

    let mut child = cmd.spawn().map_err(|err| {
        AppError::internal(format!(
            "执行 {} 失败: {}。请确认可执行文件 {} 是否可用",
            generator_cmd,
            err,
            binary.display()
        ))
    })?;

    let status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| AppError::internal(format!("{} 运行超时", generator_cmd)))?
        .map_err(AppError::internal)?;

    Ok(GeneratorOutput {
        exit_code: status.code(),
        log_path: log_path.to_path_buf(),
        command_lines: vec![command_line],
    })
}

/// 对每个场景文件依次执行 wfgen，输出追加到同一日志。
async fn run_generator_wfusion(
    binary: &Path,
    generator_cmd: &str,
    project_dir: &Path,
    log_path: &Path,
    scenarios: &[PathBuf],
    timeout: Duration,
) -> Result<GeneratorOutput, AppError> {
    let log_file = File::create(log_path).map_err(AppError::internal)?;
    let runtime_addr = format!("127.0.0.1:{WFUSION_RUNTIME_TCP_PORT}");

    let mut command_lines = Vec::with_capacity(scenarios.len());
    let mut final_exit_code: Option<i32> = None;

    for scenario in scenarios {
        let command_line = format!(
            "{} gen --scenario {} --send --addr 127.0.0.1:{} --no-oracle",
            binary.display(),
            scenario.display(),
            WFUSION_RUNTIME_TCP_PORT
        );
        command_lines.push(command_line.clone());

        // 场景之间的分隔，便于日志分析。
        writeln!(&log_file, "--- 场景: {}", scenario.display()).map_err(AppError::internal)?;
        writeln!(&log_file, "执行命令: {}", command_line).map_err(AppError::internal)?;
        writeln!(&log_file).map_err(AppError::internal)?;

        let stdout = log_file.try_clone().map_err(AppError::internal)?;
        let stderr = log_file.try_clone().map_err(AppError::internal)?;

        let mut cmd = Command::new(binary);
        cmd.arg("gen")
            .arg("--scenario")
            .arg(scenario)
            .arg("--send")
            .arg("--addr")
            .arg(&runtime_addr)
            .arg("--no-oracle");
        cmd.current_dir(project_dir).stdout(stdout).stderr(stderr);

        let mut child = cmd.spawn().map_err(|err| {
            AppError::internal(format!(
                "执行 {} 失败: {}。请确认可执行文件 {} 是否可用",
                generator_cmd,
                err,
                binary.display()
            ))
        })?;

        let status = tokio::time::timeout(timeout, child.wait())
            .await
            .map_err(|_| AppError::internal(format!("{} 运行超时", generator_cmd)))?
            .map_err(AppError::internal)?;

        // 记录首个非零退出码，否则使用最后一次的状态。
        let code = status.code();
        if code.unwrap_or(0) != 0 && final_exit_code.is_none() {
            final_exit_code = code;
        }
        if final_exit_code.is_none() {
            final_exit_code = code;
        }

        writeln!(&log_file).map_err(AppError::internal)?;
    }

    Ok(GeneratorOutput {
        exit_code: final_exit_code,
        log_path: log_path.to_path_buf(),
        command_lines,
    })
}

/// 运行 `<cmd> --version`，返回 stdout/stderr 中非空内容，优先返回 stdout。
pub async fn command_version_output(cmd: &str) -> Result<String, AppError> {
    let binary = resolve_toolchain_command(cmd);
    let output = Command::new(&binary)
        .arg("--version")
        .output()
        .await
        .map_err(|err| {
            AppError::internal(format!("{} --version 执行失败: {}", binary.display(), err))
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            Ok(stdout)
        } else {
            Ok(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(AppError::internal(format!(
            "{} --version 返回非 0 状态: {}",
            binary.display(),
            if stderr.is_empty() {
                "未提供错误输出".to_string()
            } else {
                stderr
            }
        )))
    }
}

/// 检查指定 UDP 端口当前是否可绑定，用于预先识别残留进程占用。
pub fn ensure_udp_port_available(port: u16) -> Result<(), AppError> {
    let bind_addr = format!("0.0.0.0:{port}");
    UdpSocket::bind(&bind_addr)
        .map_err(|err| AppError::validation(format!("UDP 端口 {} 不可用: {}", port, err)))?;
    Ok(())
}

/// 检查指定 TCP 端口当前是否可绑定，用于预先识别残留进程占用。
pub fn ensure_tcp_port_available(port: u16) -> Result<(), AppError> {
    let bind_addr = format!("0.0.0.0:{port}");
    TcpListener::bind(&bind_addr)
        .map_err(|err| AppError::validation(format!("TCP 端口 {} 不可用: {}", port, err)))?;
    Ok(())
}

/// 在指定目录执行 `<admin_cmd> check` 并返回输出。
async fn run_admin_check(project_dir: &Path, admin_cmd: &str) -> Result<String, AppError> {
    let binary = resolve_toolchain_command(admin_cmd);
    let output = Command::new(&binary)
        .arg("check")
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|err| {
            AppError::internal(format!(
                "执行 {} check 失败 ({}): {}",
                admin_cmd,
                binary.display(),
                err
            ))
        })?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(AppError::internal(format!(
            "{} check 返回非 0 状态 ({}): {}",
            admin_cmd,
            binary.display(),
            if stderr.is_empty() {
                "未提供错误输出"
            } else {
                &stderr
            }
        )))
    }
}

/// 在指定目录执行 `wpadm check` 并返回输出。
pub async fn run_wpadm_check(project_dir: &Path) -> Result<String, AppError> {
    run_admin_check(project_dir, "wpadm").await
}

/// 在指定目录执行 `wfadm check` 并返回输出。
pub async fn run_wfadm_check(project_dir: &Path) -> Result<String, AppError> {
    run_admin_check(project_dir, "wfadm").await
}

/// 读取文件内容并检查是否包含指定字符串，文件不存在时返回 false。
async fn read_file_contains(path: &Path, needle: &str) -> Result<bool, AppError> {
    if !path.exists() {
        return Ok(false);
    }

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(AppError::internal)?;
    let mut buf = String::new();
    file.read_to_string(&mut buf)
        .await
        .map_err(AppError::internal)?;
    Ok(buf.contains(needle))
}

/// 返回系统对应的 daemon 二进制名称。
pub fn daemon_command(system: SystemKind) -> &'static str {
    match system {
        SystemKind::Wparse => "wparse",
        SystemKind::Wfusion => "wfusion",
    }
}

/// 返回系统对应的生成器二进制名称。
pub fn generator_command(system: SystemKind) -> &'static str {
    match system {
        SystemKind::Wparse => "wpgen",
        SystemKind::Wfusion => "wfgen",
    }
}

/// 返回系统对应的 daemon 就绪日志标记。
pub fn daemon_ready_marker(system: SystemKind) -> &'static str {
    match system {
        SystemKind::Wparse => "engine started",
        SystemKind::Wfusion => "WarpFusion reactor started",
    }
}

/// 查找沙盒内所有 `.wfg` 场景文件，按文件名排序。
pub fn collect_wfusion_scenarios(project_dir: &Path) -> Result<Vec<PathBuf>, AppError> {
    let scenarios_dir = project_dir.join("models/scenarios");
    let mut files = Vec::new();
    collect_scenario_files(&scenarios_dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(AppError::validation(
            "未找到 wfusion 场景文件，请在 models/scenarios 下提供 .wfg 文件",
        ));
    }
    Ok(files)
}

/// 查找沙盒内首个可用的 `.wfg` 场景文件。
pub fn find_wfusion_scenario(project_dir: &Path) -> Result<PathBuf, AppError> {
    let mut files = collect_wfusion_scenarios(project_dir)?;
    // `collect_wfusion_scenarios` 已保证至少有一个文件，且已排序。
    Ok(files.swap_remove(0))
}

fn collect_scenario_files(dir: &Path, acc: &mut Vec<PathBuf>) -> Result<(), AppError> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let path = entry.path();
        if path.is_dir() {
            collect_scenario_files(&path, acc)?;
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("wfg"))
        {
            acc.push(path);
        }
    }

    Ok(())
}
