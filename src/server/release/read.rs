//! 发布查询逻辑。

use std::collections::HashMap;

use crate::db::{
    ReleaseStatus, find_active_restore_job, find_all_releases, find_devices_by_ids,
    find_latest_draft_release, find_latest_passed_release_by_group, find_latest_sandbox_run,
    find_release_by_id, find_release_targets_by_release, find_releases_by_system,
    find_restore_job_by_target_release,
};
use crate::error::AppError;
use crate::utils::format_beijing_time;

use super::restore::{available_restore_groups, restore_attempts_for_release};
use super::stage::{
    build_device_detail, build_release_summary_stages, build_restore_summary_stages,
    parse_release_status,
};
use super::{
    ReleaseDetailResponse, ReleaseItemDto, ReleaseListQuery, ReleaseListResponse,
    RestoreAttemptDto, all_release_groups, latest_target_per_device_group, release_contains_group,
    release_system, sandbox_run_ready_for_release,
};

/// 获取发布版本列表。
pub async fn list_releases_logic(query: ReleaseListQuery) -> Result<ReleaseListResponse, AppError> {
    let (page, page_size) = query.page.normalize_default();
    let pipeline_param = query.pipeline.as_deref().or(query.note.as_deref());
    let created_by_param = query.created_by.as_deref().or(query.owner.as_deref());

    let (releases, total) = find_all_releases(
        Some(query.system),
        page,
        page_size,
        pipeline_param,
        query.version.as_deref(),
        created_by_param,
        query.status.as_deref(),
    )
    .await?;

    let all_system_releases = find_releases_by_system(query.system).await?;
    let has_draft = find_latest_draft_release(query.system).await?.is_some();
    let has_active_restore = find_active_restore_job(query.system).await?.is_some();
    let has_active_release = all_system_releases
        .iter()
        .any(|release| release.status == ReleaseStatus::RUNNING.as_ref());

    let mut items: Vec<ReleaseItemDto> = Vec::new();
    for rel in releases {
        let attempts = restore_attempts_for_release(rel.id).await?;
        let latest_restore = attempts.first().cloned();
        let restore_groups = if rel.status == ReleaseStatus::PASS.as_ref() {
            available_restore_groups(&rel, &all_system_releases)
        } else {
            Vec::new()
        };
        let (can_restore, restore_disabled_reason) = if has_draft {
            (false, Some("当前存在草稿".to_string()))
        } else if has_active_restore {
            (false, Some("当前存在进行中的还原任务".to_string()))
        } else if has_active_release {
            (false, Some("当前存在进行中的发布任务".to_string()))
        } else if restore_groups.is_empty() {
            (false, Some("没有可还原的历史发布范围".to_string()))
        } else {
            (true, None)
        };
        let latest_run = find_latest_sandbox_run(rel.id)
            .await
            .map_err(AppError::from)?;
        let sandbox_ready = latest_run
            .as_ref()
            .map(|run| sandbox_run_ready_for_release(&rel, run))
            .unwrap_or(false);

        items.push(ReleaseItemDto {
            id: rel.id,
            system: rel.system.clone(),
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
            can_restore,
            restore_disabled_reason,
            restore_groups,
            latest_restore,
        });
    }

    Ok(ReleaseListResponse::from_db(items, total, page, page_size))
}

/// 获取单个发布版本的详情。
pub async fn get_release_detail_logic(id: i32) -> Result<ReleaseDetailResponse, AppError> {
    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };
    let restore_job = find_restore_job_by_target_release(id).await?;

    let targets = latest_target_per_device_group(find_release_targets_by_release(id).await?);
    let device_ids: Vec<i32> = targets.iter().map(|t| t.device_id).collect();
    let devices = find_devices_by_ids(&device_ids).await?;
    let device_map: HashMap<i32, _> = devices.into_iter().map(|d| (d.id, d)).collect();

    let devices_detail = targets
        .into_iter()
        .map(|target| build_device_detail(&target, device_map.get(&target.device_id)))
        .collect::<Result<Vec<_>, _>>()?;

    let latest_run = find_latest_sandbox_run(id).await.map_err(AppError::from)?;
    let sandbox_ready = latest_run
        .as_ref()
        .map(|run| sandbox_run_ready_for_release(&release, run))
        .unwrap_or(false);
    let latest_sandbox_status = latest_run
        .as_ref()
        .map(|run| run.status.as_str().to_string());
    let latest_sandbox_task_id = latest_run.as_ref().map(|run| run.task_id.clone());

    let release_status = parse_release_status(&release)?;
    let restore_attempts = restore_attempts_for_release(id).await?;
    let restore_info = restore_job.as_ref().map(|job| RestoreAttemptDto {
        job_id: job.id,
        source_release_id: job.source_release_id,
        source_version: job.source_version.clone(),
        target_release_id: job.target_release_id,
        target_version: job.source_version.clone(),
        release_group: release.release_group.clone(),
        status: job.status.clone(),
        phase: job.phase.clone(),
        error_message: job.error_message.clone(),
        created_at: format_beijing_time(job.created_at),
        completed_at: job.completed_at.map(format_beijing_time),
    });
    let (previous_version, baseline_version) = if release_status == ReleaseStatus::WAIT {
        (None, None)
    } else {
        let mut previous_versions = Vec::new();
        for group in all_release_groups() {
            if !release_contains_group(&release.release_group, group) {
                continue;
            }
            let previous_release = find_latest_passed_release_by_group(
                release_system(&release)?,
                group.as_ref(),
                Some(id),
            )
            .await?;
            if let Some(prev) = previous_release.as_ref().map(|rel| rel.version.clone()) {
                previous_versions.push(prev);
            }
        }
        (
            previous_versions.first().cloned(),
            previous_versions.first().cloned(),
        )
    };

    Ok(ReleaseDetailResponse {
        id: release.id,
        system: release.system.clone(),
        version: release.version,
        release_group: release.release_group.clone(),
        status: release.status.clone(),
        pipeline: release.pipeline,
        owner: release.created_by,
        created_at: format_beijing_time(release.created_at),
        updated_at: format_beijing_time(release.updated_at),
        published_at: release.published_at.map(format_beijing_time),
        stages: restore_job
            .as_ref()
            .map(|job| {
                build_restore_summary_stages(
                    &job.status,
                    &job.phase,
                    &release.release_group,
                    &devices_detail,
                )
            })
            .unwrap_or_else(|| {
                build_release_summary_stages(sandbox_ready, &release_status, &release.release_group)
            }),
        error_message: release.error_message,
        devices: devices_detail,
        sandbox_ready,
        latest_sandbox_status,
        latest_sandbox_task_id,
        previous_version,
        baseline_version,
        restore_attempts,
        restore_info,
    })
}
