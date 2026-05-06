use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};

async fn cleanup_release(version: &str) {
    let pool = wp_station::db::get_pool();
    let _ = ReleaseEntity::delete_many()
        .filter(ReleaseColumn::Version.eq(version))
        .exec(pool.inner())
        .await;
}

#[actix_web::test]
async fn test_release_api_end_to_end_flow() {
    setup_db().await;
    let requested_pipeline = format!("pipeline-{}", rand_suffix());

    let app = test::init_service(
        App::new()
            .service(wp_station::api::list_releases)
            .service(wp_station::api::get_release_detail)
            .service(wp_station::api::create_release)
            .service(wp_station::api::validate_release)
            .service(wp_station::api::publish_release)
            .service(wp_station::api::get_release_diff),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri("/api/releases")
        .set_json(&serde_json::json!({
            "pipeline": requested_pipeline,
            "note": "api release"
        }))
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_payload: serde_json::Value = test::read_body_json(create_resp).await;
    let rel_id = create_payload["id"].as_i64().unwrap() as i32;

    let detail_req = test::TestRequest::get()
        .uri(&format!("/api/releases/{}", rel_id))
        .to_request();
    let detail_resp = test::call_service(&app, detail_req).await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_payload: serde_json::Value = test::read_body_json(detail_resp).await;
    let actual_version = detail_payload["version"]
        .as_str()
        .expect("version field")
        .to_string();

    let list_uri = format!(
        "/api/releases?page=1&page_size=5&version={}",
        actual_version
    );
    let list_req = test::TestRequest::get().uri(&list_uri).to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_body: serde_json::Value = test::read_body_json(list_resp).await;
    assert!(
        list_body["items"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .any(|item| item["id"].as_i64() == Some(rel_id as i64))
    );

    // 草稿阶段 diff 和校验在纯测试环境中可独立验证。
    let diff_req = test::TestRequest::get()
        .uri(&format!("/api/releases/{}/diff", rel_id))
        .to_request();
    let diff_resp = test::call_service(&app, diff_req).await;
    assert_eq!(diff_resp.status(), StatusCode::OK);

    let validate_req = test::TestRequest::post()
        .uri(&format!("/api/releases/{}/validate", rel_id))
        .set_json(&serde_json::json!({ "rule_type": null }))
        .to_request();
    let validate_resp = test::call_service(&app, validate_req).await;
    assert_eq!(validate_resp.status(), StatusCode::OK);
    cleanup_release(&actual_version).await;
}
