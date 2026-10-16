use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// 还原任务编排记录。
///
/// 发布主表只保存最终版本；来源关联、Git 候选和恢复阶段统一由本表持久化。
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "release_restore_jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub system: String,
    pub source_release_id: i32,
    #[sea_orm(unique)]
    pub target_release_id: i32,
    pub source_version: String,
    pub target_version: String,
    pub status: String,
    pub phase: String,
    pub selected_device_ids: String,
    pub models_previous_head: Option<String>,
    pub infra_previous_head: Option<String>,
    pub models_candidate_commit: Option<String>,
    pub infra_candidate_commit: Option<String>,
    #[sea_orm(default_value = false)]
    pub models_promoted: bool,
    #[sea_orm(default_value = false)]
    pub infra_promoted: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub lock_owner: Option<String>,
    pub lock_expires_at: Option<DateTimeUtc>,
    pub started_at: Option<DateTimeUtc>,
    pub completed_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
