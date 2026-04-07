// 规则配置数据库操作 - 纯函数式

use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use chrono::Utc;
use sea_orm::{QueryOrder, Set, entity::prelude::*};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};
use wp_proj::project::checker::CheckComponent;
use wp_station_migrations::entity::rule_config::{ActiveModel, Column, Entity, Model};

pub type RuleConfig = Model;

// 规则类型 / 连接配置类型：wpl / oml / knowledge / source / sink / parse / wpgen / source_connect / sink_connect 等
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Display, EnumString, AsRefStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum RuleType {
    All,
    Wpl,
    Oml,
    Knowledge,
    Source,
    Sink,
    Parse,
    Wpgen,
    #[serde(rename = "source_connect")]
    SourceConnect,
    #[serde(rename = "sink_connect")]
    SinkConnect,
}

impl RuleType {
    /// 映射到项目校验组件
    pub fn to_check_component(&self) -> Vec<CheckComponent> {
        use wp_proj::project::checker::CheckComponent;
        match self {
            RuleType::All => vec![
                CheckComponent::Wpl,
                CheckComponent::Oml,
                CheckComponent::Engine,
                CheckComponent::Sources,
                CheckComponent::Sinks,
                CheckComponent::Connectors,
            ],
            RuleType::Wpl => vec![CheckComponent::Wpl],
            RuleType::Oml => vec![CheckComponent::Oml],
            RuleType::Knowledge => vec![CheckComponent::Engine], //todo 缺少知识库校验
            RuleType::Source => vec![CheckComponent::Sources],
            RuleType::Sink => vec![CheckComponent::Sinks],
            RuleType::Parse => vec![CheckComponent::Engine],
            RuleType::Wpgen => vec![CheckComponent::Engine],
            RuleType::SourceConnect | RuleType::SinkConnect => vec![CheckComponent::Connectors],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewRuleConfig {
    pub rule_type: RuleType,
    pub file_name: String,
    pub display_name: Option<String>,
    pub content: Option<String>,
    pub sample_content: Option<String>,
    pub file_size: Option<i32>,
}

// ============ 数据库操作函数 ============

/// 根据类型查找规则配置（全局共享，无连接隔离）
pub async fn find_rules_by_type(rule_type: &str) -> DbResult<Vec<RuleConfig>> {
    let pool = get_pool();
    let db = pool.inner();

    let configs = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::IsActive.eq(true))
        .order_by_asc(Column::FileName)
        .all(db)
        .await?;

    Ok(configs)
}

/// 根据类型和文件名查找规则配置
pub async fn find_rule_by_type_and_name(
    rule_type: &str,
    file_name: &str,
) -> DbResult<Option<RuleConfig>> {
    let pool = get_pool();
    let db = pool.inner();

    let config = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::FileName.eq(file_name))
        .filter(Column::IsActive.eq(true))
        .one(db)
        .await?;

    Ok(config)
}

/// 获取规则文件名列表
pub async fn get_rule_file_names(rule_type: &str) -> DbResult<Vec<String>> {
    let pool = get_pool();
    let db = pool.inner();

    let configs = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::IsActive.eq(true))
        .order_by_asc(Column::FileName)
        .all(db)
        .await?;

    let file_names: Vec<String> = configs.into_iter().map(|c| c.file_name).collect();

    Ok(file_names)
}

/// 创建规则配置
pub async fn create_rule_config(config: NewRuleConfig) -> DbResult<i32> {
    info!(
        "创建规则配置: rule_type={:?}, file_name={}",
        config.rule_type, config.file_name
    );

    let pool = get_pool();
    let db = pool.inner();

    let now = Utc::now();
    let active_model = ActiveModel {
        rule_type: Set(config.rule_type.as_ref().to_string()),
        file_name: Set(config.file_name),
        display_name: Set(config.display_name),
        content: Set(config.content),
        sample_content: Set(config.sample_content),
        file_size: Set(config.file_size),
        updated_at: Set(now),
        created_at: Set(now),
        is_active: Set(true),
        version: Set(1),
        ..Default::default()
    };

    let result = Entity::insert(active_model).exec(db).await?;
    let id = result.last_insert_id;

    info!("规则配置创建成功: id={}", id);
    Ok(id)
}

/// 更新规则配置内容
pub async fn update_rule_content(
    rule_type: &str,
    file_name: &str,
    content: &str,
    file_size: i32,
) -> DbResult<()> {
    info!(
        "更新规则配置内容: rule_type={}, file_name={}, size={}",
        rule_type, file_name, file_size
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::FileName.eq(file_name))
        .filter(Column::IsActive.eq(true))
        .one(db)
        .await?
        .ok_or(DbError::not_found("规则配置"))?;

    let next_version = model.version + 1;
    let mut active_model: ActiveModel = model.into();
    active_model.content = Set(Some(content.to_string()));
    active_model.file_size = Set(Some(file_size));
    active_model.updated_at = Set(Utc::now());
    active_model.version = Set(next_version);
    active_model.update(db).await?;

    info!("规则配置内容更新成功");
    Ok(())
}

/// 更新 WPL 规则的 sample.dat 内容
pub async fn update_rule_sample_content(
    rule_type: &str,
    file_name: &str,
    sample_content: &str,
) -> DbResult<()> {
    info!(
        "更新规则 sample 内容: rule_type={}, file_name={}, size={}",
        rule_type,
        file_name,
        sample_content.len()
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::FileName.eq(file_name))
        .filter(Column::IsActive.eq(true))
        .one(db)
        .await?
        .ok_or(DbError::not_found("规则配置"))?;

    let next_version = model.version + 1;
    let mut active_model: ActiveModel = model.into();
    active_model.sample_content = Set(Some(sample_content.to_string()));
    active_model.updated_at = Set(Utc::now());
    active_model.version = Set(next_version);
    active_model.update(db).await?;

    Ok(())
}

/// 删除规则配置（软删除）
pub async fn delete_rule_config(rule_type: &str, file_name: &str) -> DbResult<()> {
    info!(
        "删除规则配置: rule_type={}, file_name={}",
        rule_type, file_name
    );

    let pool = get_pool();
    let db = pool.inner();

    let model = Entity::find()
        .filter(Column::RuleType.eq(rule_type))
        .filter(Column::FileName.eq(file_name))
        .one(db)
        .await?
        .ok_or(DbError::not_found("规则配置"))?;

    let mut active_model: ActiveModel = model.into();
    active_model.is_active = Set(false);
    active_model.update(db).await?;

    info!("规则配置删除成功");
    Ok(())
}
/// 检查 rule_configs 表是否为空
pub async fn is_rule_configs_empty() -> DbResult<bool> {
    let pool = get_pool();
    let db = pool.inner();

    let count = Entity::find()
        .filter(Column::IsActive.eq(true))
        .count(db)
        .await?;

    Ok(count == 0)
}
