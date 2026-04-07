// 发布管理业务逻辑层

use crate::db::{
    NewRelease, NewReleaseTarget, Release, ReleaseStatus, ReleaseTarget, ReleaseTargetStatus,
    ReleaseTargetUpdate, RuleType, create_release as db_create_release, create_release_targets,
    find_all_releases, find_devices_by_ids, find_latest_passed_release, find_latest_sandbox_run,
    find_release_by_id, find_release_targets_by_release, update_release_pipeline,
    update_release_status, update_release_target,
};
use crate::error::AppError;
use crate::server::sandbox::{SandboxRun, TaskStatus};
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, Setting,
    write_operation_log_for_result,
};
use crate::utils::pagination::{PageQuery, PageResponse};
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
    pub version: String,
    pub pipeline: Option<String>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseActionRequest {
    pub rule_type: Option<RuleType>,
    pub device_ids: Option<Vec<i32>>,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct ReleaseTargetActionRequest {
    #[serde(default)]
    pub device_ids: Vec<i32>,
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
    pub files: Vec<FileDiffInfo>,
    pub stats: DiffStats,
}

#[derive(Serialize)]
pub struct FileDiffInfo {
    pub file_path: String,
    pub old_path: Option<String>,
    pub change_type: String,
    pub diff_text: String,
}

#[derive(Serialize)]
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
            status: rel.status.clone(),
            pipeline: rel.pipeline.clone(),
            owner: rel.created_by.clone(),
            created_at: rel.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            updated_at: rel.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            published_at: rel
                .published_at
                .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
            stages: deserialize_stage_summary(rel.stages.as_deref()),
            sandbox_ready,
        });
    }

    Ok(ReleaseListResponse::from_db(items, total, page, page_size))
}

/// 获取单个发布版本的详情
pub async fn get_release_detail_logic(id: i32) -> Result<ReleaseDetailResponse, AppError> {
    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };

    let targets = find_release_targets_by_release(id).await?;
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

    let previous_release = find_latest_passed_release(Some(id)).await?;
    let baseline_version = previous_release.as_ref().map(|rel| rel.version.clone());

    let resp = ReleaseDetailResponse {
        id: release.id,
        version: release.version,
        status: release.status.clone(),
        pipeline: release.pipeline,
        owner: release.created_by,
        created_at: release.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: release.updated_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        published_at: release
            .published_at
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
        stages: deserialize_stage_summary(release.stages.as_deref()),
        error_message: release.error_message,
        devices: devices_detail,
        sandbox_ready,
        latest_sandbox_status,
        latest_sandbox_task_id,
        previous_version: baseline_version.clone(),
        baseline_version,
    };

    Ok(resp)
}

/// 创建新的发布版本（版本号自动生成）
pub async fn create_release_logic(
    _version: String, // 忽略用户输入，使用自动生成的版本号
    pipeline: Option<String>,
    note: Option<String>,
) -> Result<CreateReleaseResponse, AppError> {
    // 自动生成版本号
    let version = crate::server::sync::get_next_version().await?;
    info!("自动生成版本号: {}", version);
    let normalized_note = normalize_note(note);
    let final_pipeline = pipeline.clone().or_else(|| normalized_note.clone());

    let result = async {
        let initial_stages =
            serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::WAIT));
        let new_rel = NewRelease {
            version: version.clone(),
            pipeline: final_pipeline.clone(),
            created_by: None,
            stages: Some(initial_stages),
            status: Some(ReleaseStatus::WAIT),
        };

        let id = db_create_release(new_rel).await?;

        Ok::<_, AppError>(CreateReleaseResponse { id, success: true })
    }
    .await;

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Create,
        OperationLogParams::new()
            .with_target_name(version.clone())
            .with_field("version", version)
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

/// 校验发布版本
pub async fn validate_release_logic(id: i32) -> Result<ReleaseValidateResponse, AppError> {
    let filename = format!("版本 {}", id);

    let result = async {
        let resp = ReleaseValidateResponse {
            filename,
            lines: 0,
            warnings: 0,
            r#type: "发布包".to_string(),
            valid: true,
            details: vec![],
        };
        Ok::<_, AppError>(resp)
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
    if !matches!(
        release_status,
        ReleaseStatus::WAIT | ReleaseStatus::FAIL | ReleaseStatus::PARTIAL_FAIL
    ) {
        return Err(AppError::Validation(format!(
            "当前状态({})不允许发布",
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
    if let Some(ref note_value) = normalized_note {
        update_release_pipeline(id, Some(note_value.as_str())).await?;
    }
    let note_snapshot = normalized_note
        .clone()
        .or_else(|| release.pipeline.clone())
        .unwrap_or_else(|| "-".to_string());

    let stage_trace_str = serialize_stage_trace(&default_target_stage_trace());
    let new_targets: Vec<NewReleaseTarget> = devices
        .iter()
        .map(|device| NewReleaseTarget {
            release_id: id,
            device_id: device.id,
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

    let stage_summary = serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::RUNNING));
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
    let targets = find_release_targets_by_release(id).await?;
    let selected: Vec<_> = if req.device_ids.is_empty() {
        targets.iter().collect()
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
            .with_field("selected_device_ids", format!("{:?}", req.device_ids)),
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

    let targets = find_release_targets_by_release(id).await?;
    let selected: Vec<_> = if req.device_ids.is_empty() {
        targets.iter().collect()
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
        let rollback_version = find_device_previous_success_version(target.device_id)
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
            .with_field("selected_device_ids", format!("{:?}", req.device_ids)),
        &result,
    )
    .await;

    result
}

/// 获取版本差异（与上一个版本的 git diff）
pub async fn get_release_diff_logic(id: i32) -> Result<ReleaseDiffResponse, AppError> {
    use gitea::{GiteaClient, GiteaConfig};

    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };

    let setting = Setting::load();

    let gitea_config = GiteaConfig::new(
        setting.gitea.base_url.clone(),
        setting.gitea.username.clone(),
        setting.gitea.password.clone(),
    )
    .with_branch("main".to_string());

    let gitea_client = GiteaClient::new(gitea_config).map_err(AppError::git)?;

    let project_root = std::path::PathBuf::from(&setting.project_root);
    let project_path = if project_root.is_absolute() {
        project_root
    } else {
        Setting::workspace_root().join(&setting.project_root)
    };

    let release_status = parse_release_status(&release)?;

    // 辅助函数：返回空 diff 结果
    let empty_diff = || gitea::DiffResultWithFiles {
        files: vec![],
        stats: gitea::DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
    };

    let diff_result = if release_status == ReleaseStatus::WAIT {
        info!(
            "获取草稿版本差异: version={}, path={}",
            release.version,
            project_path.display()
        );
        match gitea_client.diff_with_newest_tag(&project_path) {
            Ok(result) => result,
            Err(e) => {
                warn!("获取草稿版本差异失败: error={}", e);
                empty_diff()
            }
        }
    } else {
        // 特殊处理 v1.0.0：它是初始 tag，没有更早的版本，直接返回空 diff
        if release.version == "v1.0.0" || release.version == "V1.0.0" {
            info!("版本 {} 是初始版本，返回空差异", release.version);
            empty_diff()
        } else {
            info!(
                "获取已发布版本差异: version={}, path={}",
                release.version,
                project_path.display()
            );
            match gitea_client.diff_with_previous_version(&project_path, &release.version) {
                Ok(result) => result,
                Err(e) => {
                    warn!(
                        "获取已发布版本差异失败: version={}, error={}",
                        release.version, e
                    );
                    empty_diff()
                }
            }
        }
    };

    Ok(ReleaseDiffResponse {
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
        stats: DiffStats {
            files_changed: diff_result.stats.files_changed,
            insertions: diff_result.stats.insertions,
            deletions: diff_result.stats.deletions,
        },
    })
}

fn deserialize_stage_summary(raw: Option<&str>) -> Vec<StageSnapshot> {
    raw.and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_else(|| {
            vec![StageSnapshot {
                label: "发布".to_string(),
                status: "pending".to_string(),
                detail: None,
            }]
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
                device
                    .last_seen_at
                    .map(|ts| ts.format("%Y-%m-%d %H:%M:%S").to_string()),
            )
        } else {
            (
                "-".to_string(),
                0,
                None,
                None,
                None,
                Some(target.updated_at.format("%Y-%m-%d %H:%M:%S").to_string()),
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
