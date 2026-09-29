//! 规则校验相关业务。

use crate::constants::project::FILE_WPL_PARSE;
use crate::db::RuleType;
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::project_check::{ProjectCheckTarget, validate_project_in_dir};
use crate::utils::{SystemKind, compose_repo_layout_into, write_rule_content_in_project_dir};

use super::{ValidateRuleResponse, repo_layout};

fn ensure_rule_type_supported(system: SystemKind, rule_type: RuleType) -> Result<(), AppError> {
    if matches!(system, SystemKind::Wfusion) && matches!(rule_type, RuleType::Knowledge) {
        return Err(AppError::validation("wfusion 不支持 knowledge 配置"));
    }

    if matches!(system, SystemKind::Wparse)
        && matches!(
            rule_type,
            RuleType::Windows | RuleType::Schema | RuleType::Rule | RuleType::Scenarios
        )
    {
        return Err(AppError::validation("wparse 不支持该规则类型"));
    }

    Ok(())
}

/// 校验规则配置。
pub async fn validate_rule_logic(
    system: SystemKind,
    rule_type: RuleType,
    file: String,
    content: Option<String>,
) -> Result<ValidateRuleResponse, AppError> {
    info!("规则配置校验请求: rule_type={:?}, file={}", rule_type, file);
    ensure_rule_type_supported(system, rule_type)?;

    match validate_rule_with_current_content(system, rule_type, &file, content.as_deref()) {
        Ok(_) => {
            info!("规则配置校验通过: rule_type={:?}", rule_type);
            Ok(ValidateRuleResponse {
                valid: true,
                message: None,
                details: vec![],
            })
        }
        Err(e) => {
            warn!("规则配置校验失败: rule_type={:?}, error={}", rule_type, e);
            let err_msg = e.to_string();
            Ok(ValidateRuleResponse {
                valid: false,
                message: Some(err_msg.clone()),
                details: vec![err_msg],
            })
        }
    }
}

/// 用当前项目内容加待校验内容构造临时目录，再走现有校验器。
fn validate_rule_with_current_content(
    system: SystemKind,
    rule_type: RuleType,
    file: &str,
    content: Option<&str>,
) -> Result<(), AppError> {
    if matches!(rule_type, RuleType::Wpl)
        && file.trim().ends_with(FILE_WPL_PARSE)
        && content.is_some_and(|value| value.trim().is_empty())
    {
        return Err(AppError::validation(
            "WPL 规则内容为空，请先填写 parse.wpl 后再校验",
        ));
    }

    let layout = repo_layout(system);
    let tmp_dir = Setting::workspace_root()
        .join("tmp")
        .join("project-check")
        .join(format!("{}", chrono::Utc::now().timestamp_millis()));
    std::fs::create_dir_all(&tmp_dir).map_err(AppError::internal)?;

    let result = (|| {
        compose_repo_layout_into(&layout, &tmp_dir)?;
        if let Some(current_content) = content {
            write_rule_content_in_project_dir(&tmp_dir, rule_type, file, current_content)?;
        }
        validate_project_in_dir(system, &tmp_dir, ProjectCheckTarget::RuleType(rule_type))
    })();

    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}
