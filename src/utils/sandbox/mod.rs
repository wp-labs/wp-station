//! 沙盒运行时管理模块。
//!
//! **沙盒内部专用模块，不允许被其他 API 或业务功能直接调用。**
//! 所有 `pub` 项仅供 `src/server/sandbox*.rs` 使用。
//!
//! 当前按职责拆成两部分：
//! - `workspace`：沙盒目录、文件覆盖、运行产物裁剪
//! - `process`：wparse / wpgen / wpadm / wfusion / wfgen 命令执行与端口检查

mod process;
mod workspace;

pub use self::process::{
    DaemonProcess, GeneratorOutput, collect_wfusion_scenarios, command_version_output,
    daemon_command, daemon_ready_marker, ensure_tcp_port_available, ensure_udp_port_available,
    find_wfusion_scenario, generator_command, run_generator, run_wfadm_check, run_wpadm_check,
    spawn_daemon,
};
pub(crate) use self::workspace::sandbox_runtime_override_log_lines;
pub use self::workspace::{SandboxWorkspace, collect_output_checks};
