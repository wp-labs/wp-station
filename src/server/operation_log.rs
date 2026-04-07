// 操作日志业务逻辑层

use crate::db::{
    NewOperationLog, create_operation_log, find_logs_page, operation_log::OperationLog,
};
use crate::error::AppError;
use crate::utils::pagination::{PageQuery, PageResponse};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct LogListQuery {
    /// 操作人模糊匹配
    pub operator: Option<String>,
    /// 操作类型精确匹配: create / update / delete / publish
    pub operation: Option<String>,
    /// 开始日期，格式 YYYY-MM-DD
    pub start_date: Option<String>,
    /// 结束日期，格式 YYYY-MM-DD
    pub end_date: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

// ============ 响应结构体 ============

pub type LogListResponse = PageResponse<OperationLog>;

/// 操作日志写入状态
#[derive(Debug, Clone, Copy, Serialize)]
pub enum OperationLogStatus {
    Success,
    Error,
}

impl OperationLogStatus {
    fn as_str(self) -> &'static str {
        match self {
            OperationLogStatus::Success => "success",
            OperationLogStatus::Error => "error",
        }
    }
}

/// 操作日志业务对象
#[derive(Debug, Clone, Copy, Serialize)]
pub enum OperationLogBiz {
    ConfigFile,
    RuleFile,
    KnowledgeConfig,
    Device,
    User,
    Release,
    ReleaseTarget,
    AssistTask,
}

impl OperationLogBiz {
    fn label(self) -> &'static str {
        match self {
            OperationLogBiz::ConfigFile => "配置文件",
            OperationLogBiz::RuleFile => "规则文件",
            OperationLogBiz::KnowledgeConfig => "知识库",
            OperationLogBiz::Device => "设备",
            OperationLogBiz::User => "用户",
            OperationLogBiz::Release => "发布单",
            OperationLogBiz::ReleaseTarget => "发布目标",
            OperationLogBiz::AssistTask => "辅助任务",
        }
    }
}

/// 操作日志动作枚举
#[derive(Debug, Clone, Copy, Serialize)]
pub enum OperationLogAction {
    Create,
    Update,
    Delete,
    Submit,
    Cancel,
    Reply,
    Publish,
    Retry,
    Rollback,
    Validate,
    Login,
    ResetPassword,
    ChangePassword,
}

impl OperationLogAction {
    fn code(self) -> &'static str {
        match self {
            OperationLogAction::Create => "create",
            OperationLogAction::Update => "update",
            OperationLogAction::Delete => "delete",
            OperationLogAction::Submit => "submit",
            OperationLogAction::Cancel => "cancel",
            OperationLogAction::Reply => "reply",
            OperationLogAction::Publish => "publish",
            OperationLogAction::Retry => "retry",
            OperationLogAction::Rollback => "rollback",
            OperationLogAction::Validate => "validate",
            OperationLogAction::Login => "login",
            OperationLogAction::ResetPassword => "reset-password",
            OperationLogAction::ChangePassword => "change-password",
        }
    }

    fn verb(self) -> &'static str {
        match self {
            OperationLogAction::Create => "新建",
            OperationLogAction::Update => "更新",
            OperationLogAction::Delete => "删除",
            OperationLogAction::Submit => "提交",
            OperationLogAction::Cancel => "取消",
            OperationLogAction::Reply => "写回",
            OperationLogAction::Publish => "发布",
            OperationLogAction::Retry => "重试",
            OperationLogAction::Rollback => "回滚",
            OperationLogAction::Validate => "校验",
            OperationLogAction::Login => "登录",
            OperationLogAction::ResetPassword => "重置密码",
            OperationLogAction::ChangePassword => "修改密码",
        }
    }
}

/// 业务层写入操作日志时只需传业务、动作和少量参数
#[derive(Debug, Clone, Default)]
pub struct OperationLogParams {
    pub operator: Option<String>,
    pub target_name: Option<String>,
    pub target_id: Option<String>,
    pub fields: Vec<(String, String)>,
}

impl OperationLogParams {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_operator(mut self, operator: impl Into<String>) -> Self {
        self.operator = Some(operator.into());
        self
    }

    pub fn with_target_name(mut self, target_name: impl Into<String>) -> Self {
        self.target_name = Some(target_name.into());
        self
    }

    pub fn with_target_id(mut self, target_id: impl Into<String>) -> Self {
        self.target_id = Some(target_id.into());
        self
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.fields.push((key.into(), value.into()));
        self
    }
}

fn build_description(biz: OperationLogBiz, action: OperationLogAction) -> String {
    match (biz, action) {
        (OperationLogBiz::ConfigFile, OperationLogAction::Create) => "新建配置文件".to_string(),
        (OperationLogBiz::ConfigFile, OperationLogAction::Update) => "保存配置文件".to_string(),
        (OperationLogBiz::ConfigFile, OperationLogAction::Delete) => "删除配置文件".to_string(),
        (OperationLogBiz::RuleFile, OperationLogAction::Create) => "新建规则文件".to_string(),
        (OperationLogBiz::RuleFile, OperationLogAction::Update) => "保存规则文件".to_string(),
        (OperationLogBiz::RuleFile, OperationLogAction::Delete) => "删除规则文件".to_string(),
        (OperationLogBiz::KnowledgeConfig, OperationLogAction::Create) => "新建知识库".to_string(),
        (OperationLogBiz::KnowledgeConfig, OperationLogAction::Update) => "保存知识库".to_string(),
        (OperationLogBiz::KnowledgeConfig, OperationLogAction::Delete) => "删除知识库".to_string(),
        (OperationLogBiz::AssistTask, OperationLogAction::Submit) => "提交辅助任务".to_string(),
        (OperationLogBiz::AssistTask, OperationLogAction::Cancel) => "取消辅助任务".to_string(),
        (OperationLogBiz::AssistTask, OperationLogAction::Reply) => "写回辅助任务结果".to_string(),
        _ => format!("{}{}", action.verb(), biz.label()),
    }
}

fn build_target(biz: OperationLogBiz, params: &OperationLogParams) -> Option<String> {
    match (&params.target_name, &params.target_id) {
        (Some(name), Some(id)) => Some(format!("{} {} [ID: {}]", biz.label(), name, id)),
        (Some(name), None) => Some(format!("{} {}", biz.label(), name)),
        (None, Some(id)) => Some(format!("{} [ID: {}]", biz.label(), id)),
        (None, None) => Some(biz.label().to_string()),
    }
}

fn build_content(params: &OperationLogParams) -> Option<String> {
    if params.fields.is_empty() {
        return None;
    }

    Some(
        params
            .fields
            .iter()
            .map(|(key, value)| format!("{}={}", key, value))
            .collect::<Vec<_>>()
            .join(", "),
    )
}

// ============ 业务逻辑函数 ============

/// 将 YYYY-MM-DD 字符串解析为当天起始时刻（UTC 00:00:00）
fn parse_start(date_str: &str) -> Result<DateTime<Utc>, AppError> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("无效的开始日期: {}", date_str)))?;
    let datetime = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| AppError::validation(format!("无效的开始日期: {}", date_str)))?;
    Ok(Utc.from_utc_datetime(&datetime))
}

/// 将 YYYY-MM-DD 字符串解析为当天结束时刻（UTC 23:59:59）
fn parse_end(date_str: &str) -> Result<DateTime<Utc>, AppError> {
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| AppError::validation(format!("无效的结束日期: {}", date_str)))?;
    let datetime = date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| AppError::validation(format!("无效的结束日期: {}", date_str)))?;
    Ok(Utc.from_utc_datetime(&datetime))
}

fn to_new_operation_log(
    biz: OperationLogBiz,
    action: OperationLogAction,
    params: OperationLogParams,
    status: OperationLogStatus,
) -> NewOperationLog {
    let target = build_target(biz, &params);
    let description = build_description(biz, action);
    let content = build_content(&params);

    NewOperationLog {
        operator: params.operator.unwrap_or_else(|| "system".to_string()),
        operation: action.code().to_string(),
        target,
        description: Some(description),
        content,
        status: status.as_str().to_string(),
    }
}

/// best-effort 写入操作日志。失败时只记运行日志，不影响主业务返回。
pub async fn write_operation_log(
    biz: OperationLogBiz,
    action: OperationLogAction,
    params: OperationLogParams,
    status: OperationLogStatus,
) {
    let log = to_new_operation_log(biz, action, params, status);
    if let Err(err) = create_operation_log(log).await {
        warn!("写入操作日志失败: error={}", err);
    }
}

/// 根据主业务结果自动推导 success / error，并 best-effort 写入操作日志。
pub async fn write_operation_log_for_result<T>(
    biz: OperationLogBiz,
    action: OperationLogAction,
    params: OperationLogParams,
    result: &Result<T, AppError>,
) {
    let status = if result.is_ok() {
        OperationLogStatus::Success
    } else {
        OperationLogStatus::Error
    };

    write_operation_log(biz, action, params, status).await;
}

/// 获取操作日志分页列表
pub async fn list_logs_logic(query: LogListQuery) -> Result<LogListResponse, AppError> {
    let (page, page_size) = query.page.normalize_default();

    let start_date = match query.start_date.as_deref() {
        Some(date) => Some(parse_start(date)?),
        None => None,
    };
    let end_date = match query.end_date.as_deref() {
        Some(date) => Some(parse_end(date)?),
        None => None,
    };

    if let (Some(start), Some(end)) = (start_date.as_ref(), end_date.as_ref())
        && start > end
    {
        return Err(AppError::validation("开始日期不能晚于结束日期"));
    }

    let (items, total) = find_logs_page(
        query.operator.as_deref(),
        query.operation.as_deref(),
        start_date,
        end_date,
        page,
        page_size,
    )
    .await?;

    Ok(LogListResponse::from_db(items, total, page, page_size))
}
