//! 用户管理业务逻辑层。
//!
//! 统一承接用户列表、账号维护、密码处理和登录相关的数据结构。

mod account;
mod auth;

use crate::db;
use crate::error::AppError;
use crate::utils::pagination::{PageQuery, PageResponse};
use bcrypt::{DEFAULT_COST, hash, verify};
use rand::RngExt;
use serde::{Deserialize, Serialize};

pub use self::account::{
    create_user_logic, delete_user_logic, list_users_logic, reset_password_logic,
    update_user_logic, update_user_status_logic,
};
pub use self::auth::{change_password_logic, login_logic};

// ============ 请求参数结构体 ============

/// 用户列表查询参数。
#[derive(Deserialize)]
pub struct UserListQuery {
    pub keyword: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

/// 创建用户请求体。
#[derive(Deserialize, Serialize)]
pub struct CreateUserRequest {
    pub username: String,
    /// 明文密码仅用于创建时输入，不写入日志
    pub password: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: String,
    pub remark: Option<String>,
}

/// 更新用户请求体。
#[derive(Deserialize, Serialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub remark: Option<Option<String>>,
}

/// 更新用户状态请求体。
#[derive(Deserialize, Serialize)]
pub struct UpdateUserStatusRequest {
    pub status: String,
}

/// 重置密码请求体。
#[derive(Deserialize, Serialize)]
pub struct ResetPasswordRequest {}

/// 修改密码请求体。
#[derive(Deserialize, Serialize)]
pub struct ChangePasswordRequest {
    /// 旧密码仅用于校验，不写入日志
    pub old_password: String,
    /// 新密码仅用于更新，不写入日志
    pub new_password: String,
    pub confirm_password: String,
}

/// 登录请求体。
#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    /// 登录密码仅用于校验，不写入日志
    pub password: String,
}

// ============ 响应结构体 ============

pub type UserListResponse = PageResponse<db::User>;

/// 创建用户响应体。
#[derive(Serialize)]
pub struct UserCreated {
    pub id: i32,
}

/// 重置密码响应体。
#[derive(Serialize)]
pub struct ResetPasswordResponse {
    pub new_password: String,
}

/// 登录响应体。
#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
}

// ============ 密码处理函数 ============

/// 使用 bcrypt 加密密码
fn hash_password(password: &str) -> Result<String, AppError> {
    hash(password, DEFAULT_COST).map_err(|e| AppError::internal(format!("密码加密失败: {}", e)))
}

/// 使用 bcrypt 验证密码
fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    verify(password, hash).map_err(|e| AppError::internal(format!("密码验证失败: {}", e)))
}

/// 生成强随机密码（16位：大写字母、小写字母、数字、特殊字符各至少2个）
fn generate_strong_password() -> String {
    let mut rng = rand::rng();

    // 定义字符集
    let uppercase = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lowercase = b"abcdefghijklmnopqrstuvwxyz";
    let digits = b"0123456789";
    let special = b"!@#$%^&*";

    let mut password = Vec::new();

    // 确保每种字符至少有2个
    for _ in 0..2 {
        password.push(uppercase[rng.random_range(0..uppercase.len())]);
    }
    for _ in 0..2 {
        password.push(lowercase[rng.random_range(0..lowercase.len())]);
    }
    for _ in 0..2 {
        password.push(digits[rng.random_range(0..digits.len())]);
    }
    for _ in 0..2 {
        password.push(special[rng.random_range(0..special.len())]);
    }

    // 剩余8位从所有字符集中随机选择
    let all_chars: Vec<u8> = uppercase
        .iter()
        .chain(lowercase.iter())
        .chain(digits.iter())
        .chain(special.iter())
        .copied()
        .collect();

    for _ in 0..8 {
        password.push(all_chars[rng.random_range(0..all_chars.len())]);
    }

    // 打乱顺序
    for i in (1..password.len()).rev() {
        let j = rng.random_range(0..=i);
        password.swap(i, j);
    }

    password.into_iter().map(char::from).collect()
}

// ============ 业务逻辑函数 ============
