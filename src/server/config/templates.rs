//! 配置模板业务逻辑层。
//!
//! 负责从当前系统目录读取 connector 模板，并转换为前端可直接展示、
//! 可渲染的 source / sink 配置模板结构。

use crate::db::RuleType;
use crate::error::AppError;
use crate::server::RepoLayout;
use crate::server::Setting;
use crate::utils::{
    display::connector_type_display_name, display_name_from_file,
    list_config_templates_from_layout, render_config_template, template_id_from_file,
};
use serde::{Deserialize, Serialize};

// ============ 请求参数结构体 ============

/// 配置模板列表查询参数。
#[derive(Deserialize)]
pub struct ConfigTemplateQuery {
    pub scope: RuleType,
}

/// 模板渲染请求体。
#[derive(Deserialize)]
pub struct RenderConfigTemplateRequest {
    pub scope: RuleType,
    pub template_id: String,
    pub content: String,
}

// ============ 响应结构体 ============

/// 单个模板字段响应体。
#[derive(Serialize)]
pub struct ConfigTemplateFieldItem {
    pub name: String,
    pub required: bool,
    pub default_value: Option<String>,
    pub advanced: bool,
}

/// 模板列表项响应体。
#[derive(Serialize)]
pub struct ConfigTemplateItem {
    pub scope: RuleType,
    pub template_file: String,
    pub template_id: String,
    pub display_name: String,
    pub connect: String,
    pub connector_type: String,
    pub connector_type_display_name: Option<String>,
    pub required_fields: Vec<String>,
    pub inserted_fields: Vec<String>,
    pub omitted_fields: Vec<String>,
    pub fields: Vec<ConfigTemplateFieldItem>,
}

/// 配置模板列表响应体。
#[derive(Serialize)]
pub struct ConfigTemplateListResponse {
    pub items: Vec<ConfigTemplateItem>,
}

/// 模板渲染结果响应体。
#[derive(Serialize)]
pub struct RenderConfigTemplateResponse {
    pub scope: RuleType,
    pub template_file: String,
    pub template_id: String,
    pub display_name: String,
    pub connect: String,
    pub connector_type: String,
    pub connector_type_display_name: Option<String>,
    pub instance_name: String,
    pub required_fields: Vec<String>,
    pub inserted_fields: Vec<String>,
    pub omitted_fields: Vec<String>,
    pub warnings: Vec<String>,
    pub snippet: String,
    pub content: String,
}

/// 获取 `wparse` 的固定仓库布局。
fn wparse_layout() -> RepoLayout {
    Setting::load().wparse_layout()
}

/// 获取来源 / 输出配置模板列表。
pub async fn get_config_templates_logic(
    scope: RuleType,
) -> Result<ConfigTemplateListResponse, AppError> {
    let layout = wparse_layout();
    let items = list_config_templates_from_layout(&layout, scope)?
        .into_iter()
        .map(|template| {
            let required_fields = template
                .fields
                .iter()
                .filter(|field| field.required)
                .map(|field| field.name.clone())
                .collect::<Vec<_>>();
            let inserted_fields = template
                .fields
                .iter()
                .filter(|field| !field.advanced && field.default_value.is_some())
                .map(|field| field.name.clone())
                .collect::<Vec<_>>();
            let omitted_fields = template
                .fields
                .iter()
                .filter(|field| field.advanced)
                .map(|field| field.name.clone())
                .collect::<Vec<_>>();
            let fields = template
                .fields
                .into_iter()
                .map(|field| ConfigTemplateFieldItem {
                    name: field.name,
                    required: field.required,
                    default_value: field.default_value,
                    advanced: field.advanced,
                })
                .collect::<Vec<_>>();

            ConfigTemplateItem {
                scope: template.scope,
                template_file: template.template_file.clone(),
                template_id: template_id_from_file(&template.template_file),
                display_name: display_name_from_file(&template.template_file),
                connect: template.connect,
                connector_type_display_name: connector_type_display_name(&template.connector_type)
                    .map(|value| value.to_string()),
                connector_type: template.connector_type,
                required_fields,
                inserted_fields,
                omitted_fields,
                fields,
            }
        })
        .collect();

    Ok(ConfigTemplateListResponse { items })
}

/// 渲染来源 / 输出配置模板片段。
pub async fn render_config_template_logic(
    scope: RuleType,
    template_id: String,
    content: String,
) -> Result<RenderConfigTemplateResponse, AppError> {
    let layout = wparse_layout();
    let rendered = render_config_template(&layout, scope, &template_id, &content)?;

    Ok(RenderConfigTemplateResponse {
        scope: rendered.scope,
        template_file: rendered.template_file,
        template_id: rendered.template_id,
        display_name: rendered.display_name,
        connect: rendered.connect,
        connector_type_display_name: connector_type_display_name(&rendered.connector_type)
            .map(|value| value.to_string()),
        connector_type: rendered.connector_type,
        instance_name: rendered.instance_name,
        required_fields: rendered.required_fields,
        inserted_fields: rendered.inserted_fields,
        omitted_fields: rendered.omitted_fields,
        warnings: rendered.warnings,
        snippet: rendered.snippet,
        content: rendered.content,
    })
}
