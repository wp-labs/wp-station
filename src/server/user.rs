// 用户管理业务逻辑层

use crate::db;
use crate::error::AppError;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, write_operation_log_for_result,
};
use crate::utils::pagination::{PageQuery, PageResponse};
use bcrypt::{DEFAULT_COST, hash, verify};
use rand::Rng;
use serde::{Deserialize, Serialize};

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct UserListQuery {
    pub keyword: Option<String>,
    pub role: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

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

#[derive(Deserialize, Serialize)]
pub struct UpdateUserRequest {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Deserialize, Serialize)]
pub struct UpdateUserStatusRequest {
    pub status: String,
}

#[derive(Deserialize, Serialize)]
pub struct ResetPasswordRequest {}

#[derive(Deserialize, Serialize)]
pub struct ChangePasswordRequest {
    /// 旧密码仅用于校验，不写入日志
    pub old_password: String,
    /// 新密码仅用于更新，不写入日志
    pub new_password: String,
    pub confirm_password: String,
}

#[derive(Deserialize, Serialize)]
pub struct LoginRequest {
    pub username: String,
    /// 登录密码仅用于校验，不写入日志
    pub password: String,
}

// ============ 响应结构体 ============

pub type UserListResponse = PageResponse<db::User>;

#[derive(Serialize)]
pub struct UserCreated {
    pub id: i32,
}

#[derive(Serialize)]
pub struct ResetPasswordResponse {
    pub new_password: String,
}

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
    let mut rng = rand::thread_rng();

    // 定义字符集
    let uppercase = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let lowercase = b"abcdefghijklmnopqrstuvwxyz";
    let digits = b"0123456789";
    let special = b"!@#$%^&*";

    let mut password = Vec::new();

    // 确保每种字符至少有2个
    for _ in 0..2 {
        password.push(uppercase[rng.gen_range(0..uppercase.len())]);
    }
    for _ in 0..2 {
        password.push(lowercase[rng.gen_range(0..lowercase.len())]);
    }
    for _ in 0..2 {
        password.push(digits[rng.gen_range(0..digits.len())]);
    }
    for _ in 0..2 {
        password.push(special[rng.gen_range(0..special.len())]);
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
        password.push(all_chars[rng.gen_range(0..all_chars.len())]);
    }

    // 打乱顺序
    for i in (1..password.len()).rev() {
        let j = rng.gen_range(0..=i);
        password.swap(i, j);
    }

    password.into_iter().map(char::from).collect()
}

// ============ 业务逻辑函数 ============

/// 获取用户列表（支持关键字/角色/状态筛选 + 分页）
pub async fn list_users_logic(query: UserListQuery) -> Result<UserListResponse, AppError> {
    debug!(
        "获取用户列表: keyword={:?}, role={:?}, status={:?}",
        query.keyword, query.role, query.status
    );

    let (page, page_size) = query.page.normalize_default();

    let (items, total) = db::find_users_page(
        query.keyword.as_deref(),
        query.role.as_deref(),
        query.status.as_deref(),
        page,
        page_size,
    )
    .await?;

    debug!(
        "获取用户列表成功: 共 {} 条, page={}, page_size={}",
        total, page, page_size
    );

    Ok(UserListResponse::from_db(items, total, page, page_size))
}

/// 创建用户
pub async fn create_user_logic(req: CreateUserRequest) -> Result<UserCreated, AppError> {
    info!("创建用户: username={}", req.username);

    let username = req.username.clone();
    let role = req.role.clone();
    let email = req.email.clone();
    let display_name = req.display_name.clone();

    let result = async move {
        // 检查用户名是否已存在
        if let Some(_existing) = db::find_user_by_username(&req.username).await? {
            return Err(AppError::validation(format!(
                "用户名 {} 已存在",
                req.username
            )));
        }

        // 加密密码
        let password_hash = hash_password(&req.password)?;

        let new_user = db::NewUser {
            username: req.username.clone(),
            password: password_hash,
            display_name: req.display_name,
            email: req.email,
            role: req.role,
            remark: req.remark,
        };

        let id = db::create_user(new_user).await?;

        info!("用户创建成功: id={}, username={}", id, req.username);

        Ok::<_, AppError>(UserCreated { id })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name(username)
            .with_field("role", role)
            .with_field("email", email.unwrap_or_else(|| "-".to_string()))
            .with_field(
                "display_name",
                display_name.unwrap_or_else(|| "-".to_string()),
            ),
        &result,
    )
    .await;

    result
}

/// 编辑用户基本信息
pub async fn update_user_logic(id: i32, req: UpdateUserRequest) -> Result<(), AppError> {
    info!("更新用户: id={}", id);

    let display_name = req.display_name.clone();
    let email = req.email.clone();
    let role = req.role.clone();
    let remark = req.remark.clone();

    let result = async move {
        let update_data = db::UpdateUser {
            display_name: req.display_name,
            email: req.email,
            role: req.role,
            remark: req.remark,
        };

        db::update_user(id, update_data).await?;

        info!("更新用户成功: id={}", id);

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::Update,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field(
                "display_name",
                display_name.unwrap_or_else(|| "-".to_string()),
            )
            .with_field("email", email.unwrap_or_else(|| "-".to_string()))
            .with_field("role", role.unwrap_or_else(|| "-".to_string()))
            .with_field(
                "remark",
                remark
                    .and_then(|item| item)
                    .unwrap_or_else(|| "-".to_string()),
            ),
        &result,
    )
    .await;

    result
}

/// 更新用户状态（启用 / 禁用）
pub async fn update_user_status_logic(
    id: i32,
    req: UpdateUserStatusRequest,
) -> Result<(), AppError> {
    info!("更新用户状态: id={}, status={}", id, req.status);

    let status = req.status.clone();
    let result = async {
        // 验证状态值
        if req.status != "active" && req.status != "inactive" {
            return Err(AppError::validation("状态值必须是 active 或 inactive"));
        }

        db::update_user_status(id, &req.status).await?;

        info!("更新用户状态成功: id={}, status={}", id, req.status);

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::Update,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("status", status),
        &result,
    )
    .await;

    result
}

/// 重置用户密码（生成随机强密码）
pub async fn reset_password_logic(
    id: i32,
    _req: ResetPasswordRequest,
) -> Result<ResetPasswordResponse, AppError> {
    info!("重置用户密码: id={}", id);

    let result = async {
        // 生成随机强密码
        let new_password = generate_strong_password();

        // 加密密码
        let password_hash = hash_password(&new_password)?;

        // 更新数据库
        db::reset_user_password(id, password_hash).await?;

        info!("重置用户密码成功: id={}", id);

        // 返回明文密码（仅此一次可见）
        Ok::<_, AppError>(ResetPasswordResponse { new_password })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::ResetPassword,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("mode", "admin-reset"),
        &result,
    )
    .await;

    result
}

/// 修改用户密码
pub async fn change_password_logic(id: i32, req: ChangePasswordRequest) -> Result<(), AppError> {
    info!("修改用户密码: id={}", id);

    let result = async {
        // 验证新密码和确认密码是否一致
        if req.new_password != req.confirm_password {
            return Err(AppError::validation("新密码和确认密码不一致"));
        }

        // 查询用户获取当前密码哈希
        let user = db::find_user_by_id(id)
            .await?
            .ok_or_else(|| AppError::not_found("用户"))?;

        // 验证旧密码
        let is_valid = verify_password(&req.old_password, &user.password)?;
        if !is_valid {
            return Err(AppError::Unauthorized("旧密码错误".to_string()));
        }

        // 加密新密码
        let new_password_hash = hash_password(&req.new_password)?;

        // 更新数据库
        db::change_user_password(id, new_password_hash).await?;

        info!("修改用户密码成功: id={}", id);

        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::ChangePassword,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("mode", "self-change"),
        &result,
    )
    .await;

    result
}

/// 用户登录
pub async fn login_logic(req: LoginRequest) -> Result<LoginResponse, AppError> {
    info!("用户登录: username={}", req.username);

    let username = req.username.clone();
    let result = async {
        // 查询用户
        let user = db::find_user_by_username(&req.username)
            .await?
            .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

        // 验证密码
        let is_valid = verify_password(&req.password, &user.password)?;
        if !is_valid {
            return Err(AppError::Unauthorized("用户名或密码错误".to_string()));
        }

        // 生成简单的 token（使用时间戳 + 随机数）
        let mut rng = rand::thread_rng();
        let random_num: u64 = rng.r#gen();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AppError::internal(format!("获取系统时间失败: {}", e)))?
            .as_secs();
        let token = format!("token_{}_{}", timestamp, random_num);

        info!("用户登录成功: username={}", req.username);

        Ok::<_, AppError>(LoginResponse {
            token,
            username: user.username.clone(),
            display_name: user.display_name.unwrap_or(user.username),
            role: user.role,
        })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::Login,
        OperationLogParams::new()
            .with_operator(username.clone())
            .with_target_name(username)
            .with_field("auth", "password"),
        &result,
    )
    .await;

    result
}

/// 删除用户（软删除）
pub async fn delete_user_logic(id: i32) -> Result<(), AppError> {
    info!("删除用户: id={}", id);

    let result = async {
        db::delete_user(id).await?;

        info!("删除用户成功: id={}", id);
        Ok::<_, AppError>(())
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::User,
        OperationLogAction::Delete,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("delete_mode", "soft"),
        &result,
    )
    .await;

    result
}
