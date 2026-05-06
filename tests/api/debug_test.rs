use crate::common::{
    rand_suffix, remove_project_path, setup_db, test_models_root, test_project_layout,
};
use actix_web::{App, http::StatusCode, test, web};
use std::sync::Arc;
use tokio::sync::Mutex;
use wp_station::server::SharedRecord;
use wp_station::utils::{read_knowdb_config, write_knowdb_config, write_knowledge_files};

fn cleanup_knowledge_entry(file: &str) {
    remove_project_path(format!("models/knowledge/{file}"));
}

#[actix_web::test]
async fn test_debug_api_endpoints_cover_all_handlers() {
    setup_db().await;
    std::fs::create_dir_all(test_models_root().join(".run"))
        .expect("prepare knowledge runtime dir");

    let know_file = format!("debug_knowledge_{}", rand_suffix());
    let layout = test_project_layout();
    let existing_knowdb = read_knowdb_config(&layout)
        .expect("read knowdb")
        .and_then(|(content, _)| Some(content))
        .unwrap_or_else(|| "version = 2\n".to_string());
    let updated_knowdb = format!(
        r#"{existing}

[[tables]]
enabled = true
name = "{table_name}"
[tables.columns]
by_index = [0]
[tables.csv]
has_header = false
[tables.expected_rows]
min = 0
max = 10
"#,
        existing = existing_knowdb.trim_end(),
        table_name = know_file
    );
    write_knowdb_config(&layout, &updated_knowdb).expect("write knowdb");
    write_knowledge_files(
        &layout,
        &know_file,
        Some("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);".to_string()),
        Some("INSERT INTO {table} (id) VALUES (?1);".to_string()),
        Some("1\n".to_string()),
    )
    .expect("write knowledge files");

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

    // run a simple SQL query via knowledge API
    let query_req = test::TestRequest::post()
        .uri("/api/debug/knowledge/query")
        .set_json(&serde_json::json!({
            "table": know_file,
            "sql": "SELECT 1 as value"
        }))
        .to_request();
    let query_resp = test::call_service(&app, query_req).await;
    let query_status = query_resp.status();
    let query_body_bytes = test::read_body(query_resp).await;
    let query_body_text = String::from_utf8_lossy(&query_body_bytes).to_string();
    assert_eq!(
        query_status,
        StatusCode::OK,
        "query body: {query_body_text}"
    );
    let query_body: serde_json::Value =
        serde_json::from_slice(&query_body_bytes).expect("parse query body");
    assert_eq!(query_body["success"], true);
    assert_eq!(query_body["columns"], serde_json::json!(["value"]));

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

    cleanup_knowledge_entry(&know_file);
}
