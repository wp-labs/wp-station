//! 沙盒命令执行与进程管理。

use std::fs::{self, File};
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
use crate::utils::sandbox::wfusion_source_overlay_path;

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
        SystemKind::Wfusion => format!(
            "{} daemon -c conf/wfusion.toml --overlay {}",
            binary.display(),
            wfusion_source_overlay_path(project_dir).display()
        ),
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
        let overlay_path = wfusion_source_overlay_path(project_dir);
        if !overlay_path.is_file() {
            return Err(AppError::validation(format!(
                "wfusion 沙盒缺少 source overlay: {}",
                overlay_path.display()
            )));
        }
        cmd.args(["-c", "conf/wfusion.toml"])
            .arg("--overlay")
            .arg(overlay_path);
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
        run_wfgen_lint(binary, project_dir, &log_file, scenario, timeout).await?;

        if is_sdm_event_sample(scenario) {
            let (code, commands) = run_sdm_event_sample(
                binary,
                project_dir,
                &log_file,
                scenario,
                &runtime_addr,
                timeout,
            )
            .await?;
            command_lines.extend(commands);
            if code.unwrap_or(0) != 0 && final_exit_code.is_none() {
                final_exit_code = code;
            }
            if final_exit_code.is_none() {
                final_exit_code = code;
            }
            continue;
        }

        let command_line = format!(
            "{} gen --scenario {} --send --addr 127.0.0.1:{}",
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
            .arg(&runtime_addr);
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

/// 判断是否为当前自包含的五条样例场景。
fn is_sdm_event_sample(scenario: &Path) -> bool {
    scenario.file_stem().and_then(|stem| stem.to_str()) == Some("sdm_event")
}

/// 生成 `sdm_event` 后过滤背景流，再把五条注入样例发送给 wfusion。
///
/// `wfadm check` 要求 WFG 保留合法的 `background` 块，不能直接删除该块。
/// 因此先用完整场景生成 JSONL，再按生成器写入的 `hit_event_id_*` 标记过滤掉
/// 背景事件，最后调用 `wfgen send` 发送过滤后的五条数据。
async fn run_sdm_event_sample(
    binary: &Path,
    project_dir: &Path,
    mut log_file: &File,
    scenario: &Path,
    runtime_addr: &str,
    timeout: Duration,
) -> Result<(Option<i32>, Vec<String>), AppError> {
    let output_dir = project_dir.join(".wfgen-sandbox-output");
    fs::create_dir_all(&output_dir).map_err(AppError::internal)?;
    let stem = scenario
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("sdm_event");
    let generated_path = output_dir.join(format!("{stem}.jsonl"));
    let filtered_path = output_dir.join(format!("{stem}.injected.jsonl"));
    let generation_log_path = output_dir.join(format!("{stem}.generation.log"));

    let generate_command = format!(
        "{} gen --scenario {} --out {}",
        binary.display(),
        scenario.display(),
        output_dir.display()
    );
    writeln!(log_file, "--- 场景: {}", scenario.display()).map_err(AppError::internal)?;
    writeln!(log_file, "执行命令: {generate_command}").map_err(AppError::internal)?;
    writeln!(log_file).map_err(AppError::internal)?;

    let generation_log = File::create(&generation_log_path).map_err(AppError::internal)?;
    let stdout = generation_log.try_clone().map_err(AppError::internal)?;
    let stderr = generation_log.try_clone().map_err(AppError::internal)?;
    let mut generate = Command::new(binary);
    generate
        .arg("gen")
        .arg("--scenario")
        .arg(scenario)
        .arg("--out")
        .arg(&output_dir)
        .current_dir(project_dir)
        .stdout(stdout)
        .stderr(stderr);
    let mut child = generate.spawn().map_err(|err| {
        AppError::internal(format!(
            "执行 wfgen 生成阶段失败: {}。请确认可执行文件 {} 是否可用",
            err,
            binary.display()
        ))
    })?;
    let generation_status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| AppError::internal("wfgen 生成阶段运行超时"))?
        .map_err(AppError::internal)?;
    if !generation_status.success() {
        append_file_to_log(&generation_log_path, log_file)?;
        return Err(AppError::validation(format!(
            "wfgen 生成阶段失败: {}",
            scenario.display()
        )));
    }

    let (total, injected) = filter_sdm_event_injected_events(&generated_path, &filtered_path)?;
    let input_path = if injected > 0 && injected < total {
        writeln!(
            log_file,
            "已过滤背景事件: 共生成{}条，保留{}条注入样例",
            total, injected
        )
        .map_err(AppError::internal)?;
        filtered_path
    } else {
        generated_path
    };
    let send_command = format!(
        "{} send --scenario {} --input {} --addr {}",
        binary.display(),
        scenario.display(),
        input_path.display(),
        runtime_addr
    );
    writeln!(log_file, "执行命令: {send_command}").map_err(AppError::internal)?;
    writeln!(log_file).map_err(AppError::internal)?;

    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;
    let mut send = Command::new(binary);
    send.arg("send")
        .arg("--scenario")
        .arg(scenario)
        .arg("--input")
        .arg(&input_path)
        .arg("--addr")
        .arg(runtime_addr)
        .current_dir(project_dir)
        .stdout(stdout)
        .stderr(stderr);
    let mut child = send.spawn().map_err(|err| {
        AppError::internal(format!(
            "执行 wfgen 发送阶段失败: {}。请确认可执行文件 {} 是否可用",
            err,
            binary.display()
        ))
    })?;
    let send_status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| AppError::internal("wfgen 发送阶段运行超时"))?
        .map_err(AppError::internal)?;
    if !send_status.success() {
        return Err(AppError::validation(format!(
            "wfgen 发送阶段失败: {}",
            scenario.display()
        )));
    }

    writeln!(log_file).map_err(AppError::internal)?;
    Ok((send_status.code(), vec![generate_command, send_command]))
}

/// 过滤 wfgen 为自包含样例生成的背景事件。
fn filter_sdm_event_injected_events(
    generated_path: &Path,
    filtered_path: &Path,
) -> Result<(usize, usize), AppError> {
    let content = fs::read_to_string(generated_path).map_err(|err| {
        AppError::internal(format!(
            "读取 wfgen 生成文件 {} 失败: {}",
            generated_path.display(),
            err
        ))
    })?;
    let mut total = 0usize;
    let mut injected_lines = Vec::new();
    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        total += 1;
        let event_id = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|event| {
                event
                    .get("event_id")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            });
        if event_id
            .as_deref()
            .is_some_and(|value| value.starts_with("hit_event_id_"))
        {
            injected_lines.push(line);
        }
    }

    if total == 0 {
        return Err(AppError::validation(format!(
            "wfgen 生成文件为空: {}",
            generated_path.display()
        )));
    }
    let mut filtered = injected_lines.join("\n");
    if !filtered.is_empty() {
        filtered.push('\n');
    }
    fs::write(filtered_path, filtered).map_err(AppError::internal)?;
    Ok((total, injected_lines.len()))
}

fn append_file_to_log(path: &Path, mut log_file: &File) -> Result<(), AppError> {
    let content = fs::read_to_string(path).unwrap_or_default();
    if !content.is_empty() {
        writeln!(log_file, "{content}").map_err(AppError::internal)?;
    }
    Ok(())
}

/// 发送场景前先执行 wfgen lint，避免把场景编译错误延迟到运行时才暴露。
async fn run_wfgen_lint(
    binary: &Path,
    project_dir: &Path,
    mut log_file: &File,
    scenario: &Path,
    timeout: Duration,
) -> Result<(), AppError> {
    let command_line = format!("{} lint {}", binary.display(), scenario.display());
    writeln!(log_file, "执行命令: {}", command_line).map_err(AppError::internal)?;
    writeln!(log_file).map_err(AppError::internal)?;

    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;
    let mut child = Command::new(binary);
    child
        .arg("lint")
        .arg(scenario)
        .current_dir(project_dir)
        .stdout(stdout)
        .stderr(stderr);

    let mut child = child.spawn().map_err(|err| {
        AppError::internal(format!(
            "执行 wfgen lint 失败: {}。请确认可执行文件 {} 可用",
            err,
            binary.display()
        ))
    })?;
    let status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| AppError::internal(format!("{} lint 运行超时", binary.display())))?
        .map_err(AppError::internal)?;
    if !status.success() {
        return Err(AppError::validation(format!(
            "wfgen lint 失败: {}",
            scenario.display()
        )));
    }

    Ok(())
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
