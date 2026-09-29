//! 用户管理 API。
//!
//! 提供用户管理、认证和密码相关入口。

use actix_web::{HttpResponse, delete, get, post, put, web};

use crate::error::AppError;
use crate::server::{
    ChangePasswordRequest, CreateUserRequest, LoginRequest, ResetPasswordRequest,
    UpdateUserRequest, UpdateUserStatusRequest, UserListQuery, change_password_logic,
    create_user_logic, delete_user_logic, list_users_logic, login_logic, reset_password_logic,
    update_user_logic, update_user_status_logic,
};

#[get("/api/users")]
/// 用户管理：获取用户列表。
pub async fn list_users(query: web::Query<UserListQuery>) -> Result<HttpResponse, AppError> {
    let resp = list_users_logic(query.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/users")]
/// 用户管理：创建用户。
pub async fn create_user(req: web::Json<CreateUserRequest>) -> Result<HttpResponse, AppError> {
    let resp = create_user_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[put("/api/users/{id}")]
/// 用户管理：编辑用户基本信息。
pub async fn update_user(
    path: web::Path<i32>,
    req: web::Json<UpdateUserRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    update_user_logic(id, req.into_inner()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[put("/api/users/{id}/status")]
/// 用户管理：更新用户状态。
pub async fn update_user_status(
    path: web::Path<i32>,
    req: web::Json<UpdateUserStatusRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    update_user_status_logic(id, req.into_inner()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/users/{id}/reset-password")]
/// 用户管理：重置用户密码。
pub async fn reset_user_password(
    path: web::Path<i32>,
    req: web::Json<ResetPasswordRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    let resp = reset_password_logic(id, req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[post("/api/users/{id}/change-password")]
/// 用户管理：修改用户密码。
pub async fn change_user_password(
    path: web::Path<i32>,
    req: web::Json<ChangePasswordRequest>,
) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    change_password_logic(id, req.into_inner()).await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/api/auth/login")]
/// 用户登录。
pub async fn login(req: web::Json<LoginRequest>) -> Result<HttpResponse, AppError> {
    let resp = login_logic(req.into_inner()).await?;
    Ok(HttpResponse::Ok().json(resp))
}

#[delete("/api/users/{id}")]
/// 用户管理：删除用户。
pub async fn delete_user(path: web::Path<i32>) -> Result<HttpResponse, AppError> {
    let id = path.into_inner();
    delete_user_logic(id).await?;
    Ok(HttpResponse::NoContent().finish())
}
