use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "rule_configs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(column_name = "type")]
    pub rule_type: String,
    pub file_name: String,
    pub display_name: Option<String>,
    pub content: Option<String>,
    pub sample_content: Option<String>,
    pub file_size: Option<i32>,
    pub updated_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub version: i32,
    pub is_active: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
