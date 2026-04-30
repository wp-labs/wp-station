use serde::{Deserialize, Serialize};
use strum::{AsRefStr, Display, EnumString};

use crate::db::RuleType;
use crate::error::AppError;

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
    pub fn from_rule_type(rule_type: RuleType) -> Self {
        match rule_type {
            RuleType::Wpl | RuleType::Oml | RuleType::Knowledge => Self::Models,
            _ => Self::Infra,
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        value
            .parse::<ReleaseGroup>()
            .map_err(|_| AppError::validation(format!("无效的发布组: {}", value)))
    }
}
