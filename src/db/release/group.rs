//! 发布分组定义与转换工具。

use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

use crate::db::RuleType;
use crate::error::AppError;

/// 发布使用的固定分组类型。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, AsRefStr, Hash,
)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ReleaseGroup {
    Models,
    Infra,
}

impl ReleaseGroup {
    /// 根据规则类型映射到发布分组。
    pub fn from_rule_type(rule_type: RuleType) -> Self {
        match rule_type {
            RuleType::Wpl
            | RuleType::Oml
            | RuleType::Windows
            | RuleType::Schema
            | RuleType::Rule
            | RuleType::Scenarios
            | RuleType::Knowledge => Self::Models,
            _ => Self::Infra,
        }
    }

    /// 从字符串解析发布分组。
    pub fn parse(value: &str) -> Result<Self, AppError> {
        value
            .parse::<ReleaseGroup>()
            .map_err(|_| AppError::validation(format!("无效的发布组: {}", value)))
    }
}
