use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use rand::Rng;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{DeviceStatus, NewDevice, create_device, delete_device};
use wp_station_migrations::entity::device::Entity as DeviceEntity;
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};

async fn cleanup_release(version: &str) {
    let pool = wp_station::db::get_pool();
    let _ = ReleaseEntity::delete_many()
        .filter(ReleaseColumn::Version.eq(version))
        .exec(pool.inner())
        .await;
}

async fn hard_delete_device(id: i32) {
    let pool = wp_station::db::get_pool();
    let _ = DeviceEntity::delete_by_id(id).exec(pool.inner()).await;
}

async fn seed_devices(count: usize) -> Vec<i32> {
    let mut rng = rand::thread_rng();
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let new_device = NewDevice {
            name: Some(format!("release-api-device-{}", rand_suffix())),
            ip: "127.0.0.1".to_string(),
            port: rng.gen_range(2000..9000),
            remark: Some("release api test".to_string()),
            token: format!("token-{}", rand_suffix()),
            status: Some(DeviceStatus::Active),
        };
        let id = create_device(new_device)
            .await
            .expect("seed release api device");
        ids.push(id);
    }
    ids
}

async fn cleanup_devices(ids: &[i32]) {
    for id in ids {
        let _ = delete_device(*id).await;
        hard_delete_device(*id).await;
    }
}

#[actix_web::test]
async fn test_release_api_end_to_end_flow() {
    setup_db().await;
    let requested_version = format!("REL-{}", rand_suffix());

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
            "version": requested_version,
            "pipeline": "pipeline-a",
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

    // Diff before publish covers the WAIT branch
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

    let device_ids = seed_devices(2).await;
    let publish_req = test::TestRequest::post()
        .uri(&format!("/api/releases/{}/publish", rel_id))
        .set_json(&serde_json::json!({ "device_ids": device_ids }))
        .to_request();
    let publish_resp = test::call_service(&app, publish_req).await;
    assert_eq!(publish_resp.status(), StatusCode::OK);

    // Diff after publish covers the PASS branch
    let after_diff_req = test::TestRequest::get()
        .uri(&format!("/api/releases/{}/diff", rel_id))
        .to_request();
    let after_diff_resp = test::call_service(&app, after_diff_req).await;
    assert_eq!(after_diff_resp.status(), StatusCode::OK);

    cleanup_devices(&device_ids).await;
    cleanup_release(&actual_version).await;
}
