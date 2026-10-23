use crate::common::{rand_suffix, remove_project_path, resolve_project_path, setup_db};
use actix_web::{App, http::StatusCode, test};
use std::fs;

fn cleanup_source(file: &str) {
    remove_project_path(format!("topology/sources/{file}"));
}

fn write_source_connector_template(file: &str, content: &str) {
    let path = resolve_project_path(format!("connectors/source.d/{file}"));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create source connector parent");
    }
    fs::write(path, content).expect("write source connector template");
}

#[actix_web::test]
async fn test_config_file_crud_via_api() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::get_config_files)
            .service(wp_station::api::get_config)
            .service(wp_station::api::create_config_file)
            .service(wp_station::api::save_config)
            .service(wp_station::api::delete_config_file),
    )
    .await;
    let file = format!("api-config-{}.toml", rand_suffix());

    let create_req = test::TestRequest::post()
        .uri("/api/config/files")
        .set_json(serde_json::json!({
            "system": "wparse",
            "rule_type": "source",
            "file": file.clone(),
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::OK);

    let list_uri = format!(
        "/api/config/files?system=wparse&rule_type=source&keyword={}",
        file
    );
    let list_req = test::TestRequest::get().uri(&list_uri).to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let save_req = test::TestRequest::post()
        .uri("/api/config")
        .set_json(serde_json::json!({
            "system": "wparse",
            "rule_type": "source",
            "file": file.clone(),
            "content": format!("[[sources]]\nkey = \"{}\"", rand_suffix()),
        }))
        .to_request();
    let save_resp = test::call_service(&app, save_req).await;
    assert_eq!(save_resp.status(), StatusCode::OK);

    let get_uri = format!("/api/config?system=wparse&rule_type=source&file={}", file);
    let get_req = test::TestRequest::get().uri(&get_uri).to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let payload: serde_json::Value = test::read_body_json(get_resp).await;
    assert_eq!(
        payload.get("file").and_then(|f| f.as_str()),
        Some(file.as_str())
    );

    let delete_uri = format!(
        "/api/config/files?system=wparse&rule_type=source&file={}",
        file
    );
    let delete_req = test::TestRequest::delete().uri(&delete_uri).to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), StatusCode::OK);

    cleanup_source(&file);
}

#[actix_web::test]
async fn test_get_config_not_found_returns_placeholder() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::get_config_files)
            .service(wp_station::api::get_config)
            .service(wp_station::api::create_config_file)
            .service(wp_station::api::save_config)
            .service(wp_station::api::delete_config_file),
    )
    .await;

    let req = test::TestRequest::get()
        .uri("/api/config?system=wparse&rule_type=parse&file=non-existent.toml")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        payload.get("file").and_then(|f| f.as_str()),
        Some("non-existent.toml")
    );
}

#[actix_web::test]
async fn test_get_source_templates_via_api() {
    setup_db().await;
    write_source_connector_template(
        "51-dmdb-endpoint.toml",
        r#"[[connectors]]
id = "dmdb_endpoint_src"
type = "dmdb"
allow_override = ["endpoint", "driver", "username", "password", "table", "cursor_column", "cursor_type", "start_from", "batch", "poll_interval_ms", "error_backoff_ms", "connect_timeout_secs", "query_timeout_secs"]
[connectors.params]
endpoint = "159.75.175.212:5236"
driver = "DM8 ODBC DRIVER"
username = "SYSDBA"
password = "SYSDBA"
table = "nginx_logs"
cursor_column = "id"
cursor_type = "int"
start_from = "0"
batch = 512
poll_interval_ms = 1000
error_backoff_ms = 2000
connect_timeout_secs = 8
query_timeout_secs = 15
"#,
    );
    let app = test::init_service(App::new().service(wp_station::api::get_config_templates)).await;

    let req = test::TestRequest::get()
        .uri("/api/config/templates?scope=source")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: serde_json::Value = test::read_body_json(resp).await;
    let items = payload
        .get("items")
        .and_then(|value| value.as_array())
        .expect("items should be an array");

    assert!(
        items.iter().any(|item| {
            item.get("template_id").and_then(|value| value.as_str()) == Some("dmdb-endpoint")
        }),
        "source templates should include dmdb-endpoint"
    );
}

#[actix_web::test]
async fn test_render_source_template_omits_advanced_fields() {
    setup_db().await;
    write_source_connector_template(
        "51-dmdb-endpoint.toml",
        r#"[[connectors]]
id = "dmdb_endpoint_src"
type = "dmdb"
allow_override = ["endpoint", "driver", "username", "password", "table", "cursor_column", "cursor_type", "start_from", "batch", "poll_interval_ms", "error_backoff_ms", "connect_timeout_secs", "query_timeout_secs"]
[connectors.params]
endpoint = "159.75.175.212:5236"
driver = "DM8 ODBC DRIVER"
username = "SYSDBA"
password = "SYSDBA"
table = "nginx_logs"
cursor_column = "id"
cursor_type = "int"
start_from = "0"
batch = 512
poll_interval_ms = 1000
error_backoff_ms = 2000
connect_timeout_secs = 8
query_timeout_secs = 15
"#,
    );
    let app = test::init_service(App::new().service(wp_station::api::render_config_template)).await;

    let req = test::TestRequest::post()
        .uri("/api/config/templates/render")
        .set_json(serde_json::json!({
            "scope": "source",
            "template_id": "dmdb-endpoint",
            "content": ""
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: serde_json::Value = test::read_body_json(resp).await;
    let snippet = payload
        .get("snippet")
        .and_then(|value| value.as_str())
        .expect("snippet should be string");
    let omitted_fields = payload
        .get("omitted_fields")
        .and_then(|value| value.as_array())
        .expect("omitted_fields should be array");

    assert!(snippet.contains("connect = \"dmdb_endpoint_src\""));
    assert!(snippet.contains("cursor_column = \"id\""));
    assert!(!snippet.contains("batch = 512"));
    assert!(
        omitted_fields
            .iter()
            .any(|field| field.as_str() == Some("batch"))
    );
    assert!(
        omitted_fields
            .iter()
            .any(|field| field.as_str() == Some("query_timeout_secs"))
    );
}

#[actix_web::test]
async fn test_render_sink_template_adds_skeleton_and_unique_name() {
    setup_db().await;
    let app = test::init_service(App::new().service(wp_station::api::render_config_template)).await;

    let req = test::TestRequest::post()
        .uri("/api/config/templates/render")
        .set_json(serde_json::json!({
            "scope": "sink",
            "template_id": "elasticsearch",
            "content": "[[sink_group.sinks]]\nname = \"all_elasticsearch\"\nconnect = \"elasticsearch_sink\"\n"
        }))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: serde_json::Value = test::read_body_json(resp).await;
    let snippet = payload
        .get("snippet")
        .and_then(|value| value.as_str())
        .expect("snippet should be string");
    let content = payload
        .get("content")
        .and_then(|value| value.as_str())
        .expect("content should be string");
    let instance_name = payload
        .get("instance_name")
        .and_then(|value| value.as_str())
        .expect("instance_name should be string");

    assert_eq!(instance_name, "all_elasticsearch_2");
    assert!(snippet.contains("version = \"1.0\""));
    assert!(snippet.contains("[sink_group]"));
    assert!(snippet.contains("name = \"all_elasticsearch_2\""));
    assert!(content.contains("name = \"all_elasticsearch_2\""));
}
