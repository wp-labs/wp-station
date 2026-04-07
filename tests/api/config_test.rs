use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station_migrations::entity::rule_config::{Column as RuleColumn, Entity as RuleEntity};

async fn cleanup_rule(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = RuleEntity::delete_many()
        .filter(RuleColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
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
        .set_json(&serde_json::json!({
            "rule_type": "source",
            "file": file.clone(),
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::OK);

    let list_uri = format!("/api/config/files?rule_type=source&keyword={}", file);
    let list_req = test::TestRequest::get().uri(&list_uri).to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let save_req = test::TestRequest::post()
        .uri("/api/config")
        .set_json(&serde_json::json!({
            "rule_type": "source",
            "file": file.clone(),
            "content": format!("[[sources]]\nkey = \"{}\"", rand_suffix()),
        }))
        .to_request();
    let save_resp = test::call_service(&app, save_req).await;
    assert_eq!(save_resp.status(), StatusCode::OK);

    let get_uri = format!("/api/config?rule_type=source&file={}", file);
    let get_req = test::TestRequest::get().uri(&get_uri).to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);
    let payload: serde_json::Value = test::read_body_json(get_resp).await;
    assert_eq!(
        payload.get("file").and_then(|f| f.as_str()),
        Some(file.as_str())
    );

    let delete_uri = format!("/api/config/files?rule_type=source&file={}", file);
    let delete_req = test::TestRequest::delete().uri(&delete_uri).to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), StatusCode::OK);

    cleanup_rule(&file).await;
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
        .uri("/api/config?rule_type=parse&file=non-existent.toml")
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(
        payload.get("file").and_then(|f| f.as_str()),
        Some("non-existent.toml")
    );
}
