//! 用户认证与密码修改相关业务。

use crate::db;
use crate::error::AppError;
use rand::RngExt;

use super::{ChangePasswordRequest, LoginRequest, LoginResponse, hash_password, verify_password};

/// 修改用户密码。
pub async fn change_password_logic(id: i32, req: ChangePasswordRequest) -> Result<(), AppError> {
    info!("修改用户密码: id={}", id);

    async {
        if req.new_password != req.confirm_password {
            return Err(AppError::validation("新密码和确认密码不一致"));
        }

        let user = db::find_user_by_id(id)
            .await?
            .ok_or_else(|| AppError::not_found("用户"))?;

        let is_valid = verify_password(&req.old_password, &user.password)?;
        if !is_valid {
            return Err(AppError::Unauthorized("旧密码错误".to_string()));
        }

        let new_password_hash = hash_password(&req.new_password)?;
        db::change_user_password(id, new_password_hash).await?;

        info!("修改用户密码成功: id={}", id);
        Ok::<_, AppError>(())
    }
    .await
}

/// 用户登录。
pub async fn login_logic(req: LoginRequest) -> Result<LoginResponse, AppError> {
    info!("用户登录: username={}", req.username);

    async {
        let user = db::find_user_by_username(&req.username)
            .await?
            .ok_or_else(|| AppError::Unauthorized("用户名或密码错误".to_string()))?;

        let is_valid = verify_password(&req.password, &user.password)?;
        if !is_valid {
            return Err(AppError::Unauthorized("用户名或密码错误".to_string()));
        }

        let mut rng = rand::rng();
        let random_num: u64 = rng.random();
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
    .await
}
