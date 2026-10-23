//! 发布写操作逻辑。

use chrono::Utc;
use tempfile::tempdir;
use tracing::info;

use crate::constants::release::{GROUP_ALL, GROUP_DRAFT, GROUP_INFRA, GROUP_MODELS};
use crate::db::{
    NewReleaseTarget, ReleaseGroup, ReleaseStatus, ReleaseTargetStatus, ReleaseTargetUpdate,
    create_release_targets, find_device_previous_success_version, find_devices_by_ids,
    find_release_by_id, find_release_targets_by_release, update_release_group,
    update_release_pipeline, update_release_status, update_release_target,
};
use crate::error::AppError;
use crate::utils::project_check::{ProjectCheckTarget, validate_project_in_dir};
use crate::utils::{compose_repo_layout_into, layout_for_system};

use super::{
    ReleasePublishResponse, ReleaseTargetActionRequest, ReleaseValidateResponse,
    can_publish_release, default_target_stage_trace, latest_target_per_device_group,
    normalize_note, release_group_title, release_system, rollback_target_stage_trace,
    serialize_stage_summary, serialize_stage_trace, stage_summary_for_release,
    stage_summary_for_status, summarize_published_groups, target_group_publish_succeeded,
};

/// 校验发布版本。
///
/// 将双仓库合成后执行全部项目组件的完整性校验（WPL、OML、Engine、Sources、Sinks、Connectors）。
pub async fn validate_release_logic(id: i32) -> Result<ReleaseValidateResponse, AppError> {
    info!("发布版本校验请求: release_id={}", id);
    let filename = format!("版本 {}", id);

    async {
        let release = find_release_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("发布记录不存在".to_string()))?;
        let system = release_system(&release)?;
        let layout = layout_for_system(system).as_repo_layout();
        let validate_dir = tempdir().map_err(AppError::internal)?;
        compose_repo_layout_into(&layout, validate_dir.path())?;

        let check_result = validate_project_in_dir(
            system,
            validate_dir.path(),
            ProjectCheckTarget::WholeProject,
        );

        match check_result {
            Ok(_) => {
                info!("发布版本校验通过: release_id={}", id);
                Ok::<_, AppError>(ReleaseValidateResponse {
                    filename,
                    lines: 0,
                    warnings: 0,
                    r#type: "发布包".to_string(),
                    valid: true,
                    details: vec!["所有组件校验通过".to_string()],
                })
            }
            Err(err) => {
                warn!("发布版本校验失败: release_id={}, error={}", id, err);
                Ok::<_, AppError>(ReleaseValidateResponse {
                    filename,
                    lines: 0,
                    warnings: 1,
                    r#type: "发布包".to_string(),
                    valid: false,
                    details: vec![err.to_string()],
                })
            }
        }
    }
    .await
}

/// 执行发布动作（多台设备）。
pub async fn publish_release_logic(
    id: i32,
    release_group: ReleaseGroup,
    device_ids: Vec<i32>,
    note: Option<String>,
    full_publish: bool,
) -> Result<ReleasePublishResponse, AppError> {
    if device_ids.is_empty() {
        return Err(AppError::Validation("请至少选择一台目标设备".to_string()));
    }

    let release = find_release_by_id(id)
        .await?
        .ok_or_else(|| AppError::NotFound("发布记录不存在".to_string()))?;
    let release_system = release_system(&release)?;

    if full_publish && release.release_group != GROUP_DRAFT {
        return Err(AppError::Validation("全量发布只允许从草稿开始".to_string()));
    }

    let release_status = super::stage::parse_release_status(&release)?;
    if !can_publish_release(&release, &release_status) {
        return Err(AppError::Validation(format!(
            "当前状态({})不允许继续发布",
            release.status
        )));
    }

    let devices = find_devices_by_ids(&device_ids).await?;
    if devices.len() != device_ids.len() {
        return Err(AppError::Validation("部分设备不存在或已被删除".to_string()));
    }

    let normalized_note = normalize_note(note);
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

    if full_publish {
        // 先为两个仓库准备同一版本的 tag，设备仍严格按 models 成功后再执行 infra。
        crate::server::push_and_tag_release(&release.version, release_system, ReleaseGroup::Models)
            .await?;
        crate::server::push_and_tag_release(&release.version, release_system, ReleaseGroup::Infra)
            .await?;
    } else {
        crate::server::push_and_tag_release(&release.version, release_system, release_group)
            .await?;
    }

    let stage_trace_str = serialize_stage_trace(&default_target_stage_trace());
    let mut new_targets = Vec::with_capacity(devices.len() * if full_publish { 2 } else { 1 });
    for device in &devices {
        new_targets.push(NewReleaseTarget {
            release_id: id,
            device_id: device.id,
            release_group: ReleaseGroup::Models.as_ref().to_string(),
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
            attempt_no: 1,
            operation: "publish".to_string(),
            previous_group_version: None,
            request_summary: None,
            response_status: None,
            response_summary: None,
        });
        if full_publish {
            new_targets.push(NewReleaseTarget {
                release_id: id,
                device_id: device.id,
                release_group: ReleaseGroup::Infra.as_ref().to_string(),
                status: ReleaseTargetStatus::PENDING,
                stage_trace: Some(stage_trace_str.clone()),
                remote_job_id: None,
                rollback_job_id: None,
                current_config_version: device.config_version.clone(),
                target_config_version: release.version.clone(),
                client_version: device.client_version.clone(),
                error_message: None,
                next_poll_at: None,
                poll_attempts: 0,
                attempt_no: 1,
                operation: "publish".to_string(),
                previous_group_version: None,
                request_summary: None,
                response_status: None,
                response_summary: None,
            });
        } else if release_group != ReleaseGroup::Models {
            new_targets.last_mut().unwrap().release_group = release_group.as_ref().to_string();
        }
    }

    create_release_targets(new_targets).await?;

    let mut published_parts = Vec::new();
    if release.release_group == GROUP_MODELS || release.release_group == GROUP_ALL {
        published_parts.push(ReleaseGroup::Models);
    }
    if release.release_group == GROUP_INFRA || release.release_group == GROUP_ALL {
        published_parts.push(ReleaseGroup::Infra);
    }
    if !published_parts.contains(&release_group) {
        published_parts.push(release_group);
    }
    let effective_release_group = if full_publish {
        GROUP_ALL.to_string()
    } else {
        summarize_published_groups(&published_parts)
    };

    let stage_summary = serialize_stage_summary(&stage_summary_for_release(
        &ReleaseStatus::RUNNING,
        &effective_release_group,
    ));
    update_release_group(id, &effective_release_group).await?;
    update_release_pipeline(id, normalized_note.as_deref()).await?;
    update_release_status(id, ReleaseStatus::RUNNING, None, Some(&stage_summary)).await?;

    Ok(ReleasePublishResponse {
        success: true,
        message: format!("已触发发布，共 {} 台设备", device_ids.len()),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: device_ids.len(),
    })
}

/// 根据设备 ID 或目标 ID 筛选本次重试/回滚要处理的发布目标。
fn select_targets<'a>(
    targets: &'a [crate::db::ReleaseTarget],
    req: &ReleaseTargetActionRequest,
) -> Vec<&'a crate::db::ReleaseTarget> {
    if req.device_ids.is_empty() {
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
    }
}

/// 重试失败的发布子任务。
pub async fn retry_release_logic(
    id: i32,
    req: ReleaseTargetActionRequest,
) -> Result<ReleasePublishResponse, AppError> {
    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let selected = select_targets(&targets, &req);

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

    Ok(ReleasePublishResponse {
        success: true,
        message: format!("已重新排队 {} 台设备", affected),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: affected,
    })
}

/// 回滚指定设备到上一个成功版本（如果没有则回滚到 `v1.0.0`）。
pub async fn rollback_release_logic(
    id: i32,
    req: ReleaseTargetActionRequest,
) -> Result<ReleasePublishResponse, AppError> {
    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let selected = select_targets(&targets, &req);

    if selected.is_empty() {
        return Err(AppError::Validation("未找到可回滚的设备".to_string()));
    }

    let mut affected = 0usize;
    for target in selected {
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

    Ok(ReleasePublishResponse {
        success: true,
        message: format!("已触发 {} 台设备回滚到上一个成功版本", affected),
        release_status: ReleaseStatus::RUNNING.as_ref().to_string(),
        enqueued: affected,
    })
}
