// 知识库配置数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{QueryOrder, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use wp_station_migrations::entity::knowledge_config::{ActiveModel, Column, Entity, Model};

pub type KnowledgeConfig = Model;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewKnowledgeConfig {
    pub file_name: String,
    pub config_content: Option<String>,
    pub create_sql: Option<String>,
    pub insert_sql: Option<String>,
    pub data_content: Option<String>,
}

// ============ 数据库操作函数 ============

/// 查找所有知识库配置（全局共享，无连接隔离）
pub async fn find_all_knowledge_configs() -> DbResult<Vec<KnowledgeConfig>> {
    debug!("查询所有知识库配置");

    let pool = get_pool();
    let db = pool.inner();

    let configs = Entity::find()
        .order_by_desc(Column::CreatedAt)
        .all(db)
        .await?;

    debug!("查询到 {} 个知识库配置", configs.len());
    Ok(configs)
}

/// 根据文件名查找知识库配置
pub async fn find_knowledge_config_by_file_name(
    file_name: &str,
) -> DbResult<Option<KnowledgeConfig>> {
    let pool = get_pool();
    let db = pool.inner();

    let config = Entity::find()
        .filter(Column::FileName.eq(file_name))
        .one(db)
        .await?;

    Ok(config)
}

/// 创建知识库配置
pub async fn create_knowledge_config(config: NewKnowledgeConfig) -> DbResult<i32> {
    info!("创建知识库配置: file_name={}", config.file_name);

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let active_model = ActiveModel {
        file_name: Set(config.file_name),
        config_content: Set(config.config_content),
        create_sql: Set(config.create_sql),
        insert_sql: Set(config.insert_sql),
        data_content: Set(config.data_content),
        is_active: Set(true),
        updated_at: Set(now),
        created_at: Set(now),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;

    info!("知识库配置创建成功: id={}", id);
    Ok(id)
}

/// 更新知识库配置
pub async fn update_knowledge_config(file_name: &str, config: NewKnowledgeConfig) -> DbResult<()> {
    info!("更新知识库配置: file_name={}", file_name);

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::FileName.eq(file_name))
        .one(db)
        .await?
        .ok_or(DbError::not_found("知识库配置"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.config_content = Set(config.config_content);
    active_model.create_sql = Set(config.create_sql);
    active_model.insert_sql = Set(config.insert_sql);
    active_model.data_content = Set(config.data_content);
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    info!("知识库配置更新成功");
    Ok(())
}

/// 更新知识库配置激活状态
pub async fn update_knowledge_config_active(file_name: &str, is_active: bool) -> DbResult<()> {
    info!(
        "更新知识库配置激活状态: file_name={}, is_active={}",
        file_name, is_active
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::FileName.eq(file_name))
        .one(db)
        .await?
        .ok_or(DbError::not_found("知识库配置"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.is_active = Set(is_active);
    active_model.updated_at = Set(Utc::now());
    active_model.update(db).await?;

    info!("知识库配置激活状态更新成功");
    Ok(())
}

/// 获取知识库配置状态列表（全局共享）
pub async fn get_knowledge_config_status_list() -> DbResult<Vec<(String, bool)>> {
    let pool = get_pool();
    let db = pool.inner();

    // 只返回处于激活状态的配置，保持与 wpl/oml 一致的行为
    let configs = Entity::find()
        .filter(Column::IsActive.eq(true))
        .order_by_asc(Column::FileName)
        .all(db)
        .await?;

    let status_list: Vec<(String, bool)> = configs
        .into_iter()
        .map(|c| (c.file_name, c.is_active))
        .collect();

    Ok(status_list)
}

/// 删除所有知识库配置，返回删除条数
pub async fn delete_all_knowledge_configs() -> DbResult<u64> {
    info!("准备清空 knowledge_configs 表");

    let pool = get_pool();
    let db = pool.inner();

    let result = Entity::delete_many().exec(db).await?;
    info!("knowledge_configs 已清空: rows={}", result.rows_affected);
    Ok(result.rows_affected)
}

/// 获取一份用于 knowdb.toml 的配置（任意一条记录）
pub async fn get_knowdb_config_entry() -> DbResult<Option<KnowledgeConfig>> {
    let pool = get_pool();
    let db = pool.inner();

    let entry = Entity::find()
        .filter(Column::ConfigContent.is_not_null())
        .order_by_desc(Column::UpdatedAt)
        .one(db)
        .await?;

    Ok(entry)
}

/// 将同一份 knowdb 配置更新到所有知识库记录
pub async fn update_knowdb_config(content: Option<String>) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let configs = Entity::find().all(db).await?;
    if configs.is_empty() {
        // 没有数据集时直接返回，避免生成孤立目录
        info!("没有知识库数据集，跳过 knowdb 配置更新");
        return Ok(());
    }

    for config in configs {
        let mut active_model: ActiveModel = config.into();
        active_model.config_content = Set(content.clone());
        active_model.updated_at = Set(Utc::now());
        active_model.update(db).await?;
    }

    Ok(())
}
