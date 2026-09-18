//! 统一错误模型与 HTTP 错误响应封装。

use actix_web::{HttpResponse, ResponseError};
use orion_error::{OrionError, StructError, UnifiedReason, reason::ErrorIdentityProvider};
use serde::Serialize;
use std::fmt::Display;
use wpl::{WparseReason, parser::error::WplCodeReason};

/// WPL 相关的结构化错误原因。
#[derive(Debug, Clone, PartialEq, OrionError)]
enum AppWplReason {
    #[orion_error(identity = "biz.wpl_parse_error", message = "WPL 解析失败")]
    WplParse(String),

    #[orion_error(transparent)]
    Uvs(UnifiedReason),
}

impl From<WplCodeReason> for AppWplReason {
    fn from(reason: WplCodeReason) -> Self {
        match reason {
            WplCodeReason::Plugin
            | WplCodeReason::Syntax
            | WplCodeReason::Empty
            | WplCodeReason::UnSupport => AppWplReason::WplParse(String::new()),
            WplCodeReason::Uvs(uvs) => AppWplReason::Uvs(uvs),
        }
    }
}

impl From<WparseReason> for AppWplReason {
    fn from(reason: WparseReason) -> Self {
        match reason {
            WparseReason::Plugin(message) => AppWplReason::WplParse(message),
            WparseReason::NotMatch => AppWplReason::WplParse("规则不匹配".to_string()),
            WparseReason::LineProc(message) => AppWplReason::WplParse(message),
            WparseReason::Uvs(uvs) => AppWplReason::Uvs(uvs),
        }
    }
}

/// 标准错误响应外层结构。
#[derive(Debug, Serialize)]
pub struct ErrorBody<T = serde_json::Value> {
    pub success: bool,
    pub error: ErrorDetail<T>,
}

/// 标准错误响应详情结构。
#[derive(Debug, Serialize)]
pub struct ErrorDetail<T = serde_json::Value> {
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<T>,
}

/// 应用统一错误枚举。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("资源不存在: {0}")]
    NotFound(String),

    #[error("参数验证失败: {0}")]
    Validation(String),

    #[error("服务器内部错误: {0}")]
    Internal(String),

    // Git/Gitea 相关错误
    #[error("Git 操作失败: {0}")]
    Git(String),

    // WPL 解析相关错误
    #[error("WPL 解析失败: {0}")]
    WplParse(String),

    #[error("{message}")]
    WplParseStructured { code: &'static str, message: String },

    // OML 转换相关错误
    #[error("OML 转换失败: {0}")]
    OmlTransform(String),

    // 调试相关错误
    #[error("未找到解析结果，请先执行日志解析")]
    NoParseResult,

    // 连接测试相关错误
    #[error("端口 {addr} 不可达: {reason}")]
    PortUnreachable { addr: String, reason: String },

    #[error("Git Token 无效: {reason}")]
    InvalidGitToken { reason: String },

    // 认证相关错误
    #[error("认证失败: {0}")]
    Unauthorized(String),

    #[error("{message}")]
    TooManyRequests { code: &'static str, message: String },

    #[error("{message}")]
    Conflict { code: &'static str, message: String },
}

impl AppError {
    /// 构造内部错误。
    pub fn internal<E: Display>(e: E) -> Self {
        AppError::Internal(e.to_string())
    }

    /// 构造 Git 相关错误。
    pub fn git<E: Display>(e: E) -> Self {
        AppError::Git(e.to_string())
    }

    /// 构造 WPL 解析错误。
    pub fn wpl_parse<E: Display>(e: E) -> Self {
        AppError::WplParse(e.to_string())
    }

    /// 构造带最佳匹配提示的 WPL 解析错误。
    pub fn wpl_best_error(depth: usize, hint: impl Into<String>) -> Self {
        let message = format!("解析深度: {}\n{}", depth, hint.into());
        AppError::WplParseStructured {
            code: "biz.wpl_parse_error",
            message: format!("WPL 解析失败\n  -> Info: {}", message),
        }
    }

    /// 构造 OML 转换错误。
    pub fn oml_transform<E: Display>(e: E) -> Self {
        AppError::OmlTransform(e.to_string())
    }

    /// 创建 NotFound 错误
    pub fn not_found(msg: impl Into<String>) -> Self {
        AppError::NotFound(msg.into())
    }

    /// 创建 Validation 错误
    pub fn validation(msg: impl Into<String>) -> Self {
        AppError::Validation(msg.into())
    }

    /// 创建 PortUnreachable 错误
    pub fn port_unreachable(addr: impl Into<String>, reason: impl Display) -> Self {
        AppError::PortUnreachable {
            addr: addr.into(),
            reason: reason.to_string(),
        }
    }

    /// 创建 InvalidGitToken 错误
    pub fn invalid_git_token(reason: impl Into<String>) -> Self {
        AppError::InvalidGitToken {
            reason: reason.into(),
        }
    }

    /// 创建 TooManyRequests 错误
    pub fn too_many_requests(msg: impl Into<String>) -> Self {
        AppError::too_many_requests_with_code("TOO_MANY_REQUESTS", msg)
    }

    /// 创建带业务码的限流错误。
    pub fn too_many_requests_with_code(code: &'static str, msg: impl Into<String>) -> Self {
        AppError::TooManyRequests {
            code,
            message: msg.into(),
        }
    }

    /// 创建 Conflict 错误
    pub fn conflict(msg: impl Into<String>) -> Self {
        AppError::conflict_with_code("CONFLICT", msg)
    }

    /// 创建带业务码的冲突错误。
    pub fn conflict_with_code(code: &'static str, msg: impl Into<String>) -> Self {
        AppError::Conflict {
            code,
            message: msg.into(),
        }
    }

    /// 返回当前错误对应的稳定错误码。
    fn code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Validation(_) => "VALIDATION_ERROR",
            AppError::Internal(_) => "INTERNAL_ERROR",
            AppError::Git(_) => "GIT_ERROR",
            AppError::WplParse(_) => "WPL_PARSE_ERROR",
            AppError::WplParseStructured { code, .. } => code,
            AppError::OmlTransform(_) => "OML_TRANSFORM_ERROR",
            AppError::NoParseResult => "NO_PARSE_RESULT",
            AppError::PortUnreachable { .. } => "PORT_UNREACHABLE",
            AppError::InvalidGitToken { .. } => "INVALID_GIT_TOKEN",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::TooManyRequests { code, .. } => code,
            AppError::Conflict { code, .. } => code,
        }
    }
}

impl ResponseError for AppError {
    /// 将统一错误转换为 HTTP 响应。
    fn error_response(&self) -> HttpResponse {
        use actix_web::http::StatusCode;

        let status = match self {
            // 400 Bad Request - 客户端输入错误
            AppError::Validation(_)
            | AppError::WplParse(_)
            | AppError::WplParseStructured { .. }
            | AppError::OmlTransform(_)
            | AppError::NoParseResult
            | AppError::PortUnreachable { .. }
            | AppError::InvalidGitToken { .. } => StatusCode::BAD_REQUEST,

            // 401 Unauthorized - 认证失败
            AppError::Unauthorized(_) => StatusCode::UNAUTHORIZED,

            // 404 Not Found - 资源不存在
            AppError::NotFound(_) => StatusCode::NOT_FOUND,

            // 409 Conflict
            AppError::Conflict { .. } => StatusCode::CONFLICT,

            // 429 Too Many Requests - 资源忙
            AppError::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,

            // 500 Internal Server Error - 服务器内部错误
            AppError::Internal(_) | AppError::Git(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let details = match self {
            AppError::PortUnreachable { addr, reason } => {
                Some(serde_json::json!({ "addr": addr, "reason": reason }))
            }
            AppError::InvalidGitToken { reason } => Some(serde_json::json!({ "reason": reason })),
            _ => None,
        };

        let body = ErrorBody {
            success: false,
            error: ErrorDetail {
                code: self.code(),
                message: self.to_string(),
                details,
            },
        };

        HttpResponse::build(status).json(body)
    }
}

impl From<StructError<WplCodeReason>> for AppError {
    fn from(error: StructError<WplCodeReason>) -> Self {
        let reason: AppWplReason = error.reason().clone().into();
        let app_error = StructError::builder(reason).source(error).finish();
        AppError::WplParseStructured {
            code: app_error.reason().stable_code(),
            message: app_error.display_chain(),
        }
    }
}

impl From<StructError<WparseReason>> for AppError {
    fn from(error: StructError<WparseReason>) -> Self {
        let reason: AppWplReason = error.reason().clone().into();
        let app_error = StructError::builder(reason).source(error).finish();
        AppError::WplParseStructured {
            code: app_error.reason().stable_code(),
            message: app_error.display_chain(),
        }
    }
}

/// 通用数据库错误，供所有仓储层复用
/// 通用数据库错误，供所有仓储层复用。
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("{entity} 不存在")]
    NotFound { entity: &'static str },

    #[error("数据库错误: {0}")]
    Db(#[from] sea_orm::DbErr),
}

pub type DbResult<T> = std::result::Result<T, DbError>;

impl DbError {
    /// 构造资源不存在的数据库错误。
    pub fn not_found(entity: &'static str) -> Self {
        DbError::NotFound { entity }
    }
}

/// 自动转换 DbError 为 AppError
impl From<DbError> for AppError {
    fn from(e: DbError) -> Self {
        match e {
            DbError::NotFound { entity } => {
                AppError::NotFound(format!("{} 不存在或已删除", entity))
            }
            DbError::Db(db_err) => AppError::internal(db_err),
        }
    }
}
