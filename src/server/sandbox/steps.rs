//! 沙盒各执行阶段的具体实现。

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::sleep;

use crate::constants::sandbox::{
    DAEMON_READY_BEFORE_WPGEN_WAIT_MS, RUNTIME_UDP_PORT, WFUSION_RUNTIME_TCP_PORT,
};
use crate::error::AppError;
use crate::server::sandbox::analyze::{self, StageError};
use crate::utils::sandbox::{self, SandboxWorkspace};

use super::progress::{RunResources, set_stage_log_path};
use super::{FileOverride, SandboxStage, SandboxTaskHandle};

/// 准备沙盒工作区并写出初始目录日志。
pub(super) async fn stage_prepare_workspace(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
    overrides: &[FileOverride],
) -> Result<String, StageError> {
    let system = resources.system();
    let workspace =
        SandboxWorkspace::prepare(task.task_id(), system, overrides).map_err(to_stage_error)?;
    let workspace_path = workspace.display_relative(&workspace.project_dir);
    task.with_run_mut(|run| {
        run.workspace_path = Some(workspace_path);
    })
    .await;

    let mut log_lines = vec![
        format!("task_id: {}", task.task_id()),
        format!(
            "source_models_root: {}",
            workspace.display_relative(&workspace.source_models_root)
        ),
        format!(
            "source_infra_root: {}",
            workspace.display_relative(&workspace.source_infra_root)
        ),
        format!(
            "source_connectors_root: {}",
            workspace.display_relative(&workspace.source_connectors_root)
        ),
        format!(
            "sandbox_project_dir: {}",
            workspace.display_relative(&workspace.project_dir)
        ),
        format!("system: {}", system.as_ref()),
        "已按当前系统的 models + infra(conf/connectors/topology) 合成沙盒目录".to_string(),
        "目录结构（截断预览）:".to_string(),
        format!(
            "$ tree -L 4 {}",
            workspace.display_relative(&workspace.project_dir)
        ),
    ];
    match workspace.render_tree_listing(4, 200) {
        Ok(tree) => log_lines.push(tree),
        Err(err) => log_lines.push(format!("生成目录树失败: {}", err)),
    }
    log_lines.push("\n沙盒运行时配置:".to_string());
    log_lines.extend(sandbox::sandbox_runtime_override_log_lines(
        &workspace, system,
    ));
    let log_path = workspace
        .write_text_log("prepare.log", &log_lines.join("\n"))
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::PrepareWorkspace,
        &log_path,
        Some(&workspace),
    )
    .await;

    resources.set_workspace(workspace);
    Ok("已复制到沙盒目录".to_string())
}

/// 执行启动前检查，包括命令、端口和项目校验。
pub(super) async fn stage_preflight_check(
    task: &Arc<SandboxTaskHandle>,
    resources: &RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let system = resources.system();
    let mut log_lines: Vec<String> = Vec::new();

    log_lines.push("开始预检查：验证命令与必要文件".to_string());

    let daemon_cmd = sandbox::daemon_command(system);
    let generator_cmd = sandbox::generator_command(system);
    log_lines.push(format!("\n检查 {} 命令", daemon_cmd));
    match sandbox::command_version_output(daemon_cmd).await {
        Ok(version) => {
            let version_text = if version.is_empty() {
                format!("{} --version 未输出版本信息", daemon_cmd)
            } else {
                format!("{} --version 输出: {}", daemon_cmd, version)
            };
            log_lines.push(version_text);
        }
        Err(err) => {
            log_lines.push(format!("{} --version 执行失败: {}", daemon_cmd, err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "PREFLIGHT_CHECK_FAILED",
            )
            .await;
        }
    }

    log_lines.push(format!("\n检查 {} 命令", generator_cmd));
    match sandbox::command_version_output(generator_cmd).await {
        Ok(version) => {
            let version_text = if version.is_empty() {
                format!("{} --version 未输出版本信息", generator_cmd)
            } else {
                format!("{} --version 输出: {}", generator_cmd, version)
            };
            log_lines.push(version_text);
        }
        Err(err) => {
            log_lines.push(format!("{} --version 执行失败: {}", generator_cmd, err));
            return fail_preflight_check(
                task,
                &workspace,
                &log_lines,
                "命令检查失败，请点击查看详情",
                "PREFLIGHT_CHECK_FAILED",
            )
            .await;
        }
    }

    match system {
        crate::utils::SystemKind::Wparse => {
            log_lines.push(format!("\n检查 UDP 端口 {}", RUNTIME_UDP_PORT));
            match sandbox::ensure_udp_port_available(RUNTIME_UDP_PORT) {
                Ok(_) => {
                    log_lines.push(format!("UDP 端口 {} 可用", RUNTIME_UDP_PORT));
                }
                Err(err) => {
                    log_lines.push(format!("UDP 端口 {} 不可用: {}", RUNTIME_UDP_PORT, err));
                    let summary = format!("UDP 端口 {} 已被占用，请点击查看详情", RUNTIME_UDP_PORT);
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        &summary,
                        "SANDBOX_UDP_PORT_UNAVAILABLE",
                    )
                    .await;
                }
            }
        }
        crate::utils::SystemKind::Wfusion => {
            log_lines.push(format!("\n检查 TCP 端口 {}", WFUSION_RUNTIME_TCP_PORT));
            match sandbox::ensure_tcp_port_available(WFUSION_RUNTIME_TCP_PORT) {
                Ok(_) => {
                    log_lines.push(format!("TCP 端口 {} 可用", WFUSION_RUNTIME_TCP_PORT));
                }
                Err(err) => {
                    log_lines.push(format!(
                        "TCP 端口 {} 不可用: {}",
                        WFUSION_RUNTIME_TCP_PORT, err
                    ));
                    let summary = format!(
                        "TCP 端口 {} 已被占用，请点击查看详情",
                        WFUSION_RUNTIME_TCP_PORT
                    );
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        &summary,
                        "SANDBOX_TCP_PORT_UNAVAILABLE",
                    )
                    .await;
                }
            }
        }
    }

    match system {
        crate::utils::SystemKind::Wparse => {
            log_lines.push("\n检查 wpadm 命令".to_string());
            match sandbox::command_version_output("wpadm").await {
                Ok(version) => {
                    let version_text = if version.is_empty() {
                        "wpadm --version 未输出版本信息".to_string()
                    } else {
                        format!("wpadm --version 输出: {}", version)
                    };
                    log_lines.push(version_text);
                }
                Err(err) => {
                    log_lines.push(format!("wpadm --version 执行失败: {}", err));
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        "命令检查失败，请点击查看详情",
                        "PREFLIGHT_CHECK_FAILED",
                    )
                    .await;
                }
            }

            let wpgen_conf = workspace.project_dir.join("conf/wpgen.toml");
            log_lines.push(format!(
                "\n检查文件：{}",
                workspace.display_relative(&wpgen_conf)
            ));
            if let Err(err) = ensure_exists(&wpgen_conf) {
                log_lines.push(format!("检查失败：{}", err.summary));
                return fail_preflight_check(
                    task,
                    &workspace,
                    &log_lines,
                    "命令检查失败，请点击查看详情",
                    "PREFLIGHT_CHECK_FAILED",
                )
                .await;
            }
        }
        crate::utils::SystemKind::Wfusion => {
            log_lines.push("\n检查 wfadm 命令".to_string());
            match sandbox::command_version_output("wfadm").await {
                Ok(version) => {
                    let version_text = if version.is_empty() {
                        "wfadm --version 未输出版本信息".to_string()
                    } else {
                        format!("wfadm --version 输出: {}", version)
                    };
                    log_lines.push(version_text);
                }
                Err(err) => {
                    log_lines.push(format!("wfadm --version 执行失败: {}", err));
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        "命令检查失败，请点击查看详情",
                        "PREFLIGHT_CHECK_FAILED",
                    )
                    .await;
                }
            }

            let scenario_path =
                sandbox::find_wfusion_scenario(&workspace.project_dir).map_err(to_stage_error)?;
            log_lines.push(format!(
                "\n检查文件：{}",
                workspace.display_relative(&scenario_path)
            ));
            if let Err(err) = ensure_exists(&scenario_path) {
                log_lines.push(format!("检查失败：{}", err.summary));
                return fail_preflight_check(
                    task,
                    &workspace,
                    &log_lines,
                    "命令检查失败，请点击查看详情",
                    "PREFLIGHT_CHECK_FAILED",
                )
                .await;
            }
        }
    }

    log_lines.push("命令与文件检查通过".to_string());
    match system {
        crate::utils::SystemKind::Wparse => {
            log_lines.push("\n执行 wpadm check".to_string());
            match sandbox::run_wpadm_check(&workspace.project_dir).await {
                Ok(output) => {
                    if output.is_empty() {
                        log_lines.push("wpadm check 通过: 未返回额外输出".to_string());
                    } else {
                        log_lines.push(format!("wpadm check 通过: {}", output));
                    }
                }
                Err(err) => {
                    log_lines.push(format!("wpadm check 失败: {}", err));
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        "命令检查失败，请点击查看详情",
                        "WPADM_CHECK_FAILED",
                    )
                    .await;
                }
            }
        }
        crate::utils::SystemKind::Wfusion => {
            log_lines.push("\n执行 wfadm check".to_string());
            match sandbox::run_wfadm_check(&workspace.project_dir).await {
                Ok(output) => {
                    if output.is_empty() {
                        log_lines.push("wfadm check 通过: 未返回额外输出".to_string());
                    } else {
                        log_lines.push(format!("wfadm check 通过: {}", output));
                    }
                }
                Err(err) => {
                    log_lines.push(format!("wfadm check 失败: {}", err));
                    return fail_preflight_check(
                        task,
                        &workspace,
                        &log_lines,
                        "命令检查失败，请点击查看详情",
                        "WFADM_CHECK_FAILED",
                    )
                    .await;
                }
            }
        }
    }

    log_lines.push("预检查全部通过".to_string());
    write_preflight_log(task, &workspace, &log_lines).await?;
    Ok("命令检查均通过".to_string())
}

/// 将预检查阶段的完整日志写入 `check.log` 并回填路径。
async fn write_preflight_log(
    task: &Arc<SandboxTaskHandle>,
    workspace: &SandboxWorkspace,
    log_lines: &[String],
) -> Result<(), StageError> {
    let log_path = workspace
        .write_text_log("check.log", &log_lines.join("\n"))
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::PreflightCheck,
        &log_path,
        Some(workspace),
    )
    .await;
    Ok(())
}

/// 以统一方式写出预检查日志并返回阶段失败结果。
async fn fail_preflight_check(
    task: &Arc<SandboxTaskHandle>,
    workspace: &SandboxWorkspace,
    log_lines: &[String],
    summary: &str,
    code: &str,
) -> Result<String, StageError> {
    write_preflight_log(task, workspace, log_lines).await?;
    Err(StageError::with_code(summary, code))
}

/// 启动 `wparse` 并等待 ready 标记。
pub(super) async fn stage_start_daemon(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let system = resources.system();
    let daemon_cmd = sandbox::daemon_command(system);
    let log_name = format!("{}.log", daemon_cmd);
    let log_path = workspace.log_path(&log_name);
    let port_check = match system {
        crate::utils::SystemKind::Wparse => sandbox::ensure_udp_port_available(RUNTIME_UDP_PORT)
            .map_err(|err| (RUNTIME_UDP_PORT.to_string(), "UDP", err)),
        crate::utils::SystemKind::Wfusion => {
            sandbox::ensure_tcp_port_available(WFUSION_RUNTIME_TCP_PORT)
                .map_err(|err| (WFUSION_RUNTIME_TCP_PORT.to_string(), "TCP", err))
        }
    };
    if let Err((port, protocol, err)) = port_check {
        let log_text = format!(
            "启动 {daemon_cmd} 前检查 {protocol} 端口失败\n{protocol} 端口 {port} 不可用: {err}\n"
        );
        let written_log = workspace
            .write_text_log(&log_name, &log_text)
            .map_err(to_stage_error)?;
        set_stage_log_path(
            task,
            SandboxStage::StartDaemon,
            &written_log,
            Some(&workspace),
        )
        .await;
        return Err(StageError::with_code(
            format!("{protocol} 端口 {port} 已被占用，请点击查看详情"),
            if protocol == "UDP" {
                "SANDBOX_UDP_PORT_UNAVAILABLE"
            } else {
                "SANDBOX_TCP_PORT_UNAVAILABLE"
            },
        ));
    }

    let mut daemon = sandbox::spawn_daemon(system, &workspace.project_dir, &log_path)
        .await
        .map_err(to_stage_error)?;
    resources.set_daemon_log(log_path.clone());
    let wait_result = daemon.wait_for_marker(
        sandbox::daemon_ready_marker(system),
        Duration::from_millis(resources.options.startup_timeout_ms),
    );

    set_stage_log_path(task, SandboxStage::StartDaemon, &log_path, Some(&workspace)).await;

    match wait_result.await {
        Ok(_) => {
            resources.metrics_mut().daemon_ready = true;
            resources.set_daemon(daemon);
            Ok(format!("{daemon_cmd} 已启动，等待模拟数据发送分析"))
        }
        Err(err) => {
            resources.set_daemon(daemon);
            Err(StageError::with_code(
                format!("{daemon_cmd} 启动失败，请点击查看详情"),
                format!("DAEMON_START_FAILED: {}", err),
            ))
        }
    }
}

/// 运行 `wpgen` 并解析生成结果。
pub(super) async fn stage_run_wpgen(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let workspace = resources.workspace()?.clone();
    let system = resources.system();
    let generator_cmd = sandbox::generator_command(system);
    let log_path = workspace.log_path(&format!("{generator_cmd}.log"));
    sleep(Duration::from_millis(DAEMON_READY_BEFORE_WPGEN_WAIT_MS)).await;
    let output = sandbox::run_generator(
        system,
        &workspace.project_dir,
        &log_path,
        resources.options.sample_count,
        Duration::from_millis(resources.options.wpgen_timeout_ms),
    )
    .await
    .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::RunWpgen,
        &output.log_path,
        Some(&workspace),
    )
    .await;

    let (count, _) =
        analyze::analyse_generator_result(system, &output.log_path).map_err(|err| {
            StageError::with_code(
                format!("{generator_cmd} 启动失败，请点击查看详情"),
                err.code
                    .unwrap_or_else(|| "GENERATOR_ANALYSE_FAILED".to_string()),
            )
        })?;
    let metrics = resources.metrics_mut();
    metrics.wpgen_exit_code = output.exit_code;
    metrics.input_count = count;
    metrics.wpgen_generated = Some(count);

    Ok(match system {
        crate::utils::SystemKind::Wparse => format!(
            "wparse 监听稳定等待{}ms后，wpgen 已启动，已发送{}条消息。命令: {}",
            DAEMON_READY_BEFORE_WPGEN_WAIT_MS,
            count,
            output.command_lines.join("\n")
        ),
        crate::utils::SystemKind::Wfusion => {
            let cmds = output.command_lines.join("\n");
            format!(
                "wfusion 启动稳定等待{}ms后，wfgen 已启动（{}个场景），已生成并发送{}条消息。命令:\n{}",
                DAEMON_READY_BEFORE_WPGEN_WAIT_MS,
                output.command_lines.len(),
                count,
                cmds,
            )
        }
    })
}

/// 分析运行输出并给出是否通过的结论。
pub(super) async fn stage_analyse_runtime_output(
    task: &Arc<SandboxTaskHandle>,
    resources: &mut RunResources,
) -> Result<String, StageError> {
    let wait_ms = resources.options.runtime_collect_ms;
    sleep(Duration::from_millis(wait_ms)).await;

    let workspace = resources.workspace()?.clone();
    let daemon_log = resources
        .daemon_log()
        .ok_or_else(|| StageError::new("daemon 尚未启动"))?;
    let system = resources.system();

    let expected_success = if resources.metrics().input_count > 0 {
        resources.metrics().input_count
    } else {
        resources.options.sample_count as usize
    };
    let analysis = analyze::analyse_runtime_output(
        system,
        &workspace.project_dir,
        &daemon_log,
        expected_success,
    )?;
    resources.set_output_checks(analysis.output_checks.clone());
    let metrics_mut = resources.metrics_mut();
    metrics_mut.passed = analysis.metrics.passed;
    metrics_mut.miss_count = analysis.metrics.miss_count;
    metrics_mut.error_count = analysis.metrics.error_count;
    metrics_mut.output_count = analysis.metrics.output_count;

    let mut log_text = format!(
        "等待 {}ms 收集 {} 输出\n\n",
        wait_ms,
        sandbox::daemon_command(system)
    );
    log_text.push_str(&analysis.log_text);
    let log_path = workspace
        .write_text_log("analysis.log", &log_text)
        .map_err(to_stage_error)?;
    set_stage_log_path(
        task,
        SandboxStage::AnalyseRuntimeOutput,
        &log_path,
        Some(&workspace),
    )
    .await;

    if let Some(daemon) = resources.take_daemon() {
        daemon.terminate().await.map_err(to_stage_error)?;
    }

    if analysis.passed {
        Ok(match system {
            crate::utils::SystemKind::Wparse => format!(
                "已等待{}ms 收集输出并关闭wparse；已模拟{}条消息，成功输出{}条。",
                wait_ms, expected_success, analysis.metrics.output_count
            ),
            crate::utils::SystemKind::Wfusion => format!(
                "已等待{}ms 收集输出并关闭wfusion；已生成{}条消息，业务输出{}条。",
                wait_ms, expected_success, analysis.metrics.output_count
            ),
        })
    } else {
        let mut details: Vec<String> = analysis
            .output_checks
            .iter()
            .filter(|check| check.affects_pass && !check.is_empty)
            .map(|check| format!("{} 非空（{}行）", check.relative_path, check.line_count))
            .collect();
        if analysis.metrics.miss_count > 0 {
            details.push(format!("rule miss 日志 {} 条", analysis.metrics.miss_count));
        }
        if system == crate::utils::SystemKind::Wparse
            && analysis.metrics.output_count < expected_success
        {
            details.push(format!(
                "成功输出数量 {} 条，至少应为 {} 条",
                analysis.metrics.output_count, expected_success
            ));
        }
        if system == crate::utils::SystemKind::Wfusion && analysis.metrics.output_count == 0 {
            details.push("未观察到 alert.json 业务告警输出，请检查场景是否命中规则".to_string());
        }
        let summary = if details.is_empty() {
            "结果检查失败，请点击查看详情".to_string()
        } else {
            format!("结果检查失败：{}", details.join("；"))
        };
        Err(StageError::with_code(summary, "RUNTIME_ANALYSIS_FAILED"))
    }
}

/// 校验预检查依赖文件是否存在。
fn ensure_exists(path: &Path) -> Result<(), StageError> {
    if path.exists() {
        Ok(())
    } else {
        Err(StageError::new(format!("缺少必要文件: {}", path.display())))
    }
}

/// 将通用应用错误转换为沙盒阶段错误。
fn to_stage_error(err: AppError) -> StageError {
    StageError::new(err.to_string())
}
