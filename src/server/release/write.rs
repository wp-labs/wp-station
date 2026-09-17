//! 发布写操作逻辑。

use chrono::Utc;
use tracing::{info, warn};

use crate::constants::release::{GROUP_ALL, GROUP_DRAFT, GROUP_INFRA, GROUP_MODELS};
use crate::db::{
    NewReleaseTarget, ReleaseGroup, ReleaseStatus, ReleaseTargetStatus, ReleaseTargetUpdate,
    create_release_targets, find_device_previous_success_version, find_devices_by_ids,
    find_latest_draft_release, find_latest_sandbox_run, find_release_by_id,
    find_release_targets_by_release, update_release_group, update_release_pipeline,
    update_release_status, update_release_target,
};
use crate::error::AppError;
use crate::server::{Setting, refresh_draft_release_logic, restore_release_to_gitea};
use crate::utils::knowledge::reload_knowledge;
use crate::utils::project_check::{ProjectCheckTarget, validate_project_in_dir};
use crate::utils::{compose_repo_layout_into, layout_for_system};

use super::{
    ReleasePublishResponse, ReleaseRestoreResponse, ReleaseTargetActionRequest,
    ReleaseValidateResponse, can_publish_release, default_target_stage_trace,
    latest_target_per_device_group, normalize_note, release_group_title, release_system,
    rollback_target_stage_trace, sandbox_run_passed, serialize_stage_summary,
    serialize_stage_trace, stage_summary_for_release, stage_summary_for_status,
    summarize_published_groups, target_group_publish_succeeded,
};

/// 还原发布成功版本的配置到对应 Gitea 仓库，并准备一个可继续编辑的草稿。
pub async fn restore_release_logic(
    id: i32,
    requested_system: Option<crate::utils::SystemKind>,
) -> Result<ReleaseRestoreResponse, AppError> {
    async {
        let release = find_release_by_id(id)
            .await?
            .ok_or_else(|| AppError::NotFound("发布记录不存在".to_string()))?;
        let system = release_system(&release)?;

        if let Some(requested_system) = requested_system
            && requested_system != system
        {
            return Err(AppError::validation("发布记录所属系统与当前系统不一致"));
        }

        let status = super::stage::parse_release_status(&release)?;
        if status != ReleaseStatus::PASS {
            return Err(AppError::validation("只有发布成功的记录可以还原"));
        }

        let groups = restore_groups(&release.release_group)?;
        let existing_draft = find_latest_draft_release(system).await?;
        // 先确保草稿存在。这样还原完成后用户可以直接在当前草稿中检查和继续发布。
        let draft = refresh_draft_release_logic(system, Some("还原发布配置")).await?;

        restore_release_to_gitea(system, &release.version, &groups).await?;

        let layout = crate::utils::layout_for_system(system).as_repo_layout();
        if let Err(err) = reload_knowledge(&layout) {
            warn!(
                "还原后知识库重载失败（忽略）: system={}, error={}",
                system.as_ref(),
                err
            );
        }

        Ok::<_, AppError>(ReleaseRestoreResponse {
            success: true,
            message: format!("已将版本 {} 还原到草稿并同步到 Gitea", release.version),
            release_id: release.id,
            draft_id: draft.id,
            draft_created: existing_draft.is_none(),
            source_version: release.version,
            restored_groups: groups
                .iter()
                .map(|group| group.as_ref().to_string())
                .collect(),
        })
    }
    .await
}

/// 将发布记录的聚合范围转换为实际需要还原的仓库分组。
fn restore_groups(release_group: &str) -> Result<Vec<ReleaseGroup>, AppError> {
    match release_group {
        GROUP_MODELS => Ok(vec![ReleaseGroup::Models]),
        GROUP_INFRA => Ok(vec![ReleaseGroup::Infra]),
        GROUP_ALL => Ok(vec![ReleaseGroup::Models, ReleaseGroup::Infra]),
        GROUP_DRAFT => Err(AppError::validation("草稿记录不能执行还原")),
        _ => Err(AppError::validation(format!(
            "不支持还原的发布范围: {}",
            release_group
        ))),
    }
}

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
        let validate_dir = Setting::workspace_root()
            .join("tmp")
            .join("release-validate")
            .join(format!("{}", id));
        if validate_dir.exists() {
            let _ = std::fs::remove_dir_all(&validate_dir);
        }
        std::fs::create_dir_all(&validate_dir).map_err(AppError::internal)?;
        compose_repo_layout_into(&layout, &validate_dir)?;

        let check_result =
            validate_project_in_dir(system, &validate_dir, ProjectCheckTarget::WholeProject);
        let _ = std::fs::remove_dir_all(&validate_dir);

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

/// 读取环境变量，决定是否跳过发布前的沙盒通过校验。
fn should_skip_sandbox_check() -> bool {
    std::env::var("WARP_STATION_SKIP_SANDBOX")
        .map(|val| val == "1" || val.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 执行发布动作（多台设备）。
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
    let release_system = release_system(&release)?;

    let release_status = super::stage::parse_release_status(&release)?;
    if !can_publish_release(&release, &release_status) {
        return Err(AppError::Validation(format!(
            "当前状态({})不允许继续发布",
            release.status
        )));
    }

    if !should_skip_sandbox_check() {
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

    crate::server::push_and_tag_release(&release.version, release_system, release_group).await?;

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
    if release.release_group == GROUP_MODELS || release.release_group == GROUP_ALL {
        published_parts.push(ReleaseGroup::Models);
    }
    if release.release_group == GROUP_INFRA || release.release_group == GROUP_ALL {
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
