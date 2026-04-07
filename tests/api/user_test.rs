use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use wp_station::server::{
    ChangePasswordRequest, CreateUserRequest, LoginRequest, ResetPasswordRequest,
    UpdateUserRequest, UpdateUserStatusRequest,
};

#[actix_web::test]
async fn test_user_api_manages_full_lifecycle() {
    setup_db().await;
    let username = format!("api-user-{}", rand_suffix());
    let initial_password = "InitPassw0rd!";

    let app = test::init_service(
        App::new()
            .service(wp_station::api::list_users)
            .service(wp_station::api::create_user)
            .service(wp_station::api::update_user)
            .service(wp_station::api::update_user_status)
            .service(wp_station::api::reset_user_password)
            .service(wp_station::api::change_user_password)
            .service(wp_station::api::delete_user)
            .service(wp_station::api::login),
    )
    .await;

    let create_req = test::TestRequest::post()
        .uri("/api/users")
        .set_json(&CreateUserRequest {
            username: username.clone(),
            password: initial_password.to_string(),
            display_name: Some("API User".to_string()),
            email: Some("user@example.com".to_string()),
            role: "admin".to_string(),
            remark: Some("created via api test".to_string()),
        })
        .to_request();
    let create_resp = test::call_service(&app, create_req).await;
    assert_eq!(create_resp.status(), StatusCode::OK);
    let create_body: serde_json::Value = test::read_body_json(create_resp).await;
    let user_id = create_body["id"].as_i64().unwrap() as i32;

    // List users to ensure new user exists
    let list_req = test::TestRequest::get()
        .uri("/api/users?page=1&page_size=5")
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);

    // Update user profile
    let update_req = test::TestRequest::put()
        .uri(&format!("/api/users/{}", user_id))
        .set_json(&UpdateUserRequest {
            display_name: Some("Updated User".to_string()),
            email: Some("updated@example.com".to_string()),
            role: Some("viewer".to_string()),
            remark: Some(Some("updated remark".to_string())),
        })
        .to_request();
    let update_resp = test::call_service(&app, update_req).await;
    assert_eq!(update_resp.status(), StatusCode::NO_CONTENT);

    // Disable the user and then reset password
    let status_req = test::TestRequest::put()
        .uri(&format!("/api/users/{}/status", user_id))
        .set_json(&UpdateUserStatusRequest {
            status: "inactive".to_string(),
        })
        .to_request();
    let status_resp = test::call_service(&app, status_req).await;
    assert_eq!(status_resp.status(), StatusCode::NO_CONTENT);

    let reset_req = test::TestRequest::post()
        .uri(&format!("/api/users/{}/reset-password", user_id))
        .set_json(&ResetPasswordRequest {})
        .to_request();
    let reset_resp = test::call_service(&app, reset_req).await;
    assert_eq!(reset_resp.status(), StatusCode::OK);
    let reset_body: serde_json::Value = test::read_body_json(reset_resp).await;
    let temp_password = reset_body["new_password"]
        .as_str()
        .expect("new password from reset")
        .to_string();

    // Change password to a known value
    let change_req = test::TestRequest::post()
        .uri(&format!("/api/users/{}/change-password", user_id))
        .set_json(&ChangePasswordRequest {
            old_password: temp_password.clone(),
            new_password: "NewStrongPass1!".to_string(),
            confirm_password: "NewStrongPass1!".to_string(),
        })
        .to_request();
    let change_resp = test::call_service(&app, change_req).await;
    assert_eq!(change_resp.status(), StatusCode::NO_CONTENT);

    // Reactivate user before attempting login
    let activate_req = test::TestRequest::put()
        .uri(&format!("/api/users/{}/status", user_id))
        .set_json(&UpdateUserStatusRequest {
            status: "active".to_string(),
        })
        .to_request();
    let activate_resp = test::call_service(&app, activate_req).await;
    assert_eq!(activate_resp.status(), StatusCode::NO_CONTENT);

    // Login with new password
    let login_req = test::TestRequest::post()
        .uri("/api/auth/login")
        .set_json(&LoginRequest {
            username: username.clone(),
            password: "NewStrongPass1!".to_string(),
        })
        .to_request();
    let login_resp = test::call_service(&app, login_req).await;
    assert_eq!(login_resp.status(), StatusCode::OK);

    // Delete user to clean up
    let delete_req = test::TestRequest::delete()
        .uri(&format!("/api/users/{}", user_id))
        .to_request();
    let delete_resp = test::call_service(&app, delete_req).await;
    assert_eq!(delete_resp.status(), StatusCode::NO_CONTENT);
}
