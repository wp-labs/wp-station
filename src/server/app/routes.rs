//! HTTP 路由挂载辅助。
//!
//! 这一层只负责把现有 API 按领域挂到 Actix `ServiceConfig`，
//! 避免启动入口被长串 `.service(...)` 淹没。

use actix_web::{HttpRequest, HttpResponse, web};

use crate::api;

/// 挂载所有 API 服务。
pub(super) fn configure_api_services(cfg: &mut web::ServiceConfig) {
    cfg
        // 系统 API
        .service(api::hello)
        .service(api::get_version)
        .service(api::get_features_config)
        .service(api::get_integration_rule_overview)
        .service(api::get_integration_runtime_overview)
        .service(api::import_project_from_files)
        .service(api::import_project_archive)
        .service(api::confirm_project_archive_import)
        .service(api::export_project_archive)
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
        .service(api::get_knowdb_config)
        .service(api::save_knowdb_config)
        .service(api::validate_rule)
        // 配置管理 API
        .service(api::get_config_files)
        .service(api::get_config_templates)
        .service(api::get_config)
        .service(api::render_config_template)
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
        .service(api::restore_release)
        .service(api::get_release_restore)
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
        .service(api::wpl_format)
        .service(api::oml_format)
        .service(api::wfs_format)
        .service(api::wfl_format)
        .service(api::wfg_format)
        .service(api::toml_format)
        .service(api::debug_examples)
        .service(api::debug_wfusion_rule_editor_parse)
        // AI 辅助规则编写 API
        .service(api::assist_submit)
        .service(api::assist_list)
        .service(api::assist_get)
        .service(api::assist_cancel)
        .service(api::assist_reply)
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
        .service(api::query);
}

/// 统一处理未命中的路由。
pub(super) async fn default_route(req: HttpRequest) -> HttpResponse {
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
        super::static_files(req)
            .await
            .unwrap_or_else(|_| HttpResponse::NotFound().finish())
    }
}
