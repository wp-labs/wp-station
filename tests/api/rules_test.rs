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
async fn test_rule_file_crud_and_validation() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::get_rule_files)
            .service(wp_station::api::get_rule_content)
            .service(wp_station::api::create_rule_file)
            .service(wp_station::api::save_rule)
            .service(wp_station::api::delete_rule_file)
            .service(wp_station::api::validate_rule),
    )
    .await;
    let file = format!("api-rule-{}.toml", rand_suffix());

    let create_req = test::TestRequest::post()
        .uri("/api/config/rules/files")
        .set_json(&serde_json::json!({
            "rule_type": "wpl",
            "file": file.clone(),
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::NO_CONTENT);

    let save_req = test::TestRequest::post()
        .uri("/api/config/rules/save")
        .set_json(&serde_json::json!({
            "rule_type": "wpl",
            "file": file.clone(),
            "content": "package test { rule sample { digit:a } }",
        }))
        .to_request();
    let save_resp = test::call_service(&app, save_req).await;
    assert_eq!(save_resp.status(), StatusCode::NO_CONTENT);

    let list_uri = "/api/config/rules/files?rule_type=wpl&page=1&page_size=10";
    let list_req = test::TestRequest::get().uri(list_uri).to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let items: serde_json::Value = test::read_body_json(list_resp).await;
    assert!(
        items["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["file"] == file)
    );

    let get_uri = format!("/api/config/rules?rule_type=wpl&file={}", file);
    let get_req = test::TestRequest::get().uri(&get_uri).to_request();
    let get_resp = test::call_service(&app, get_req).await;
    assert_eq!(get_resp.status(), StatusCode::OK);

    let validate_req = test::TestRequest::post()
        .uri("/api/config/rules/validate")
        .set_json(&serde_json::json!({
            "rule_type": "wpl",
            "file": file.clone(),
        }))
        .to_request();
    let validate_resp = test::call_service(&app, validate_req).await;
    assert_eq!(validate_resp.status(), StatusCode::OK);
    let validation: serde_json::Value = test::read_body_json(validate_resp).await;
    assert!(
        validation.get("valid").is_some(),
        "validation response should include a valid flag"
    );

    let delete_uri = format!("/api/config/rules/files?rule_type=wpl&file={}", file);
    let delete_req = test::TestRequest::delete().uri(&delete_uri).to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);

    cleanup_rule(&file).await;
}
