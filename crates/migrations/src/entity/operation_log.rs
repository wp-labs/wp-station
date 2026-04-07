use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "operation_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    // 操作人用户名
    pub operator: String,
    // 操作类型: create / update / delete / publish
    pub operation: String,
    // 操作对象描述
    pub target: Option<String>,
    // 操作描述（页面展示）
    pub description: Option<String>,
    // 操作详细内容（不在页面展示，供审计用）
    pub content: Option<String>,
    // 状态: success / error
    pub status: String,
    // 操作时间
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
