use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use rand::Rng;
use wp_station::db::{DeviceStatus, NewDevice, create_device as create_device_record};
use wp_station::server::{CreateDeviceRequest, UpdateDeviceRequest};

fn device_payload() -> CreateDeviceRequest {
    let mut rng = rand::thread_rng();
    CreateDeviceRequest {
        name: Some(format!("api-device-{}", rand_suffix())),
        ip: "127.0.0.1".to_string(),
        port: rng.gen_range(2000..9000),
        remark: Some("api integration".to_string()),
        token: format!("token-{}", rand_suffix()),
    }
}

#[actix_web::test]
async fn test_list_devices_endpoints() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::list_devices)
            .service(wp_station::api::list_online_devices)
            .service(wp_station::api::create_device)
            .service(wp_station::api::update_device)
            .service(wp_station::api::delete_device),
    )
    .await;

    let list_req = test::TestRequest::get()
        .uri("/api/devices?page=1&page_size=5")
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    let online_req = test::TestRequest::get()
        .uri("/api/devices/online")
        .to_request();
    let online_resp = test::call_service(&app, online_req).await;
    assert_eq!(online_resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn test_device_crud_flow_via_api() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::list_devices)
            .service(wp_station::api::list_online_devices)
            .service(wp_station::api::create_device)
            .service(wp_station::api::update_device)
            .service(wp_station::api::delete_device),
    )
    .await;

    let create_body = device_payload();
    let device_id = create_device_record(NewDevice {
        name: create_body.name.clone(),
        ip: create_body.ip.clone(),
        port: create_body.port,
        remark: create_body.remark.clone(),
        token: create_body.token.clone(),
        status: Some(DeviceStatus::Inactive),
    })
    .await
    .expect("seed device");

    let update_req = test::TestRequest::put()
        .uri("/api/devices")
        .set_json(&UpdateDeviceRequest {
            id: device_id,
            name: Some(format!("updated-{}", rand_suffix())),
            ip: create_body.ip.clone(),
            port: create_body.port + 1,
            remark: Some("updated via api".to_string()),
            token: create_body.token.clone(),
        })
        .to_request();
    let update_resp = test::call_service(&app, update_req).await;
    assert_eq!(update_resp.status(), StatusCode::OK);
    let update_body: serde_json::Value = test::read_body_json(update_resp).await;
    assert_eq!(
        update_body.get("success").and_then(|v| v.as_bool()),
        Some(true)
    );

    let delete_req = test::TestRequest::delete()
        .uri(&format!("/api/devices/{}", device_id))
        .to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);
}
