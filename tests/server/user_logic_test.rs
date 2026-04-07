use crate::common::{rand_suffix, setup_db};
use wp_station::server::user::{
    ChangePasswordRequest, CreateUserRequest, LoginRequest, ResetPasswordRequest,
    UpdateUserRequest, UpdateUserStatusRequest, UserListQuery, change_password_logic,
    create_user_logic, delete_user_logic, list_users_logic, login_logic, reset_password_logic,
    update_user_logic, update_user_status_logic,
};
use wp_station::utils::pagination::PageQuery;

#[tokio::test]
async fn test_full_user_logic_flow() {
    setup_db().await;
    let username = format!("logic-user-{}", rand_suffix());

    let create_req = CreateUserRequest {
        username: username.clone(),
        password: "InitPass123!!".to_string(),
        display_name: Some("Logic User".to_string()),
        email: Some("logic@example.com".to_string()),
        role: "operator".to_string(),
        remark: Some("from server test".to_string()),
    };

    let created = create_user_logic(create_req)
        .await
        .expect("create user via logic");
    let user_id = created.id;
    assert!(user_id > 0);

    let list_resp = list_users_logic(UserListQuery {
        keyword: Some(username.clone()),
        role: Some("operator".to_string()),
        status: Some("active".to_string()),
        page: PageQuery {
            page: Some(1),
            page_size: Some(10),
        },
    })
    .await
    .expect("list users via logic");
    assert!(list_resp.items.iter().any(|u| u.username == username));

    update_user_logic(
        user_id,
        UpdateUserRequest {
            display_name: Some("Updated Name".to_string()),
            email: Some("updated@example.com".to_string()),
            role: Some("admin".to_string()),
            remark: Some(Some("note".to_string())),
        },
    )
    .await
    .expect("update user logic");

    update_user_status_logic(
        user_id,
        UpdateUserStatusRequest {
            status: "inactive".to_string(),
        },
    )
    .await
    .expect("update status logic");

    let reset_resp = reset_password_logic(user_id, ResetPasswordRequest {})
        .await
        .expect("reset password logic");
    let temporary_password = reset_resp.new_password.clone();

    change_password_logic(
        user_id,
        ChangePasswordRequest {
            old_password: temporary_password,
            new_password: "BrandNewPass456!!".to_string(),
            confirm_password: "BrandNewPass456!!".to_string(),
        },
    )
    .await
    .expect("change password logic");

    update_user_status_logic(
        user_id,
        UpdateUserStatusRequest {
            status: "active".to_string(),
        },
    )
    .await
    .expect("reactivate user before login");

    let login_resp = login_logic(LoginRequest {
        username: username.clone(),
        password: "BrandNewPass456!!".to_string(),
    })
    .await
    .expect("login logic");
    assert_eq!(login_resp.username, username);
    assert!(!login_resp.token.is_empty());

    delete_user_logic(user_id).await.expect("delete user logic");
}
