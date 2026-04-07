use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test, web};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use std::sync::Arc;
use tokio::sync::Mutex;
use wp_station::db::{NewKnowledgeConfig, create_knowledge_config};
use wp_station::server::SharedRecord;
use wp_station_migrations::entity::knowledge_config::{
    Column as KnowledgeColumn, Entity as KnowledgeEntity,
};

async fn cleanup_knowledge_entry(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = KnowledgeEntity::delete_many()
        .filter(KnowledgeColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
}

#[actix_web::test]
async fn test_debug_api_endpoints_cover_all_handlers() {
    setup_db().await;
    std::fs::create_dir_all("project_root/.run").expect("prepare knowledge runtime dir");

    let know_file = format!("debug-knowledge-{}", rand_suffix());
    create_knowledge_config(NewKnowledgeConfig {
        file_name: know_file.clone(),
        config_content: Some("table = \"demo\"".to_string()),
        create_sql: Some("CREATE TABLE demo(id INTEGER)".to_string()),
        insert_sql: Some("INSERT INTO demo VALUES (1)".to_string()),
        data_content: None,
    })
    .await
    .expect("create knowledge config");

    let shared: SharedRecord = Arc::new(Mutex::new(None));
    let shared_data = web::Data::new(shared.clone());
    let app = test::init_service(
        App::new()
            .app_data(shared_data.clone())
            .service(wp_station::api::debug_parse)
            .service(wp_station::api::debug_knowledge_status)
            .service(wp_station::api::debug_knowledge_query)
            .service(wp_station::api::debug_performance_run)
            .service(wp_station::api::debug_performance_get)
            .service(wp_station::api::wpl_format)
            .service(wp_station::api::oml_format)
            .service(wp_station::api::debug_examples),
    )
    .await;

    // parse logs with WPL rules and ensure response contains JSON payload
    let parse_req = test::TestRequest::post()
        .uri("/api/debug/parse")
        .set_json(&serde_json::json!({
            "rules": "package demo { rule entry { ( chars:name ) } }",
            "logs": "alice"
        }))
        .to_request();
    let parse_resp = test::call_service(&app, parse_req).await;
    assert_eq!(parse_resp.status(), StatusCode::OK);
    let parse_body: serde_json::Value = test::read_body_json(parse_resp).await;
    assert!(parse_body.get("format_json").is_some());

    // knowledge status should list the inserted entry
    let status_req = test::TestRequest::get()
        .uri("/api/debug/knowledge/status")
        .to_request();
    let status_resp = test::call_service(&app, status_req).await;
    assert_eq!(status_resp.status(), StatusCode::OK);
    let status_body: serde_json::Value = test::read_body_json(status_resp).await;
    assert!(
        status_body
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item.get("tag_name").and_then(|n| n.as_str()) == Some(know_file.as_str()))
    );

    // run a simple SQL query via knowledge API
    let query_req = test::TestRequest::post()
        .uri("/api/debug/knowledge/query")
        .set_json(&serde_json::json!({
            "table": "demo",
            "sql": "SELECT 1 as value"
        }))
        .to_request();
    let query_resp = test::call_service(&app, query_req).await;
    assert_eq!(query_resp.status(), StatusCode::OK);
    let query_body: serde_json::Value = test::read_body_json(query_resp).await;
    assert_eq!(query_body["success"], true);

    // start a performance task and fetch it back
    let run_req = test::TestRequest::post()
        .uri("/api/debug/performance/run")
        .set_json(&serde_json::json!({
            "sample": "{\"msg\": \"hello\"}",
            "config": "{\"concurrency\":1}"
        }))
        .to_request();
    let run_resp = test::call_service(&app, run_req).await;
    assert_eq!(run_resp.status(), StatusCode::OK);
    let run_payload: serde_json::Value = test::read_body_json(run_resp).await;
    let task_id = run_payload["task_id"].as_str().unwrap().to_string();

    let get_req = test::TestRequest::get()
        .uri(&format!("/api/debug/performance/{}", task_id))
        .to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    // formatter endpoints accept raw text payloads
    let wpl_req = test::TestRequest::post()
        .uri("/api/debug/wpl/format")
        .set_payload("package demo { rule r { ( chars:name ) } }")
        .insert_header(("content-type", "text/plain"))
        .to_request();
    let wpl_resp = test::call_service(&app, wpl_req).await;
    assert_eq!(wpl_resp.status(), StatusCode::OK);

    let oml_req = test::TestRequest::post()
        .uri("/api/debug/oml/format")
        .set_payload("name:test\nrule:/foo/*\n---\nvalue = read(raw) ;")
        .insert_header(("content-type", "text/plain"))
        .to_request();
    let oml_resp = test::call_service(&app, oml_req).await;
    assert_eq!(oml_resp.status(), StatusCode::OK);

    let examples_req = test::TestRequest::get()
        .uri("/api/debug/examples")
        .to_request();
    let examples_resp = test::call_service(&app, examples_req).await;
    assert_eq!(examples_resp.status(), StatusCode::OK);

    cleanup_knowledge_entry(&know_file).await;
}
