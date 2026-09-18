//! 发布管理业务逻辑层。
//!
//! 双系统改造后，发布记录显式绑定 `system`：
//! - 列表和详情按 system 过滤
//! - 校验和 diff 按 system 定位仓库
//! - 真正执行发布时再按 system 分发到不同服务实现

pub mod diff;
mod draft;
mod read;
mod restore;
pub mod restore_runner;
pub mod runner;
pub mod stage;
mod write;

use crate::constants::release::{GROUP_ALL, GROUP_DRAFT, GROUP_INFRA, GROUP_MODELS, group_title};
use crate::db::{
    Release, ReleaseGroup, ReleaseStatus, ReleaseTarget, ReleaseTargetStatus, RuleType,
};
use crate::error::AppError;
use crate::server::sandbox::{SandboxRun, TaskStatus};
use crate::utils::SystemKind;
use crate::utils::pagination::{PageQuery, PageResponse};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub use self::diff::get_release_diff_logic;
pub use self::draft::{create_release_logic, refresh_draft_release_logic};
pub use self::read::{get_release_detail_logic, list_releases_logic};
pub use self::restore::{
    create_restore_job_logic, get_restore_job_logic, restore_attempts_for_release,
};
pub use self::stage::{
    default_target_stage_trace, parse_stage_trace, rollback_target_stage_trace,
    serialize_stage_summary, serialize_stage_trace, stage_summary_for_release,
    stage_summary_for_status,
};
pub use self::write::{
    publish_release_logic, retry_release_logic, rollback_release_logic, validate_release_logic,
};

/// 发布列表查询参数。
#[derive(Deserialize)]
pub struct ReleaseListQuery {
    /// 当前只查询单个系统的发布记录。
    pub system: SystemKind,
    pub note: Option<String>,
    pub pipeline: Option<String>,
    pub version: Option<String>,
    pub owner: Option<String>,
    pub created_by: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

/// 创建发布请求。
#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    /// 新建发布时必须显式指定 system。
    pub system: SystemKind,
    pub pipeline: Option<String>,
    pub note: Option<String>,
}

/// 发布、校验等动作请求。
#[derive(Deserialize)]
pub struct ReleaseActionRequest {
    pub system: Option<SystemKind>,
    pub release_group: Option<ReleaseGroup>,
    pub rule_type: Option<RuleType>,
    pub device_ids: Option<Vec<i32>>,
    pub full_publish: Option<bool>,
    pub note: Option<String>,
}

/// 面向设备目标的发布动作请求。
#[derive(Deserialize)]
pub struct ReleaseTargetActionRequest {
    pub system: Option<SystemKind>,
    #[serde(default)]
    pub device_ids: Vec<i32>,
    #[serde(default)]
    pub target_ids: Vec<i32>,
}

/// 还原发布配置请求。
#[derive(Deserialize)]
pub struct ReleaseRestoreRequest {
    pub system: SystemKind,
    #[serde(default)]
    pub device_ids: Vec<i32>,
    /// 还原范围：models、infra 或 all；不传时兼容旧客户端按全量处理。
    pub release_group: Option<String>,
    pub note: Option<String>,
}

/// 发布阶段快照。
#[derive(Serialize, Deserialize, Clone)]
pub struct StageSnapshot {
    pub label: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 发布列表项。
#[derive(Serialize)]
pub struct ReleaseItemDto {
    pub id: i32,
    pub system: String,
    pub version: String,
    pub release_group: String,
    pub status: String,
    pub pipeline: Option<String>,
    pub owner: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub stages: Vec<StageSnapshot>,
    pub sandbox_ready: bool,
    pub can_restore: bool,
    pub restore_disabled_reason: Option<String>,
    pub restore_groups: Vec<String>,
    pub latest_restore: Option<RestoreAttemptDto>,
}

/// 发布列表分页响应。
pub type ReleaseListResponse = PageResponse<ReleaseItemDto>;

/// 发布详情中的单设备结果。
#[derive(Serialize)]
pub struct ReleaseDeviceDetail {
    pub id: i32,
    pub device_id: i32,
    pub release_group: String,
    pub device_name: Option<String>,
    pub ip: String,
    pub port: i32,
    pub status: String,
    pub operation: String,
    pub attempt_no: i32,
    pub client_version: Option<String>,
    pub config_version: Option<String>,
    pub target_config_version: String,
    pub stage_trace: Vec<StageSnapshot>,
    pub error_message: Option<String>,
    pub request_summary: Option<String>,
    pub response_status: Option<String>,
    pub response_summary: Option<String>,
    pub last_seen_at: Option<String>,
}

/// 发布详情响应。
#[derive(Serialize)]
pub struct ReleaseDetailResponse {
    pub id: i32,
    pub system: String,
    pub version: String,
    pub release_group: String,
    pub status: String,
    pub pipeline: Option<String>,
    pub owner: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub published_at: Option<String>,
    pub stages: Vec<StageSnapshot>,
    pub error_message: Option<String>,
    pub devices: Vec<ReleaseDeviceDetail>,
    pub sandbox_ready: bool,
    pub latest_sandbox_status: Option<String>,
    pub latest_sandbox_task_id: Option<String>,
    pub previous_version: Option<String>,
    pub baseline_version: Option<String>,
    pub restore_attempts: Vec<RestoreAttemptDto>,
    pub restore_info: Option<RestoreAttemptDto>,
}

/// 创建发布响应。
#[derive(Serialize)]
pub struct CreateReleaseResponse {
    pub id: i32,
    pub success: bool,
}

/// 发布校验响应。
#[derive(Serialize)]
pub struct ReleaseValidateResponse {
    pub filename: String,
    pub lines: i32,
    pub warnings: i32,
    pub r#type: String,
    pub valid: bool,
    pub details: Vec<String>,
}

/// 发布执行响应。
#[derive(Serialize)]
pub struct ReleasePublishResponse {
    pub success: bool,
    pub message: String,
    pub release_status: String,
    pub enqueued: usize,
}

/// 还原发布配置响应。
#[derive(Serialize)]
pub struct ReleaseRestoreResponse {
    pub success: bool,
    pub message: String,
    pub job_id: i32,
    pub target_release_id: i32,
    pub source_version: String,
    pub target_version: String,
    pub release_group: String,
    pub status: String,
}

#[derive(Serialize, Clone)]
pub struct RestoreAttemptDto {
    pub job_id: i32,
    pub source_release_id: i32,
    pub source_version: String,
    pub target_release_id: i32,
    pub target_version: String,
    pub release_group: String,
    pub status: String,
    pub phase: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Serialize)]
pub struct RestoreTargetDto {
    pub id: i32,
    pub device_id: i32,
    pub release_group: String,
    pub operation: String,
    pub attempt_no: i32,
    pub status: String,
    pub previous_group_version: Option<String>,
    pub target_config_version: String,
    pub error_message: Option<String>,
    pub request_summary: Option<String>,
    pub response_status: Option<String>,
    pub response_summary: Option<String>,
}

#[derive(Serialize)]
pub struct RestoreJobDetailResponse {
    pub id: i32,
    pub system: String,
    pub source_release_id: i32,
    pub target_release_id: i32,
    pub source_version: String,
    pub target_version: String,
    pub release_group: String,
    pub status: String,
    pub phase: String,
    pub models_promoted: bool,
    pub infra_promoted: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub targets: Vec<RestoreTargetDto>,
}

impl RestoreJobDetailResponse {
    fn from_models(
        job: crate::db::ReleaseRestoreJob,
        targets: Vec<crate::db::ReleaseTarget>,
        release_group: String,
    ) -> Self {
        Self {
            id: job.id,
            system: job.system,
            source_release_id: job.source_release_id,
            target_release_id: job.target_release_id,
            source_version: job.source_version.clone(),
            target_version: job.source_version,
            release_group,
            status: job.status,
            phase: job.phase,
            models_promoted: job.models_promoted,
            infra_promoted: job.infra_promoted,
            error_code: job.error_code,
            error_message: job.error_message,
            created_at: crate::utils::format_beijing_time(job.created_at),
            completed_at: job.completed_at.map(crate::utils::format_beijing_time),
            targets: targets
                .into_iter()
                .map(|target| RestoreTargetDto {
                    id: target.id,
                    device_id: target.device_id,
                    release_group: target.release_group,
                    operation: target.operation,
                    attempt_no: target.attempt_no,
                    status: target.status,
                    previous_group_version: target.previous_group_version,
                    target_config_version: target.target_config_version,
                    error_message: target.error_message,
                    request_summary: target.request_summary,
                    response_status: target.response_status,
                    response_summary: target.response_summary,
                })
                .collect(),
        }
    }
}

/// 发布差异响应。
#[derive(Serialize)]
pub struct ReleaseDiffResponse {
    pub groups: Vec<ReleaseDiffGroupSummary>,
    pub files: Vec<ReleaseDiffFileInfo>,
    pub stats: DiffStats,
    pub total_files: usize,
    pub offset: usize,
    pub limit: usize,
    pub has_more: bool,
}

/// 单个分组的差异摘要。
#[derive(Serialize, Clone)]
pub struct ReleaseDiffGroupSummary {
    pub release_group: String,
    pub title: String,
    pub current_version: String,
    pub previous_version: Option<String>,
    pub stats: DiffStats,
    pub total_files: usize,
}

/// 单个文件的差异详情。
#[derive(Serialize, Clone)]
pub struct ReleaseDiffFileInfo {
    pub release_group: String,
    pub file_path: String,
    pub old_path: Option<String>,
    pub change_type: String,
    pub diff_text: String,
}

/// 差异统计摘要。
#[derive(Serialize, Clone)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

/// 判断最近一次沙盒任务是否通过。
pub(super) fn sandbox_run_passed(run: &SandboxRun) -> bool {
    run.status == TaskStatus::Success
        && run
            .conclusion
            .as_ref()
            .map(|conclusion| conclusion.passed)
            .unwrap_or(false)
}

/// 草稿配置发生变化后，旧的沙盒通过结果不能继续解锁发布。
pub(super) fn sandbox_run_ready_for_release(release: &Release, run: &SandboxRun) -> bool {
    if !sandbox_run_passed(run) {
        return false;
    }

    match stage::parse_release_status(release) {
        Ok(ReleaseStatus::WAIT) => run.created_at >= release.updated_at,
        _ => true,
    }
}

/// 归一化可选备注，去掉空白字符串。
pub(super) fn normalize_note(input: Option<String>) -> Option<String> {
    input
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 从发布记录字符串字段解析系统类型。
pub(super) fn release_system(release: &Release) -> Result<SystemKind, AppError> {
    release
        .system
        .parse::<SystemKind>()
        .map_err(|_| AppError::validation(format!("无效的发布系统类型: {}", release.system)))
}

pub(super) fn release_group_title(release_group: &str) -> String {
    group_title(release_group).to_string()
}

pub(super) fn aggregated_release_group(parts: &[ReleaseGroup]) -> String {
    let has_models = parts.contains(&ReleaseGroup::Models);
    let has_infra = parts.contains(&ReleaseGroup::Infra);
    match (has_models, has_infra) {
        (true, true) => GROUP_ALL.to_string(),
        (true, false) => ReleaseGroup::Models.as_ref().to_string(),
        (false, true) => ReleaseGroup::Infra.as_ref().to_string(),
        (false, false) => GROUP_DRAFT.to_string(),
    }
}

pub(super) fn release_contains_group(release_group: &str, target_group: ReleaseGroup) -> bool {
    match release_group {
        GROUP_ALL => true,
        GROUP_MODELS => target_group == ReleaseGroup::Models,
        GROUP_INFRA => target_group == ReleaseGroup::Infra,
        _ => false,
    }
}

pub(super) fn all_release_groups() -> Vec<ReleaseGroup> {
    vec![ReleaseGroup::Models, ReleaseGroup::Infra]
}

pub(super) fn release_has_any_published_scope(release: &Release) -> bool {
    release.release_group != GROUP_DRAFT
}

pub(super) fn summarize_published_groups(groups: &[ReleaseGroup]) -> String {
    aggregated_release_group(groups)
}

pub(super) fn latest_target_per_device_group(targets: Vec<ReleaseTarget>) -> Vec<ReleaseTarget> {
    let mut latest: HashMap<(i32, String), ReleaseTarget> = HashMap::new();
    for target in targets {
        let key = (target.device_id, target.release_group.clone());
        match latest.get(&key) {
            Some(existing) if existing.created_at >= target.created_at => {}
            _ => {
                latest.insert(key, target);
            }
        }
    }

    let mut values = latest.into_values().collect::<Vec<_>>();
    values.sort_by_key(|target| target.id);
    values
}

pub(super) fn target_group_publish_succeeded(
    targets: &[ReleaseTarget],
    release_group: ReleaseGroup,
) -> bool {
    let group_name = release_group.as_ref();
    let group_targets = targets
        .iter()
        .filter(|target| target.release_group == group_name)
        .collect::<Vec<_>>();

    !group_targets.is_empty()
        && group_targets.iter().all(|target| {
            matches!(
                target.status.parse::<ReleaseTargetStatus>(),
                Ok(ReleaseTargetStatus::SUCCESS | ReleaseTargetStatus::ROLLED_BACK)
            )
        })
}

pub(super) fn can_publish_release(release: &Release, release_status: &ReleaseStatus) -> bool {
    match release_status {
        ReleaseStatus::WAIT => true,
        ReleaseStatus::PASS | ReleaseStatus::FAIL | ReleaseStatus::PARTIAL_FAIL => {
            release_has_any_published_scope(release)
        }
        ReleaseStatus::RUNNING | ReleaseStatus::INIT => false,
    }
}
