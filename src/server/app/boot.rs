//! 应用启动阶段的初始化逻辑。
//!
//! 这一层只负责：
//! - 日志级别拼装
//! - 数据库和迁移初始化
//! - 外部依赖预热
//! - 默认目录和默认配置补齐
//! - 后台任务启动

use crate::db::{DeviceStatus, find_all_devices, get_pool, init_pool};
use crate::server::sync::ensure_project_repositories;
use crate::server::{Setting, spawn_release_task_runner, spawn_restore_task_runner};
use crate::utils::{
    WarpParseService, WfusionService, all_system_layouts, check_device_health,
    init_default_configs_to_infra_for_system, init_default_configs_to_models_for_system,
};
use wp_station_migrations::MigratorTrait;

/// 为高噪音依赖追加模块级日志过滤，避免校验接口刷出大量中间过程日志。
pub(super) fn build_runtime_log_level(level: &str) -> String {
    let base_level = if level.trim().is_empty() {
        "debug"
    } else {
        level.trim()
    };

    let noisy_targets = ["orion_error", "wp_config", "wp_engine"];

    let mut runtime_level = base_level.to_string();
    for target in noisy_targets {
        if !base_level.contains(&format!("{target}=")) {
            runtime_level.push(',');
            runtime_level.push_str(target);
            runtime_level.push_str("=warn");
        }
    }

    runtime_level
}

/// 完成应用启动前的所有初始化动作。
pub(super) async fn initialize_runtime(setting: &Setting) -> std::io::Result<()> {
    info!("启动 WarpStation 服务器");
    info!("Web 地址: {}:{}", setting.web.host, setting.web.port);
    info!("数据库: {}", setting.database.safe_summary());

    init_pool(&setting.database).await.map_err(|e| {
        error!("数据库连接失败: {}", e);
        std::io::Error::other(format!("数据库连接失败: {}", e))
    })?;

    let pool = get_pool();
    wp_station_migrations::Migrator::up(pool.inner(), None)
        .await
        .map_err(|e| {
            error!("数据库迁移失败: {}", e);
            std::io::Error::other(format!("数据库迁移失败: {}", e))
        })?;
    info!("数据库迁移完成");

    preload_admin_api_tls(setting)?;

    spawn_release_task_runner(setting.admin_api.clone());
    spawn_restore_task_runner();

    match ensure_project_repositories().await {
        Ok(_) => info!("项目 Git 仓库检查完成"),
        Err(e) => warn!("检查项目 Git 仓库失败，已降级跳过 Gitea 初始化: {}", e),
    }

    initialize_repo_layouts()?;
    info!("默认配置检查完成");

    spawn_health_check_task();
    Ok(())
}

/// 启动时预加载设备管理接口 TLS 证书，尽早暴露证书问题。
fn preload_admin_api_tls(setting: &Setting) -> std::io::Result<()> {
    if setting.admin_api.enabled {
        WarpParseService::preload_tls(&setting.admin_api).map_err(|e| {
            error!("WarpParse TLS 证书加载失败: {}", e);
            std::io::Error::other(format!("WarpParse TLS 证书加载失败: {}", e))
        })?;
        WfusionService::preload_tls(&setting.admin_api).map_err(|e| {
            error!("wfusion TLS 证书加载失败: {}", e);
            std::io::Error::other(format!("wfusion TLS 证书加载失败: {}", e))
        })?;
        info!("设备管理接口访问协议: https，TLS 证书加载完成");
    } else {
        info!("设备管理接口访问协议: http，已跳过 TLS 证书加载");
    }

    Ok(())
}

/// 补齐双系统固定目录，并按系统补齐默认配置。
fn initialize_repo_layouts() -> std::io::Result<()> {
    for layout in all_system_layouts() {
        init_default_configs_to_models_for_system(
            layout.system,
            layout.models_root.to_string_lossy().as_ref(),
        )
        .map_err(|e| {
            error!(
                "加载默认 models 配置失败: system={}, error={}",
                layout.system.as_ref(),
                e
            );
            std::io::Error::other(format!("加载默认配置失败: {}", e))
        })?;
        init_default_configs_to_infra_for_system(
            layout.system,
            layout.infra_root.to_string_lossy().as_ref(),
        )
        .map_err(|e| {
            error!(
                "加载默认 infra 配置失败: system={}, error={}",
                layout.system.as_ref(),
                e
            );
            std::io::Error::other(format!("加载默认配置失败: {}", e))
        })?;
    }

    Ok(())
}

/// 后台健康检查定时任务：每 60 秒遍历所有非删除连接，调用 `/health` 更新状态。
///
/// 启动时立即执行一次，然后定时执行。
fn spawn_health_check_task() {
    tokio::spawn(async move {
        info!("启动时执行设备健康检查");
        match find_all_devices().await {
            Ok(devices) => {
                let count = devices.len();
                for device in devices {
                    if device.status != DeviceStatus::Deleted.as_ref() {
                        check_device_health(device.id).await;
                    }
                }
                info!("启动时设备健康检查完成，共检查 {} 台设备", count);
            }
            Err(e) => {
                warn!("启动时健康检查失败: {}", e);
            }
        }

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

            match find_all_devices().await {
                Ok(devices) => {
                    let count = devices.len();
                    debug!("定时执行设备健康检查，共 {} 台设备", count);
                    for device in devices {
                        if device.status != DeviceStatus::Deleted.as_ref() {
                            check_device_health(device.id).await;
                        }
                    }
                }
                Err(e) => {
                    warn!("健康检查定时任务查询连接失败: {}", e);
                }
            }
        }
    });
}
