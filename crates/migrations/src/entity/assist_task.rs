use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// AI 辅助规则编写任务实体
/// 统一存储 AI 分析和人工提单两种类型的任务，结果均通过 reply 接口写回
#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "assist_tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    /// 外部任务 ID，格式 assist-{timestamp}-{random6}，全局唯一
    #[sea_orm(unique)]
    pub task_id: String,
    /// 任务类型：ai / manual
    pub task_type: String,
    /// 目标规则类型：wpl / oml / both
    pub target_rule: String,
    /// 用户提交的日志数据
    pub log_data: String,
    /// 当前已有的规则内容（供 AI 参考，可为空）
    pub current_rule: Option<String>,
    /// 用户补充说明（仅 manual 类型使用，可为空）
    pub extra_note: Option<String>,
    /// 任务状态：pending / processing / success / error / cancelled
    pub status: String,
    /// 建议的 WPL 解析规则（成功后填入）
    pub wpl_suggestion: Option<String>,
    /// 建议的 OML 转化规则（成功后填入）
    pub oml_suggestion: Option<String>,
    /// 分析说明文字（成功后填入）
    pub explanation: Option<String>,
    /// 失败原因（失败时填入）
    pub error_message: Option<String>,
    /// 任务创建时间
    pub created_at: DateTimeUtc,
    /// 最后更新时间
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
