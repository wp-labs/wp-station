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
        .map(|(content, _)| content)
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
            .service(wp_station::api::wpl_format)
            .service(wp_station::api::oml_format)
            .service(wp_station::api::debug_wfusion_rule_editor_parse)
            .service(wp_station::api::debug_examples),
    )
    .await;

    // parse logs with WPL rules and ensure response contains JSON payload
    let parse_req = test::TestRequest::post()
        .uri("/api/debug/parse")
        .set_json(serde_json::json!({
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
    assert!(status_body.as_array().unwrap().iter().any(|item| {
        item.get("tag_name").and_then(|n| n.as_str()) == Some(know_file.as_str())
            && item.get("label").and_then(|n| n.as_str()) == Some(know_file.as_str())
            && item.get("suggested_sql").and_then(|n| n.as_str())
                == Some(format!("select * from {know_file} limit 20;").as_str())
            && item.get("source_kind").and_then(|n| n.as_str()) == Some("local")
    }));

    // 再执行本地 knowledge SQL；此前 status 查询会误删 authority.sqlite，导致这里 connect db 失败。
    let query_req = test::TestRequest::post()
        .uri("/api/debug/knowledge/query")
        .set_json(serde_json::json!({
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

    let actual_table_req = test::TestRequest::post()
        .uri("/api/debug/knowledge/query")
        .set_json(serde_json::json!({
            "table": know_file,
            "source_kind": "local",
            "sql": format!("SELECT * FROM {know_file} LIMIT 20")
        }))
        .to_request();
    let actual_table_resp = test::call_service(&app, actual_table_req).await;
    let actual_table_status = actual_table_resp.status();
    let actual_table_body_bytes = test::read_body(actual_table_resp).await;
    let actual_table_body_text = String::from_utf8_lossy(&actual_table_body_bytes).to_string();
    assert_eq!(
        actual_table_status,
        StatusCode::OK,
        "actual table body: {actual_table_body_text}"
    );
    let actual_table_body: serde_json::Value =
        serde_json::from_slice(&actual_table_body_bytes).expect("parse actual table body");
    assert_eq!(actual_table_body["success"], true);

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

    let wfusion_parse_req = test::TestRequest::post()
        .uri("/api/debug/wfusion-editor/parse")
        .set_json(serde_json::json!({
            "events_ndjson": "{\"_stream\":\"netflow\",\"sip\":\"10.0.0.99\",\"dip\":\"192.168.1.10\",\"dport\":22,\"bytes_out\":100,\"protocol\":\"tcp\",\"event_time\":1700000000000000000}\n{\"_stream\":\"auth_events\",\"sip\":\"10.0.0.99\",\"dip\":\"192.168.1.10\",\"dport\":22,\"service\":\"ssh\",\"user\":\"root\",\"result\":\"success\",\"event_time\":1700000001000000000}\n{\"_stream\":\"netflow\",\"sip\":\"10.0.0.99\",\"dip\":\"192.168.1.10\",\"dport\":22,\"bytes_out\":50000,\"protocol\":\"tcp\",\"event_time\":1700000002000000000}",
            "wfs": "window conn_events {\n    stream_tag = \"netflow\"\n    time = event_time\n    over = 30m\n    fields {\n        sip: ip\n        dip: ip\n        dport: digit\n        bytes_out: digit\n        protocol: chars\n        event_time: time\n    }\n}\n\nwindow auth_events {\n    stream_tag = \"auth_events\"\n    time = event_time\n    over = 30m\n    fields {\n        sip: ip\n        dip: ip\n        dport: digit\n        service: chars\n        user: chars\n        result: chars\n        event_time: time\n    }\n}\n\nwindow security_alerts {\n    over = 0\n    fields {\n        sip: ip\n        dip: ip\n        alert_type: chars\n        detail: chars\n    }\n}",
            "wfl": "rule rat_propagation {\n    events {\n        scan  : conn_events && (dport == 22 || dport == 445 || dport == 3389) && bytes_out < 1000\n        login : auth_events && result == \"success\"\n        xfer  : conn_events && bytes_out >= 10000\n    }\n    match<sip,dip:5m> {\n        on event {\n            scan | count >= 1;\n            login | count >= 1;\n            xfer | count >= 1;\n        }\n    } -> score(95.0)\n    entity(ip, scan.sip)\n    yield security_alerts (\n        sip = scan.sip,\n        dip = scan.dip,\n        alert_type = \"rat_propagation\",\n        detail = \"scan -> login -> xfer\"\n    )\n    limits { max_memory = \"64MB\"; max_instances = 10000; on_exceed = throttle; }\n}"
        }))
        .to_request();
    let wfusion_parse_resp = test::call_service(&app, wfusion_parse_req).await;
    assert_eq!(wfusion_parse_resp.status(), StatusCode::OK);
    let wfusion_parse_body: serde_json::Value = test::read_body_json(wfusion_parse_resp).await;
    assert_eq!(
        wfusion_parse_body["success"], true,
        "unexpected wfusion parse response: {wfusion_parse_body}"
    );
    assert_eq!(wfusion_parse_body["summary"]["match_count"], 1);
    assert_eq!(
        wfusion_parse_body["alerts"]
            .as_array()
            .map(|items| items.len()),
        Some(1)
    );
    let first_alert = &wfusion_parse_body["alerts"][0];
    assert_eq!(first_alert["alert_type"], "rat_propagation");
    assert_eq!(first_alert["detail"], "scan -> login -> xfer");
    assert_eq!(first_alert["sip"], "10.0.0.99");
    assert_eq!(first_alert["dip"], "192.168.1.10");
    assert_eq!(first_alert["__wfu_rule_name"], "rat_propagation");

    let wfusion_invalid_req = test::TestRequest::post()
        .uri("/api/debug/wfusion-editor/parse")
        .set_json(serde_json::json!({
            "events_ndjson": "",
            "wfs": "window conn_events { stream_tag = \"netflow\" time = event_time over = 30m fields { event_time: time sip: ip dip: ip } }",
            "wfl": "rule broken { events { a : conn_events } }"
        }))
        .to_request();
    let wfusion_invalid_resp = test::call_service(&app, wfusion_invalid_req).await;
    assert_eq!(wfusion_invalid_resp.status(), StatusCode::OK);
    let wfusion_invalid_body: serde_json::Value = test::read_body_json(wfusion_invalid_resp).await;
    assert_eq!(wfusion_invalid_body["success"], false);
    assert!(wfusion_invalid_body["stage"].as_str().is_some());
    let first_diagnostic = &wfusion_invalid_body["diagnostics"][0];
    assert_eq!(first_diagnostic["category"], "syntax");
    assert_eq!(first_diagnostic["file"], "rules/editor.wfl");
    assert!(first_diagnostic["line"].as_u64().is_some());
    assert!(first_diagnostic["column"].as_u64().is_some());
    assert!(first_diagnostic["message"].as_str().is_some());
    assert!(first_diagnostic["snippet"].as_str().is_some());

    cleanup_knowledge_entry(&know_file);
}
