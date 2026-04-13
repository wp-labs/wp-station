use std::fs::File;
use std::net::UdpSocket;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::time::sleep;

use crate::error::AppError;

const TOOLCHAIN_SEARCH_PATHS: [&str; 2] = ["/app", "/app/toolchain"];

/// 对 wparse daemon 进程的轻量封装，方便查询与终止。
pub struct DaemonProcess {
    child: Child,
    log_path: PathBuf,
}

/// wpgen 命令执行的输出摘要。
pub struct WpgenOutput {
    pub exit_code: Option<i32>,
    pub log_path: PathBuf,
}

impl DaemonProcess {
    /// 创建新的进程句柄。
    pub fn new(child: Child, log_path: PathBuf) -> Self {
        Self { child, log_path }
    }

    /// 返回日志文件路径。
    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// 等待日志中出现指定标记，常用于探测 daemon 是否就绪。
    pub async fn wait_for_marker(
        &mut self,
        marker: &str,
        timeout: Duration,
    ) -> Result<(), AppError> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().map_err(AppError::internal)? {
                return Err(AppError::internal(format!(
                    "wparse daemon 已退出，状态: {}",
                    status
                )));
            }

            if read_file_contains(&self.log_path, marker).await? {
                return Ok(());
            }

            if start.elapsed() > timeout {
                return Err(AppError::internal("等待 wparse daemon 就绪超时"));
            }

            sleep(Duration::from_millis(500)).await;
        }
    }

    /// 终止进程，忽略终止失败的错误。
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

/// 启动 wparse daemon 并将 stdout/stderr 重定向到日志文件。
pub async fn spawn_daemon(project_dir: &Path, log_path: &Path) -> Result<DaemonProcess, AppError> {
    let binary = resolve_toolchain_command("wparse");
    let mut cmd = Command::new(&binary);
    let log_file = File::create(log_path).map_err(AppError::internal)?;
    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;
    cmd.arg("daemon")
        .current_dir(project_dir)
        .stdout(stdout)
        .stderr(stderr);

    let child = cmd.spawn().map_err(|err| {
        AppError::internal(format!(
            "启动 wparse daemon 失败: {}。请检查可执行文件 {} 是否可用",
            err,
            binary.display()
        ))
    })?;

    Ok(DaemonProcess::new(child, log_path.to_path_buf()))
}

/// 执行 `wpgen sample`，并将输出写入日志文件。
pub async fn run_wpgen(
    project_dir: &Path,
    log_path: &Path,
    sample_count: u32,
    timeout: Duration,
) -> Result<WpgenOutput, AppError> {
    let binary = resolve_toolchain_command("wpgen");
    let mut cmd = Command::new(&binary);
    let log_file = File::create(log_path).map_err(AppError::internal)?;
    let stdout = log_file.try_clone().map_err(AppError::internal)?;
    let stderr = log_file.try_clone().map_err(AppError::internal)?;
    cmd.args([
        "sample",
        "-w",
        ".",
        "-n",
        &sample_count.to_string(),
        "--print_stat",
    ])
    .current_dir(project_dir)
    .stdout(stdout)
    .stderr(stderr);

    let mut child = cmd.spawn().map_err(|err| {
        AppError::internal(format!(
            "执行 wpgen sample 失败: {}。请确认可执行文件 {} 是否可用",
            err,
            binary.display()
        ))
    })?;

    let status = tokio::time::timeout(timeout, child.wait())
        .await
        .map_err(|_| AppError::internal("wpgen 运行超时"))?
        .map_err(AppError::internal)?;

    Ok(WpgenOutput {
        exit_code: status.code(),
        log_path: log_path.to_path_buf(),
    })
}

/// 校验命令是否存在于 PATH。
pub fn check_command_exists(cmd: &str) -> Result<(), AppError> {
    which::which(cmd)
        .map(|_| ())
        .map_err(|_| AppError::validation(format!("未找到可执行命令: {}", cmd)))
}

/// 运行 `<cmd> --version`，返回 stdout/stderr。
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

/// 在指定目录执行 `wproj check` 并返回输出。
pub async fn run_wproj_check(project_dir: &Path) -> Result<String, AppError> {
    let binary = resolve_toolchain_command("wproj");
    let output = Command::new(&binary)
        .arg("check")
        .current_dir(project_dir)
        .output()
        .await
        .map_err(|err| {
            AppError::internal(format!(
                "执行 wproj check 失败 ({}): {}",
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
            "wproj check 返回非 0 状态 ({}): {}",
            binary.display(),
            if stderr.is_empty() {
                "未提供错误输出"
            } else {
                &stderr
            }
        )))
    }
}

fn resolve_toolchain_command(cmd: &str) -> PathBuf {
    for base in TOOLCHAIN_SEARCH_PATHS {
        let candidate = Path::new(base).join(cmd);
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from(cmd)
}

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
