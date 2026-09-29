//! 应用启动与 HTTP 服务装配入口。
//!
//! 负责拼装 Actix 应用、共享状态和静态资源服务，
//! 真正的初始化细节与路由挂载继续下沉到子模块。

mod boot;
mod routes;

use crate::server::{SandboxState, Setting};
use crate::utils::{read_runtime_asset_from_public, sync_tree_sitter_assets_for_dev_start};
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Result, middleware::Logger, web};
use mime_guess::from_path;
use rust_embed::RustEmbed;
use std::sync::Arc;
use tokio::sync::Mutex;
use wp_model_core::model::DataRecord;

/// 调试接口之间共享的解析结果缓存。
pub type SharedRecord = Arc<Mutex<Option<DataRecord>>>;

/// 编译时嵌入的前端静态资源集合。
#[derive(RustEmbed)]
#[folder = "web/dist"]
struct WebAssets;

/// 返回前端构建产物中的静态资源，缺省回退到 `index.html`。
pub(super) async fn static_files(req: HttpRequest) -> Result<HttpResponse> {
    let mut path = req.path().trim_start_matches('/');

    if path.is_empty() {
        path = "index.html";
    }

    if let Some(bytes) = read_runtime_asset_from_public(req.path())
        .map_err(actix_web::error::ErrorInternalServerError)?
    {
        let content_type = if path.ends_with(".wasm") {
            "application/wasm".to_string()
        } else {
            from_path(path).first_or_octet_stream().to_string()
        };

        return Ok(HttpResponse::Ok().content_type(content_type).body(bytes));
    }

    if let Some(file) = WebAssets::get(path) {
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

/// 启动应用初始化流程并监听 HTTP 服务。
pub async fn start() -> std::io::Result<()> {
    let setting = Setting::load();
    let runtime_log_level = boot::build_runtime_log_level(&setting.log.level);
    simple_log::quick!(&runtime_log_level);

    if let Err(err) = sync_tree_sitter_assets_for_dev_start() {
        tracing::warn!("开发态同步 tree-sitter 资产失败: error={}", err);
    }

    boot::initialize_runtime(&setting).await?;

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
                    .exclude("/wfusion-rule-editor")
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
            .configure(routes::configure_api_services)
            .default_service(web::to(routes::default_route))
    })
    .bind((setting.web.host.as_str(), setting.web.port))?
    .run()
    .await
}
