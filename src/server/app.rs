// 应用启动逻辑

use crate::db::{DeviceStatus, find_all_devices, get_pool, init_pool};
use crate::server::release_task_runner::spawn_release_task_runner;
use crate::utils::check_device_health;
use crate::{
    api,
    server::{SandboxState, Setting},
};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Result, middleware::Logger, web};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tokio::sync::Mutex;
use wp_model_core::model::DataRecord;

// SharedRecord 类型定义
pub type SharedRecord = Arc<Mutex<Option<DataRecord>>>;

#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebAssets;

// 处理静态资源
async fn static_files(req: HttpRequest) -> Result<HttpResponse> {
    let mut path = req.path().trim_start_matches('/');

    if path.is_empty() {
        path = "index.html";
    }

    if let Some(file) = WebAssets::get(path) {
        // 特殊处理 WASM 文件的 MIME type
        let content_type = if path.ends_with(".wasm") {
            "application/wasm".to_string()
        } else {
            from_path(path).first_or_octet_stream().to_string()
        };

        Ok(HttpResponse::Ok()
            .content_type(content_type)
            .body(file.data.to_vec()))
    } else if let Some(index) = WebAssets::get("index.html") {
        Ok(HttpResponse::Ok()
            .content_type("text/html; charset=utf-8")
            .body(index.data.to_vec()))
    } else {
        Err(actix_web::error::ErrorNotFound("File not found"))
    }
}

/// 后台健康检查定时任务：每 60 秒遍历所有非删除连接，调用 /health 更新状态
/// 启动时立即执行一次，然后定时执行
fn spawn_health_check_task() {
    tokio::spawn(async move {
        // 启动时立即执行一次健康检查
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

        // 定时循环执行
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

pub async fn start() -> std::io::Result<()> {
    let setting = Setting::load();
    simple_log::quick!(&setting.log.level);

    info!("启动 WarpStation 服务器");
    info!("Web 地址: {}:{}", setting.web.host, setting.web.port);
    info!("数据库: {}", setting.database.safe_summary());

    // 初始化全局数据库连接池
    init_pool(&setting.database).await.map_err(|e| {
        error!("数据库连接失败: {}", e);
        std::io::Error::other(format!("数据库连接失败: {}", e))
    })?;

    // 运行数据库迁移
    let pool = get_pool();
    use wp_station_migrations::{Migrator, MigratorTrait};
    Migrator::up(pool.inner(), None).await.map_err(|e| {
        error!("数据库迁移失败: {}", e);
        std::io::Error::other(format!("数据库迁移失败: {}", e))
    })?;

    info!("数据库迁移完成");

    // 启动发布任务调度器
    let warparse_conf = setting.warparse.clone();
    spawn_release_task_runner(warparse_conf);

    // 检查 rule_configs 表是否为空，如果为空则从嵌入的默认配置目录加载数据
    use crate::db::{init_default_configs_from_embedded, is_rule_configs_empty};
    if is_rule_configs_empty().await.map_err(|e| {
        error!("检查 rule_configs 表失败: {}", e);
        std::io::Error::other(format!("检查 rule_configs 表失败: {}", e))
    })? {
        info!("rule_configs 表为空，开始从默认配置目录加载数据");
        init_default_configs_from_embedded(pool.inner())
            .await
            .map_err(|e| {
                error!("加载默认配置失败: {}", e);
                std::io::Error::other(format!("加载默认配置失败: {}", e))
            })?;
        info!("默认配置加载完成");

        // 导出所有配置到项目目录
        use crate::utils::export_project_from_db;
        export_project_from_db(pool.inner(), &setting.project_root)
            .await
            .map_err(|e| {
                error!("导出配置到项目目录失败: {}", e);
                std::io::Error::other(format!("导出配置失败: {}", e))
            })?;
        info!("配置导出到项目目录完成");
    } else {
        info!("rule_configs 表已有数据，跳过数据库初始化");
    }

    // 独立检查本地 Git 仓库是否已初始化（与数据库初始化状态解耦）
    let project_root = std::path::PathBuf::from(&setting.project_root);
    let project_path = if project_root.is_absolute() {
        project_root
    } else {
        crate::server::Setting::workspace_root().join(&setting.project_root)
    };

    // 若 project_root 不存在或为空，则从数据库重新导出配置文件
    let project_root_empty = !project_path.exists()
        || project_path
            .read_dir()
            .map(|d| {
                !d.flatten()
                    .any(|e| !e.file_name().to_string_lossy().starts_with('.'))
            })
            .unwrap_or(true);
    if project_root_empty {
        info!("project_root 目录为空，从数据库导出配置到项目目录");
        use crate::utils::export_project_from_db;
        export_project_from_db(pool.inner(), &setting.project_root)
            .await
            .map_err(|e| {
                error!("导出配置到项目目录失败: {}", e);
                std::io::Error::other(format!("导出配置失败: {}", e))
            })?;
        info!("配置导出到项目目录完成");
    } else {
        info!("project_root 目录已有文件，跳过导出");
    }
    if !project_path.join(".git").exists() {
        info!("本地 Git 仓库未初始化，开始初始化 Gitea 仓库");
        use crate::server::sync::init_gitea_repo;
        init_gitea_repo().await.map_err(|e| {
            error!("初始化 Gitea 仓库失败: {}", e);
            std::io::Error::other(format!("初始化 Gitea 仓库失败: {}", e))
        })?;
        info!("Gitea 仓库初始化完成，已创建 v1.0.0 tag");
    } else {
        info!("本地 Git 仓库已存在，跳过 Gitea 初始化");
    }

    // 启动健康检查后台定时任务
    spawn_health_check_task();

    // 创建并注入 SharedRecord
    let shared_record: SharedRecord = Arc::new(Mutex::new(None));
    let shared_record_data = web::Data::new(shared_record);
    let sandbox_state = web::Data::new(SandboxState::new());

    HttpServer::new(move || {
        App::new()
            // 只记录 API 请求，使用简洁格式：方法 路径 状态码 耗时
            .wrap(
                Logger::new("%r %s %Dms")
                    .exclude("/")
                    .exclude("/login")
                    .exclude("/devices")
                    .exclude("/features")
                    .exclude("/rule-manage")
                    .exclude("/config-manage")
                    .exclude("/simulate-debug")
                    .exclude("/system-release")
                    .exclude("/system-manage")
                    .exclude("/favicon.ico")
                    .exclude_regex("^/assets/")
                    .exclude_regex("^/tree-sitter/")
                    .exclude_regex("^/\\.well-known/"),
            )
            .app_data(shared_record_data.clone())
            .app_data(sandbox_state.clone())
            // 系统 API
            .service(api::hello)
            .service(api::get_version)
            // 设备管理 API
            .service(api::list_online_devices)
            .service(api::list_devices)
            .service(api::create_device)
            .service(api::update_device)
            .service(api::delete_device)
            .service(api::refresh_device_status)
            // 规则配置 API
            .service(api::get_rule_files)
            .service(api::get_rule_content)
            .service(api::create_rule_file)
            .service(api::delete_rule_file)
            .service(api::save_rule)
            .service(api::save_knowledge_rule)
            .service(api::validate_rule)
            // 配置管理 API（解析配置也复用配置接口）
            .service(api::get_config_files)
            .service(api::get_config)
            .service(api::save_config)
            .service(api::create_config_file)
            .service(api::delete_config_file)
            // 发布 API
            .service(api::list_releases)
            .service(api::get_release_detail)
            .service(api::get_release_diff)
            .service(api::create_release)
            .service(api::validate_release)
            .service(api::publish_release)
            .service(api::retry_release)
            .service(api::rollback_release)
            // 沙盒运行 API
            .service(api::create_sandbox_run)
            .service(api::get_sandbox_run)
            .service(api::stop_sandbox_run)
            .service(api::get_sandbox_stage_logs)
            .service(api::get_latest_sandbox_run)
            .service(api::list_sandbox_history)
            // 调试 API
            .service(api::debug_parse)
            .service(api::debug_transform)
            .service(api::debug_knowledge_status)
            .service(api::debug_knowledge_query)
            .service(api::debug_performance_run)
            .service(api::debug_performance_get)
            .service(api::wpl_format)
            .service(api::oml_format)
            .service(api::debug_examples)
            // AI 辅助规则编写 API
            .service(api::assist_submit)
            .service(api::assist_list)
            .service(api::assist_get)
            .service(api::assist_cancel)
            .service(api::assist_reply)
            // 操作日志 API
            .service(api::list_operation_logs)
            // 用户管理 API
            .service(api::list_users)
            .service(api::create_user)
            .service(api::update_user)
            .service(api::update_user_status)
            .service(api::reset_user_password)
            .service(api::change_user_password)
            .service(api::delete_user)
            // 认证 API
            .service(api::login)
            // 知识库 API
            .service(api::get_db_list)
            .service(api::query)
            // 默认路由：未匹配的 /api/* 返回 JSON 404，其余走静态文件（前端 SPA）
            .default_service(web::to(|req: HttpRequest| async move {
                if req.path().starts_with("/api/") {
                    HttpResponse::NotFound().json(serde_json::json!({
                        "success": false,
                        "error": {
                            "code": "NOT_FOUND",
                            "message": format!("API {} 不存在", req.path()),
                            "details": serde_json::json!({ "path": req.path() })
                        }
                    }))
                } else {
                    static_files(req)
                        .await
                        .unwrap_or_else(|_| HttpResponse::NotFound().finish())
                }
            }))
    })
    .bind((setting.web.host.as_str(), setting.web.port))?
    .run()
    .await
}
