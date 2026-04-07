// 用户管理数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{Condition, QueryOrder, QuerySelect, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use wp_station_migrations::entity::user::{ActiveModel, Column, Entity, Model};

pub type User = Model;

/// 新建用户输入，仅供数据层写库使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    pub username: String,
    /// 已加密密码，不接收明文。
    pub password: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    /// admin / operator / viewer
    pub role: String,
    pub remark: Option<String>,
}

/// 用户可变更字段集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUser {
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub role: Option<String>,
    pub remark: Option<Option<String>>,
}

// ============ 数据库操作函数 ============

/// 分页查询用户列表，支持按关键字、角色、状态筛选
pub async fn find_users_page(
    keyword: Option<&str>,
    role: Option<&str>,
    status: Option<&str>,
    page: i64,
    page_size: i64,
) -> DbResult<(Vec<User>, i64)> {
    let pool = get_pool();
    let db = pool.inner();

    let offset = (page - 1) * page_size;

    // 过滤已删除用户（status != 'deleted'）
    let mut cond = Condition::all().add(Column::Status.ne("deleted"));

    if let Some(kw) = keyword
        && !kw.is_empty()
    {
        let pattern = format!("%{}%", kw);
        cond = cond.add(
            Condition::any()
                .add(Column::Username.like(&pattern))
                .add(Column::DisplayName.like(&pattern))
                .add(Column::Email.like(&pattern)),
        );
    }

    if let Some(r) = role
        && !r.is_empty()
    {
        cond = cond.add(Column::Role.eq(r));
    }

    if let Some(s) = status
        && !s.is_empty()
    {
        cond = cond.add(Column::Status.eq(s));
    }

    let base_query = Entity::find().filter(cond);
    let total = base_query.clone().count(db).await?;
    let items = base_query
        .order_by_desc(Column::CreatedAt)
        .limit(page_size as u64)
        .offset(offset as u64)
        .all(db)
        .await?;

    Ok((items, total as i64))
}

/// 根据 ID 查找用户
pub async fn find_user_by_id(id: i32) -> DbResult<Option<User>> {
    let pool = get_pool();
    let db = pool.inner();

    let user = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?;

    Ok(user)
}

/// 创建用户
pub async fn create_user(input: NewUser) -> DbResult<i32> {
    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let active_model = ActiveModel {
        username: Set(input.username),
        password: Set(input.password),
        display_name: Set(input.display_name),
        email: Set(input.email),
        role: Set(input.role),
        status: Set("active".to_string()),
        remark: Set(input.remark),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    Ok(result.last_insert_id)
}

/// 编辑用户基本信息
pub async fn update_user(id: i32, input: UpdateUser) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?
        .ok_or(DbError::not_found("用户"))?;

    let mut active_model: ActiveModel = model.into();
    if let Some(display_name) = input.display_name {
        active_model.display_name = Set(Some(display_name));
    }
    if let Some(email) = input.email {
        active_model.email = Set(Some(email));
    }
    if let Some(role) = input.role {
        active_model.role = Set(role);
    }
    if let Some(remark) = input.remark {
        active_model.remark = Set(remark);
    }
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 更新用户状态（active / inactive）
pub async fn update_user_status(id: i32, status: &str) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?
        .ok_or(DbError::not_found("用户"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set(status.to_string());
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 删除用户（软删除，status 置为 deleted）
pub async fn delete_user(id: i32) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?
        .ok_or(DbError::not_found("用户"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.status = Set("deleted".to_string());
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 重置用户密码
pub async fn reset_user_password(id: i32, new_password: String) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?
        .ok_or(DbError::not_found("用户"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.password = Set(new_password);
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 修改用户密码（由业务层验证旧密码后调用）
pub async fn change_user_password(id: i32, new_password: String) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find_by_id(id)
        .filter(Column::Status.ne("deleted"))
        .one(db)
        .await?
        .ok_or(DbError::not_found("用户"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.password = Set(new_password);
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    Ok(())
}

/// 根据用户名查找用户（用于登录）
pub async fn find_user_by_username(username: &str) -> DbResult<Option<User>> {
    let pool = get_pool();
    let db = pool.inner();

    let user = Entity::find()
        .filter(Column::Username.eq(username))
        .filter(Column::Status.eq("active"))
        .one(db)
        .await?;

    Ok(user)
}
