// 发布管理业务逻辑层

use crate::db::{
    NewRelease, NewReleaseTarget, Release, ReleaseGroup, ReleaseStatus, ReleaseTarget,
    ReleaseTargetStatus, ReleaseTargetUpdate, RuleType, archive_extra_draft_releases,
    create_release as db_create_release, create_release_targets, find_all_releases,
    find_devices_by_ids, find_latest_draft_release, find_latest_passed_release_by_group,
    find_latest_sandbox_run, find_release_by_id, find_release_targets_by_release,
    touch_release_as_draft, update_release_group, update_release_pipeline, update_release_status,
    update_release_target,
};
use crate::error::AppError;
use crate::server::sandbox::{SandboxRun, TaskStatus};
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::pagination::{PageQuery, PageResponse};
use crate::utils::project_check::check_component_in_dir;
use crate::utils::{compose_project_layout_into, format_beijing_time};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct ReleaseListQuery {
    pub note: Option<String>,
    pub pipeline: Option<String>,
    pub version: Option<String>,
    pub owner: Option<String>,
    pub created_by: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    pub page: PageQuery,
}

#[derive(Deserialize)]
pub struct CreateReleaseRequest {
    pub pipeline: Option<String>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseActionRequest {
    pub release_group: Option<ReleaseGroup>,
    pub rule_type: Option<RuleType>,
    pub device_ids: Option<Vec<i32>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseTargetActionRequest {
    #[serde(default)]
    pub device_ids: Vec<i32>,
    #[serde(default)]
    pub target_ids: Vec<i32>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct StageSnapshot {
    pub label: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct ReleaseItemDto {
    pub id: i32,
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
}

pub type ReleaseListResponse = PageResponse<ReleaseItemDto>;

#[derive(Serialize)]
pub struct ReleaseDeviceDetail {
    pub id: i32,
    pub device_id: i32,
    pub device_name: Option<String>,
    pub ip: String,
    pub port: i32,
    pub status: String,
    pub client_version: Option<String>,
    pub config_version: Option<String>,
    pub target_config_version: String,
    pub stage_trace: Vec<StageSnapshot>,
    pub error_message: Option<String>,
    pub last_seen_at: Option<String>,
}

#[derive(Serialize)]
pub struct ReleaseDetailResponse {
    pub id: i32,
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
    pub diff_groups: Vec<ReleaseDiffGroup>,
}

#[derive(Serialize)]
pub struct CreateReleaseResponse {
    pub id: i32,
    pub success: bool,
}

#[derive(Serialize)]
pub struct ReleaseValidateResponse {
    pub filename: String,
    pub lines: i32,
    pub warnings: i32,
    pub r#type: String,
    pub valid: bool,
    pub details: Vec<String>,
}

#[derive(Serialize)]
pub struct ReleasePublishResponse {
    pub success: bool,
    pub message: String,
    pub release_status: String,
    pub enqueued: usize,
}

#[derive(Serialize)]
pub struct ReleaseDiffResponse {
    pub groups: Vec<ReleaseDiffGroup>,
    pub files: Vec<FileDiffInfo>,
    pub stats: DiffStats,
}

#[derive(Serialize, Clone)]
pub struct ReleaseDiffGroup {
    pub release_group: String,
    pub title: String,
    pub current_version: String,
    pub previous_version: Option<String>,
    pub stats: DiffStats,
    pub files: Vec<FileDiffInfo>,
}

#[derive(Serialize, Clone)]
pub struct FileDiffInfo {
    pub file_path: String,
    pub old_path: Option<String>,
    pub change_type: String,
    pub diff_text: String,
}

#[derive(Serialize, Clone)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

fn sandbox_run_passed(run: &SandboxRun) -> bool {
    run.status == TaskStatus::Success
        && run
            .conclusion
            .as_ref()
            .map(|conclusion| conclusion.passed)
            .unwrap_or(false)
}

fn normalize_note(input: Option<String>) -> Option<String> {
    input
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn should_skip_sandbox_check() -> bool {
    std::env::var("WARP_STATION_SKIP_SANDBOX")
        .map(|val| val == "1" || val.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 获取发布版本列表
pub async fn list_releases_logic(query: ReleaseListQuery) -> Result<ReleaseListResponse, AppError> {
    let (page, page_size) = query.page.normalize_default();
    let pipeline_param = query.pipeline.as_deref().or(query.note.as_deref());
    let created_by_param = query.created_by.as_deref().or(query.owner.as_deref());

    let (releases, total) = find_all_releases(
        page,
        page_size,
        pipeline_param,
        query.version.as_deref(),
        created_by_param,
        query.status.as_deref(),
    )
    .await?;

    let mut items: Vec<ReleaseItemDto> = Vec::new();
    for rel in releases {
        let latest_run = find_latest_sandbox_run(rel.id)
            .await
            .map_err(AppError::from)?;
        let sandbox_ready = latest_run.as_ref().map(sandbox_run_passed).unwrap_or(false);

        items.push(ReleaseItemDto {
            id: rel.id,
            version: rel.version.clone(),
            release_group: rel.release_group.clone(),
            status: rel.status.clone(),
            pipeline: rel.pipeline.clone(),
            owner: rel.created_by.clone(),
            created_at: format_beijing_time(rel.created_at),
            updated_at: format_beijing_time(rel.updated_at),
            published_at: rel.published_at.map(format_beijing_time),
            stages: build_release_summary_stages(
                sandbox_ready,
                &parse_release_status(&rel)?,
                &rel.release_group,
            ),
            sandbox_ready,
        });
    }

    Ok(ReleaseListResponse::from_db(items, total, page, page_size))
}

fn parse_semver(raw: &str) -> Option<(u32, u32, u32)> {
    let trimmed = raw.strip_prefix('v').or_else(|| raw.strip_prefix('V'))?;
    let mut parts = trimmed.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn draft_release_group() -> String {
    "draft".to_string()
}

async fn next_draft_release_version() -> Result<String, AppError> {
    if let Some(existing) = find_latest_draft_release().await? {
        return Ok(existing.version);
    }

    let (releases, _) = find_all_releases(1, 1000, None, None, None, None).await?;
    if let Some((major, minor, patch)) = releases
        .iter()
        .filter(|release| release.release_group != "draft")
        .filter_map(|release| parse_semver(&release.version))
        .max()
    {
        Ok(format!("v{}.{}.{}", major, minor, patch + 1))
    } else {
        Ok("v1.0.1".to_string())
    }
}

fn release_group_title(release_group: &str) -> String {
    match release_group {
        "models" => "规则配置".to_string(),
        "infra" => "设施配置".to_string(),
        "all" => "全量配置".to_string(),
        "draft" => "草稿".to_string(),
        other => other.to_string(),
    }
}

fn aggregated_release_group(parts: &[ReleaseGroup]) -> String {
    let has_models = parts.contains(&ReleaseGroup::Models);
    let has_infra = parts.contains(&ReleaseGroup::Infra);
    match (has_models, has_infra) {
        (true, true) => "all".to_string(),
        (true, false) => ReleaseGroup::Models.as_ref().to_string(),
        (false, true) => ReleaseGroup::Infra.as_ref().to_string(),
        (false, false) => "draft".to_string(),
    }
}

fn release_contains_group(release_group: &str, target_group: ReleaseGroup) -> bool {
    match release_group {
        "all" => true,
        "models" => target_group == ReleaseGroup::Models,
        "infra" => target_group == ReleaseGroup::Infra,
        _ => false,
    }
}

fn all_release_groups() -> Vec<ReleaseGroup> {
    vec![ReleaseGroup::Models, ReleaseGroup::Infra]
}

fn next_semver_version(version: &str) -> Option<String> {
    let (major, minor, patch) = parse_semver(version)?;
    Some(format!("v{}.{}.{}", major, minor, patch + 1))
}

async fn find_latest_non_init_release() -> Result<Option<Release>, AppError> {
    let (releases, _) = find_all_releases(1, 1, None, None, None, None).await?;
    Ok(releases.into_iter().next())
}

fn release_has_any_published_scope(release: &Release) -> bool {
    release.release_group != "draft"
}

fn summarize_published_groups(groups: &[ReleaseGroup]) -> String {
    aggregated_release_group(groups)
}

fn latest_target_per_device_group(targets: Vec<ReleaseTarget>) -> Vec<ReleaseTarget> {
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

fn target_group_publish_succeeded(targets: &[ReleaseTarget], release_group: ReleaseGroup) -> bool {
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

fn can_publish_release(release: &Release, release_status: &ReleaseStatus) -> bool {
    match release_status {
        ReleaseStatus::WAIT => true,
        ReleaseStatus::PASS | ReleaseStatus::FAIL | ReleaseStatus::PARTIAL_FAIL => {
            release_has_any_published_scope(release)
        }
        ReleaseStatus::RUNNING | ReleaseStatus::INIT => false,
    }
}

fn resolve_publish_label(release_group: &str, release_status: &ReleaseStatus) -> String {
    let _ = release_status;
    match release_group {
        "models" => "发布规则".to_string(),
        "infra" => "发布设施".to_string(),
        "all" => "发布".to_string(),
        _ => "发布".to_string(),
    }
}

pub fn stage_summary_for_release(
    release_status: &ReleaseStatus,
    release_group: &str,
) -> Vec<StageSnapshot> {
    let publish_status = match release_status {
        ReleaseStatus::PASS => "pass",
        ReleaseStatus::FAIL => "fail",
        ReleaseStatus::PARTIAL_FAIL => "fail",
        ReleaseStatus::RUNNING => "running",
        _ => "pending",
    };

    vec![
        StageSnapshot {
            label: "沙盒".to_string(),
            status: "pass".to_string(),
            detail: Some("最近一次沙盒验证已通过".to_string()),
        },
        StageSnapshot {
            label: resolve_publish_label(release_group, release_status),
            status: publish_status.to_string(),
            detail: Some(release_group_title(release_group)),
        },
    ]
}

fn build_release_summary_stages(
    sandbox_ready: bool,
    release_status: &ReleaseStatus,
    release_group: &str,
) -> Vec<StageSnapshot> {
    let sandbox_stage = StageSnapshot {
        label: "沙盒".to_string(),
        status: if sandbox_ready { "pass" } else { "pending" }.to_string(),
        detail: None,
    };

    let publish_status = match release_status {
        ReleaseStatus::PASS => "pass",
        ReleaseStatus::FAIL | ReleaseStatus::PARTIAL_FAIL => "fail",
        ReleaseStatus::RUNNING => "running",
        ReleaseStatus::WAIT if sandbox_ready => "running",
        _ => "pending",
    };

    vec![
        sandbox_stage,
        StageSnapshot {
            label: resolve_publish_label(release_group, release_status),
            status: publish_status.to_string(),
            detail: None,
        },
    ]
}

async fn ensure_single_draft_release() -> Result<Release, AppError> {
    let stages = serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::WAIT));
    let draft = if let Some(existing) = find_latest_draft_release().await? {
        let refreshed = touch_release_as_draft(
            existing.id,
            &existing.version,
            &draft_release_group(),
            Some(&stages),
        )
        .await?;
        find_release_by_id(refreshed.id)
            .await?
            .ok_or_else(|| AppError::NotFound("草稿发布记录不存在".to_string()))?
    } else {
        let version = match find_latest_non_init_release().await? {
            Some(latest) if release_has_any_published_scope(&latest) => {
                next_semver_version(&latest.version).unwrap_or_else(|| "v1.0.1".to_string())
            }
            Some(latest) => latest.version,
            None => next_draft_release_version().await?,
        };
        let new_rel = NewRelease {
            version,
            release_group: draft_release_group(),
            pipeline: None,
            created_by: None,
            stages: Some(stages.clone()),
            status: Some(ReleaseStatus::WAIT),
        };
        let draft_id = db_create_release(new_rel).await?;
        find_release_by_id(draft_id)
            .await?
            .ok_or_else(|| AppError::NotFound("草稿发布记录不存在".to_string()))?
    };

    archive_extra_draft_releases(draft.id).await?;

    Ok(draft)
}

/// 在保存配置后刷新唯一草稿记录。
pub async fn refresh_draft_release_logic(_note: Option<&str>) -> Result<Release, AppError> {
    ensure_single_draft_release().await
}

/// 获取单个发布版本的详情
pub async fn get_release_detail_logic(id: i32) -> Result<ReleaseDetailResponse, AppError> {
    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };

    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let device_ids: Vec<i32> = targets.iter().map(|t| t.device_id).collect();
    let devices = find_devices_by_ids(&device_ids).await?;
    let device_map: HashMap<i32, _> = devices.into_iter().map(|d| (d.id, d)).collect();

    let devices_detail = targets
        .into_iter()
        .map(|target| build_device_detail(&target, device_map.get(&target.device_id)))
        .collect::<Result<Vec<_>, _>>()?;

    let latest_run = find_latest_sandbox_run(id).await.map_err(AppError::from)?;
    let sandbox_ready = latest_run.as_ref().map(sandbox_run_passed).unwrap_or(false);
    let latest_sandbox_status = latest_run
        .as_ref()
        .map(|run| run.status.as_str().to_string());
    let latest_sandbox_task_id = latest_run.as_ref().map(|run| run.task_id.clone());

    let release_status = parse_release_status(&release)?;
    let (previous_version, baseline_version, diff_groups) = if release_status == ReleaseStatus::WAIT
    {
        (None, None, collect_draft_diff_groups().await?)
    } else {
        let group_list = all_release_groups();
        let mut groups = Vec::new();
        let mut previous_versions = Vec::new();
        for group in group_list {
            let group_was_published = release_contains_group(&release.release_group, group);
            let previous_release =
                find_latest_passed_release_by_group(group.as_ref(), Some(id)).await?;
            if group_was_published
                && let Some(prev) = previous_release.as_ref().map(|rel| rel.version.clone())
            {
                previous_versions.push(prev);
            }
            groups.push(
                collect_release_diff_for_group(
                    group.as_ref(),
                    if group_was_published {
                        Some(&release.version)
                    } else {
                        None
                    },
                    Some(release.id),
                )
                .await?,
            );
        }
        (
            previous_versions.first().cloned(),
            previous_versions.first().cloned(),
            groups,
        )
    };

    let resp = ReleaseDetailResponse {
        id: release.id,
        version: release.version,
        release_group: release.release_group.clone(),
        status: release.status.clone(),
        pipeline: release.pipeline,
        owner: release.created_by,
        created_at: format_beijing_time(release.created_at),
        updated_at: format_beijing_time(release.updated_at),
        published_at: release.published_at.map(format_beijing_time),
        stages: build_release_summary_stages(
            sandbox_ready,
            &release_status,
            &release.release_group,
        ),
        error_message: release.error_message,
        devices: devices_detail,
        sandbox_ready,
        latest_sandbox_status,
        latest_sandbox_task_id,
        previous_version,
        baseline_version,
        diff_groups,
    };

    Ok(resp)
}

/// 创建或刷新唯一草稿发布记录。
pub async fn create_release_logic(
    pipeline: Option<String>,
    note: Option<String>,
) -> Result<CreateReleaseResponse, AppError> {
    let normalized_note = normalize_note(note);
    let final_pipeline = pipeline.clone().or_else(|| normalized_note.clone());

    let result = async {
        let release = ensure_single_draft_release().await?;
        Ok::<_, AppError>(CreateReleaseResponse {
            id: release.id,
            success: true,
        })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name("draft")
            .with_field("version", "auto")
            .with_field("release_group", "draft")
            .with_field(
                "pipeline",
                final_pipeline.clone().unwrap_or_else(|| "-".to_string()),
            )
            .with_field(
                "note",
                normalized_note.clone().unwrap_or_else(|| "-".to_string()),
            ),
        &result,
    )
    .await;

    result
}

/// 校验发布版本。
///
/// 将双仓库合成后执行全部项目组件的完整性校验（WPL、OML、Engine、Sources、Sinks、Connectors）。
pub async fn validate_release_logic(id: i32) -> Result<ReleaseValidateResponse, AppError> {
    info!("发布版本校验请求: release_id={}", id);
    let filename = format!("版本 {}", id);

    let result = async {
        let components = RuleType::All.to_check_component();
        let setting = Setting::load();
        let layout = setting.project_layout();
        let validate_dir = Setting::workspace_root()
            .join("tmp")
            .join("release-validate")
            .join(format!("{}", id));
        if validate_dir.exists() {
            let _ = std::fs::remove_dir_all(&validate_dir);
        }
        std::fs::create_dir_all(&validate_dir).map_err(AppError::internal)?;
        compose_project_layout_into(&layout, &validate_dir)?;

        let check_result = check_component_in_dir(&validate_dir, components);
        let _ = std::fs::remove_dir_all(&validate_dir);

        match check_result {
            Ok(_) => {
                info!("发布版本校验通过: release_id={}", id);
                let resp = ReleaseValidateResponse {
                    filename,
                    lines: 0,
                    warnings: 0,
                    r#type: "发布包".to_string(),
                    valid: true,
                    details: vec!["所有组件校验通过".to_string()],
                };
                Ok::<_, AppError>(resp)
            }
            Err(err) => {
                warn!("发布版本校验失败: release_id={}, error={}", id, err);
                let err_msg = err.to_string();
                let resp = ReleaseValidateResponse {
                    filename,
                    lines: 0,
                    warnings: 1,
                    r#type: "发布包".to_string(),
                    valid: false,
                    details: vec![err_msg],
                };
                Ok::<_, AppError>(resp)
            }
        }
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Validate,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("check", "package"),
        &result,
    )
    .await;

    result
}

/// 执行发布动作（多台设备）
pub async fn publish_release_logic(
    id: i32,
    release_group: ReleaseGroup,
    device_ids: Vec<i32>,
    note: Option<String>,
) -> Result<ReleasePublishResponse, AppError> {
    if device_ids.is_empty() {
        return Err(AppError::Validation("请至少选择一台目标设备".to_string()));
    }

    let release = find_release_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound("发布记录不存在".to_string()))?;

    let release_status = parse_release_status(&release)?;
    if !can_publish_release(&release, &release_status) {
        return Err(AppError::Validation(format!(
            "当前状态({})不允许继续发布",
            release.status
        )));
    }

    let skip_sandbox_check = should_skip_sandbox_check();
    if !skip_sandbox_check {
        let latest_run = find_latest_sandbox_run(id).await.map_err(AppError::from)?;
        let sandbox_ready = latest_run.as_ref().map(sandbox_run_passed).unwrap_or(false);
        if latest_run.is_none() {
            return Err(AppError::Validation(
                "请先执行并通过一次沙盒验证后再发布".to_string(),
            ));
        }
        if !sandbox_ready {
            let status_text = latest_run
                .as_ref()
                .map(|run| run.status.as_str())
                .unwrap_or("unknown");
            return Err(AppError::Validation(format!(
                "最近一次沙盒任务未通过(状态: {})，请修复后重新执行",
                status_text
            )));
        }
    }

    let devices = find_devices_by_ids(&device_ids).await?;
    if devices.len() != device_ids.len() {
        return Err(AppError::Validation("部分设备不存在或已被删除".to_string()));
    }

    let normalized_note = normalize_note(note);
    let note_snapshot = normalized_note.clone().unwrap_or_else(|| "-".to_string());
    info!(
        "触发发布: release_id={}, release_group={}, version={}",
        id,
        release_group.as_ref(),
        release.version
    );

    let latest_targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    if target_group_publish_succeeded(&latest_targets, release_group) {
        return Err(AppError::Validation(format!(
            "当前版本 {} 已发布过{}，请选择另一种发布类型",
            release.version,
            release_group_title(release_group.as_ref())
        )));
    }

    crate::server::push_and_tag_release(&release.version, release_group).await?;

    let stage_trace_str = serialize_stage_trace(&default_target_stage_trace());
    let new_targets: Vec<NewReleaseTarget> = devices
        .iter()
        .map(|device| NewReleaseTarget {
            release_id: id,
            device_id: device.id,
            release_group: release_group.as_ref().to_string(),
            status: ReleaseTargetStatus::QUEUED,
            stage_trace: Some(stage_trace_str.clone()),
            remote_job_id: None,
            rollback_job_id: None,
            current_config_version: device.config_version.clone(),
            target_config_version: release.version.clone(),
            client_version: device.client_version.clone(),
            error_message: None,
            next_poll_at: Some(Utc::now()),
            poll_attempts: 0,
        })
        .collect();

    create_release_targets(new_targets).await?;

    let mut published_parts = Vec::new();
    if release.release_group == "models" || release.release_group == "all" {
        published_parts.push(ReleaseGroup::Models);
    }
    if release.release_group == "infra" || release.release_group == "all" {
        published_parts.push(ReleaseGroup::Infra);
    }
    if !published_parts.contains(&release_group) {
        published_parts.push(release_group);
    }
    let effective_release_group = summarize_published_groups(&published_parts);

    let stage_summary = serialize_stage_summary(&stage_summary_for_release(
        &ReleaseStatus::RUNNING,
        &effective_release_group,
    ));
    update_release_group(id, &effective_release_group).await?;
    update_release_pipeline(id, normalized_note.as_deref()).await?;
    update_release_status(id, ReleaseStatus::RUNNING, None, Some(&stage_summary)).await?;

    let result = Ok(ReleasePublishResponse {
        success: true,
        message: format!("已触发发布，共 {} 台设备", device_ids.len()),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: device_ids.len(),
    });

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Publish,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("release_id", id.to_string())
            .with_field("release_group", release_group.as_ref())
            .with_field("version", &release.version)
            .with_field("device_count", device_ids.len().to_string())
            .with_field("device_ids", format!("{:?}", device_ids))
            .with_field("note", note_snapshot),
        &result,
    )
    .await;

    result
}

/// 重试失败的发布子任务
pub async fn retry_release_logic(
    id: i32,
    req: ReleaseTargetActionRequest,
) -> Result<ReleasePublishResponse, AppError> {
    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let selected: Vec<_> = if req.device_ids.is_empty() {
        if req.target_ids.is_empty() {
            targets.iter().collect()
        } else {
            targets
                .iter()
                .filter(|t| req.target_ids.contains(&t.id))
                .collect()
        }
    } else {
        targets
            .iter()
            .filter(|t| req.device_ids.contains(&t.device_id))
            .collect()
    };

    if selected.is_empty() {
        return Err(AppError::Validation("未找到可重试的设备".to_string()));
    }

    let mut affected = 0usize;
    for target in selected {
        let update = ReleaseTargetUpdate {
            status: Some(ReleaseTargetStatus::QUEUED),
            stage_trace: Some(Some(serialize_stage_trace(&default_target_stage_trace()))),
            remote_job_id: Some(None),
            rollback_job_id: Some(None),
            error_message: Some(None),
            next_poll_at: Some(Some(Utc::now())),
            poll_attempts: Some(0),
            completed_at: Some(None),
            ..Default::default()
        };
        update_release_target(target.id, update).await?;
        affected += 1;
    }

    let stage_summary = serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::RUNNING));
    update_release_status(id, ReleaseStatus::RUNNING, None, Some(&stage_summary)).await?;

    let result = Ok(ReleasePublishResponse {
        success: true,
        message: format!("已重新排队 {} 台设备", affected),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: affected,
    });

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Retry,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("device_count", affected.to_string())
            .with_field("selected_device_ids", format!("{:?}", req.device_ids))
            .with_field("selected_target_ids", format!("{:?}", req.target_ids)),
        &result,
    )
    .await;

    result
}

/// 回滚指定设备到上一个成功版本（如果没有则回滚到 v1.0.0）
pub async fn rollback_release_logic(
    id: i32,
    req: ReleaseTargetActionRequest,
) -> Result<ReleasePublishResponse, AppError> {
    use crate::db::find_device_previous_success_version;

    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let selected: Vec<_> = if req.device_ids.is_empty() {
        if req.target_ids.is_empty() {
            targets.iter().collect()
        } else {
            targets
                .iter()
                .filter(|t| req.target_ids.contains(&t.id))
                .collect()
        }
    } else {
        targets
            .iter()
            .filter(|t| req.device_ids.contains(&t.device_id))
            .collect()
    };

    if selected.is_empty() {
        return Err(AppError::Validation("未找到可回滚的设备".to_string()));
    }

    let mut affected = 0usize;
    for target in selected {
        // 查找该设备上一个成功的版本
        let rollback_version =
            find_device_previous_success_version(target.device_id, &target.release_group)
                .await?
                .unwrap_or_else(|| "v1.0.0".to_string());

        info!(
            "设备回滚: device_id={}, 当前版本={}, 目标版本={}",
            target.device_id, target.target_config_version, rollback_version
        );

        let update = ReleaseTargetUpdate {
            status: Some(ReleaseTargetStatus::ROLLBACK_PENDING),
            stage_trace: Some(Some(serialize_stage_trace(&rollback_target_stage_trace()))),
            remote_job_id: Some(None),
            rollback_job_id: Some(None),
            target_config_version: Some(rollback_version),
            error_message: Some(None),
            next_poll_at: Some(Some(Utc::now())),
            poll_attempts: Some(0),
            completed_at: Some(None),
            ..Default::default()
        };
        update_release_target(target.id, update).await?;
        affected += 1;
    }

    let stage_summary = serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::RUNNING));
    update_release_status(id, ReleaseStatus::RUNNING, None, Some(&stage_summary)).await?;

    let result = Ok(ReleasePublishResponse {
        success: true,
        message: format!("已触发 {} 台设备回滚到上一个成功版本", affected),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: affected,
    });

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Rollback,
        OperationLogParams::new()
            .with_target_id(id.to_string())
            .with_field("device_count", affected.to_string())
            .with_field("selected_device_ids", format!("{:?}", req.device_ids))
            .with_field("selected_target_ids", format!("{:?}", req.target_ids)),
        &result,
    )
    .await;

    result
}

/// 获取版本差异（与上一个版本的 git diff）
pub async fn get_release_diff_logic(id: i32) -> Result<ReleaseDiffResponse, AppError> {
    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };

    let release_status = parse_release_status(&release)?;
    let diff_groups = if release_status == ReleaseStatus::WAIT {
        collect_draft_diff_groups().await?
    } else {
        let mut groups = Vec::new();
        for group in all_release_groups() {
            groups.push(
                collect_release_diff_for_group(
                    group.as_ref(),
                    if release_contains_group(&release.release_group, group) {
                        Some(&release.version)
                    } else {
                        None
                    },
                    Some(release.id),
                )
                .await?,
            );
        }
        groups
    };

    let merged_files = diff_groups
        .iter()
        .flat_map(|group| group.files.clone())
        .collect::<Vec<_>>();
    let merged_stats = diff_groups.iter().fold(
        DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
        |mut acc, item| {
            acc.files_changed += item.stats.files_changed;
            acc.insertions += item.stats.insertions;
            acc.deletions += item.stats.deletions;
            acc
        },
    );

    Ok(ReleaseDiffResponse {
        groups: diff_groups,
        files: merged_files,
        stats: merged_stats,
    })
}

async fn collect_draft_diff_groups() -> Result<Vec<ReleaseDiffGroup>, AppError> {
    let models_diff =
        collect_release_diff_for_group(ReleaseGroup::Models.as_ref(), None, None).await?;
    let infra_diff =
        collect_release_diff_for_group(ReleaseGroup::Infra.as_ref(), None, None).await?;
    Ok(vec![models_diff, infra_diff])
}

async fn collect_release_diff_for_group(
    release_group: &str,
    version: Option<&str>,
    exclude_release_id: Option<i32>,
) -> Result<ReleaseDiffGroup, AppError> {
    use gitea::{DiffResultWithFiles, GiteaClient, GiteaConfig};

    let parsed_group = ReleaseGroup::parse(release_group)?;
    let setting = Setting::load();
    let layout = setting.project_layout();

    let gitea_config = GiteaConfig::new(
        setting.gitea.base_url.clone(),
        setting.gitea.username.clone(),
        setting.gitea.password.clone(),
    )
    .with_branch("main".to_string());

    let gitea_client = GiteaClient::new(gitea_config).map_err(AppError::git)?;
    let project_path = match parsed_group {
        ReleaseGroup::Models => layout.models_root,
        ReleaseGroup::Infra => layout.infra_root,
    };

    let previous_release =
        find_latest_passed_release_by_group(release_group, exclude_release_id).await?;
    let previous_version = previous_release.as_ref().map(|rel| rel.version.clone());

    let empty_diff = || DiffResultWithFiles {
        files: vec![],
        stats: gitea::DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
    };

    let diff_result = if let Some(curr_version) = version {
        match gitea_client.diff_with_previous_version(&project_path, curr_version) {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    "获取发布版本差异失败: version={}, release_group={}, error={}",
                    curr_version, release_group, e
                );
                empty_diff()
            }
        }
    } else {
        match gitea_client.diff_with_newest_tag(&project_path) {
            Ok(result) => result,
            Err(e) => {
                warn!(
                    "获取草稿仓库差异失败: release_group={}, error={}",
                    release_group, e
                );
                empty_diff()
            }
        }
    };

    Ok(ReleaseDiffGroup {
        release_group: release_group.to_string(),
        title: match parsed_group {
            ReleaseGroup::Models => "规则配置".to_string(),
            ReleaseGroup::Infra => "设施配置".to_string(),
        },
        current_version: version.unwrap_or("draft").to_string(),
        previous_version,
        stats: DiffStats {
            files_changed: diff_result.stats.files_changed,
            insertions: diff_result.stats.insertions,
            deletions: diff_result.stats.deletions,
        },
        files: diff_result
            .files
            .into_iter()
            .map(|f| FileDiffInfo {
                file_path: f.file_path,
                old_path: f.old_path,
                change_type: f.change_type,
                diff_text: f.diff_text,
            })
            .collect(),
    })
}

pub fn serialize_stage_summary(stages: &[StageSnapshot]) -> String {
    serde_json::to_string(stages).unwrap_or_else(|_| "[]".to_string())
}

pub fn serialize_stage_trace(stages: &[StageSnapshot]) -> String {
    serde_json::to_string(stages).unwrap_or_else(|_| "[]".to_string())
}

pub fn parse_stage_trace(raw: Option<&str>) -> Vec<StageSnapshot> {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(default_target_stage_trace)
}

pub fn default_target_stage_trace() -> Vec<StageSnapshot> {
    vec![
        StageSnapshot {
            label: "准备".to_string(),
            status: "pass".to_string(),
            detail: None,
        },
        StageSnapshot {
            label: "调用客户端".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
        StageSnapshot {
            label: "运行状态".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
    ]
}

pub fn rollback_target_stage_trace() -> Vec<StageSnapshot> {
    vec![
        StageSnapshot {
            label: "准备".to_string(),
            status: "pass".to_string(),
            detail: Some("开启回滚流程".to_string()),
        },
        StageSnapshot {
            label: "调用客户端".to_string(),
            status: "pending".to_string(),
            detail: Some("重新推送旧版本配置".to_string()),
        },
        StageSnapshot {
            label: "运行状态".to_string(),
            status: "pending".to_string(),
            detail: None,
        },
    ]
}

pub fn stage_summary_for_status(status: &ReleaseStatus) -> Vec<StageSnapshot> {
    let summary_status = match status {
        ReleaseStatus::PASS => "pass",
        ReleaseStatus::FAIL => "fail",
        ReleaseStatus::PARTIAL_FAIL => "fail",
        ReleaseStatus::RUNNING => "running",
        _ => "pending",
    };
    vec![StageSnapshot {
        label: "发布".to_string(),
        status: summary_status.to_string(),
        detail: None,
    }]
}

fn parse_release_status(release: &Release) -> Result<ReleaseStatus, AppError> {
    release
        .status
        .parse::<ReleaseStatus>()
        .map_err(|_| AppError::Validation("无效的发布状态".to_string()))
}

fn build_device_detail(
    target: &ReleaseTarget,
    device: Option<&crate::db::Device>,
) -> Result<ReleaseDeviceDetail, AppError> {
    let (ip, port, name, client_version, _config_version, last_seen_at) =
        if let Some(device) = device {
            (
                device.ip.clone(),
                device.port,
                device.name.clone(),
                device.client_version.clone(),
                device.config_version.clone(),
                device.last_seen_at.map(format_beijing_time),
            )
        } else {
            (
                "-".to_string(),
                0,
                None,
                None,
                None,
                Some(format_beijing_time(target.updated_at)),
            )
        };

    Ok(ReleaseDeviceDetail {
        id: target.id,
        device_id: target.device_id,
        device_name: name,
        ip,
        port,
        status: target.status.clone(),
        client_version,
        config_version: target.current_config_version.clone(),
        target_config_version: target.target_config_version.clone(),
        stage_trace: parse_stage_trace(target.stage_trace.as_deref()),
        error_message: target.error_message.clone(),
        last_seen_at,
    })
}
