use crate::common::{setup_db, unique_name};
use sea_orm::EntityTrait;
use wp_station::db::{
    NewUser, UpdateUser, change_user_password, create_user, delete_user, find_user_by_id,
    find_user_by_username, find_users_page, reset_user_password, update_user as update_user_record,
    update_user_status,
};
use wp_station_migrations::entity::user::Entity as UserEntity;

async fn cleanup_user(id: i32) {
    let pool = wp_station::db::get_pool();
    let _ = UserEntity::delete_by_id(id).exec(pool.inner()).await;
}

fn new_user_payload() -> NewUser {
    NewUser {
        username: unique_name("user"),
        password: "secret".to_string(),
        display_name: Some("Tester".to_string()),
        email: Some("tester@example.com".to_string()),
        role: "operator".to_string(),
        remark: Some("integration-test".to_string()),
    }
}

async fn insert_user() -> (i32, NewUser) {
    setup_db().await;
    let payload = new_user_payload();
    let id = create_user(payload.clone()).await.expect("create user");
    (id, payload)
}

#[tokio::test]
async fn test_create_and_find_user() {
    let (user_id, payload) = insert_user().await;

    let found = find_user_by_id(user_id)
        .await
        .expect("find user")
        .expect("user exists");
    assert_eq!(found.username, payload.username);
    assert_eq!(found.email, payload.email);

    let by_username = find_user_by_username(&payload.username)
        .await
        .expect("find by username")
        .expect("user by username");
    assert_eq!(by_username.id, user_id);

    let _ = delete_user(user_id).await;
    cleanup_user(user_id).await;
}

#[tokio::test]
async fn test_find_users_page() {
    let (user_id, payload) = insert_user().await;

    let (items, total) = find_users_page(
        Some(&payload.username),
        Some("operator"),
        Some("active"),
        1,
        10,
    )
    .await
    .expect("list users");
    assert!(total >= 1);
    assert!(items.iter().any(|user| user.id == user_id));

    let _ = delete_user(user_id).await;
    cleanup_user(user_id).await;
}

#[tokio::test]
async fn test_update_user_fields() {
    let (user_id, _) = insert_user().await;

    let update = UpdateUser {
        display_name: Some("Updated".to_string()),
        email: Some("updated@example.com".to_string()),
        role: Some("admin".to_string()),
        remark: Some(Some("remark".to_string())),
    };

    update_user_record(user_id, update)
        .await
        .expect("update user");

    let updated = find_user_by_id(user_id)
        .await
        .expect("find updated")
        .expect("exists");
    assert_eq!(updated.display_name.as_deref(), Some("Updated"));
    assert_eq!(updated.role, "admin");
    assert_eq!(updated.remark.as_deref(), Some("remark"));

    let _ = delete_user(user_id).await;
    cleanup_user(user_id).await;
}

#[tokio::test]
async fn test_update_user_status() {
    let (user_id, _) = insert_user().await;

    update_user_status(user_id, "inactive")
        .await
        .expect("update status");

    let user = find_user_by_id(user_id)
        .await
        .expect("find after status")
        .expect("user should still be visible when inactive");
    assert_eq!(user.status, "inactive");

    cleanup_user(user_id).await;
}

#[tokio::test]
async fn test_change_and_reset_password() {
    let (user_id, _payload) = insert_user().await;

    change_user_password(user_id, "new-pass".to_string())
        .await
        .expect("change password");
    let changed = find_user_by_id(user_id)
        .await
        .expect("find after change")
        .expect("exists");
    assert_eq!(changed.password, "new-pass");

    reset_user_password(user_id, "reset-pass".to_string())
        .await
        .expect("reset password");
    let reset = find_user_by_id(user_id)
        .await
        .expect("find after reset")
        .expect("exists");
    assert_eq!(reset.password, "reset-pass");

    let _ = delete_user(user_id).await;
    cleanup_user(user_id).await;
}

#[tokio::test]
async fn test_delete_user_marks_deleted() {
    let (user_id, _) = insert_user().await;

    delete_user(user_id).await.expect("delete user");

    let missing = find_user_by_id(user_id).await.expect("find after delete");
    assert!(missing.is_none());

    cleanup_user(user_id).await;
}
