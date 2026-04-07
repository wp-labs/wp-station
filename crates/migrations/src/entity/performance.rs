use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "performance_tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub task_id: String,
    pub status: String,
    pub sample_data: Option<String>,
    pub config_content: Option<String>,
    pub start_time: DateTimeUtc,
    pub end_time: Option<DateTimeUtc>,
    pub total_lines: Option<i64>,
    pub duration: Option<i32>,
    pub avg_qps: Option<i32>,
    pub created_by: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// 性能测试结果
pub mod result {
    use super::*;
    
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "performance_results")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub task_id: i32,
        pub sink_name: String,
        pub lines: Option<i64>,
        pub qps: Option<i32>,
        pub status: Option<String>,
        pub error_message: Option<String>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
