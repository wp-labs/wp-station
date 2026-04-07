use sea_orm::{entity::prelude::*, JsonValue};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "sandbox_runs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique)]
    pub task_id: String,
    pub release_id: i32,
    pub status: String,
    pub stages_json: JsonValue,
    pub conclusion_json: Option<JsonValue>,
    pub options_json: JsonValue,
    pub workspace_path: Option<String>,
    pub daemon_ready: Option<bool>,
    pub wpgen_exit_code: Option<i32>,
    pub started_at: Option<DateTimeUtc>,
    pub ended_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
