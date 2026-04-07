use crate::common::setup_db;
use actix_web::{App, http::StatusCode, test};

#[actix_web::test]
async fn test_system_hello_endpoint() {
    setup_db().await;
    let app = test::init_service(App::new().service(wp_station::api::hello)).await;

    let req = test::TestRequest::get().uri("/api/hello").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let body = test::read_body(resp).await;
    let message: String = serde_json::from_slice(&body).expect("parse hello response");
    assert!(message.contains("Actix-web"));
}

#[actix_web::test]
async fn test_system_version_endpoint() {
    setup_db().await;
    let app = test::init_service(App::new().service(wp_station::api::get_version)).await;

    let req = test::TestRequest::get().uri("/api/version").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);

    let payload: serde_json::Value = test::read_body_json(resp).await;
    assert!(payload.get("wp_station").is_some());
    assert!(payload.get("wp_parse").is_some());
}
