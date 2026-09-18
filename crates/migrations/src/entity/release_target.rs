use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "release_targets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub release_id: i32,
    pub device_id: i32,
    #[sea_orm(default_value = "models")]
    pub release_group: String,
    pub status: String,
    pub stage_trace: Option<String>,
    pub remote_job_id: Option<String>,
    pub rollback_job_id: Option<String>,
    pub current_config_version: Option<String>,
    pub target_config_version: String,
    pub client_version: Option<String>,
    pub error_message: Option<String>,
    pub next_poll_at: Option<DateTimeUtc>,
    #[sea_orm(default_value = 0)]
    pub poll_attempts: i32,
    #[sea_orm(default_value = 1)]
    pub attempt_no: i32,
    #[sea_orm(default_value = "publish")]
    pub operation: String,
    pub previous_group_version: Option<String>,
    pub request_summary: Option<String>,
    pub response_status: Option<String>,
    pub response_summary: Option<String>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
    pub completed_at: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
