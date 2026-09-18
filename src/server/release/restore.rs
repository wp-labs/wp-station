//! 历史成功版本还原任务创建与查询。

use chrono::Utc;

use crate::constants::release::{GROUP_ALL, GROUP_INFRA, GROUP_MODELS};
use crate::db::{
    NewRestoreJob, Release, ReleaseGroup, ReleaseStatus, create_restore_job,
    find_active_restore_job, find_devices_by_ids, find_latest_draft_release, find_release_by_id,
    find_release_targets_by_release, find_releases_by_system, find_restore_job_by_id,
    find_restore_jobs_by_source,
};
use crate::error::AppError;
use crate::utils::{SystemKind, format_beijing_time};

use super::{
    ReleaseRestoreRequest, ReleaseRestoreResponse, RestoreAttemptDto, RestoreJobDetailResponse,
};

/// 返回可参与指定范围还原判定的成功版本，列表页和创建还原任务必须共用这套口径。
///
/// 只统计包含目标范围的成功记录；INIT、草稿和失败记录都不能让历史版本
/// 凭空获得“还原”按钮。发布时间相同时再用更新时间和 ID 保证最新版本
/// 判定稳定，避免列表刷新后按钮在两条记录之间跳动。
pub(super) fn successful_releases_for_group(
    releases: &[Release],
    group: ReleaseGroup,
) -> Vec<&Release> {
    let mut passed = releases
        .iter()
        .filter(|release| release.status == ReleaseStatus::PASS.as_ref())
        .filter(|release| super::release_contains_group(&release.release_group, group))
        .collect::<Vec<_>>();
    passed.sort_by(|left, right| {
        left.published_at
            .cmp(&right.published_at)
            .then_with(|| left.updated_at.cmp(&right.updated_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    passed
}

/// 解析用户在还原弹窗选择的范围。
pub(super) fn parse_restore_groups(value: Option<&str>) -> Result<Vec<ReleaseGroup>, AppError> {
    match value.unwrap_or(GROUP_ALL) {
        GROUP_MODELS => Ok(vec![ReleaseGroup::Models]),
        GROUP_INFRA => Ok(vec![ReleaseGroup::Infra]),
        GROUP_ALL => Ok(vec![ReleaseGroup::Models, ReleaseGroup::Infra]),
        value => Err(AppError::validation(format!("无效的还原范围: {value}"))),
    }
}

/// 返回来源版本当前允许选择的还原范围。
pub(super) fn available_restore_groups(source: &Release, releases: &[Release]) -> Vec<String> {
    let source_groups = match source.release_group.as_str() {
        GROUP_ALL => vec![ReleaseGroup::Models, ReleaseGroup::Infra],
        GROUP_MODELS => vec![ReleaseGroup::Models],
        GROUP_INFRA => vec![ReleaseGroup::Infra],
        _ => Vec::new(),
    };
    let mut groups = source_groups
        .iter()
        .filter(|group| {
            let passed = successful_releases_for_group(releases, **group);
            passed.len() >= 2 && passed.last().map(|release| release.id) != Some(source.id)
        })
        .map(|group| group.as_ref().to_string())
        .collect::<Vec<_>>();

    if source.release_group == GROUP_ALL
        && groups.iter().any(|group| group == GROUP_MODELS)
        && groups.iter().any(|group| group == GROUP_INFRA)
    {
        groups.push(GROUP_ALL.to_string());
    }
    groups
}

/// 创建还原任务。真正执行由 restore runner 异步完成。
pub async fn create_restore_job_logic(
    source_release_id: i32,
    req: ReleaseRestoreRequest,
) -> Result<ReleaseRestoreResponse, AppError> {
    if req.device_ids.is_empty() {
        return Err(AppError::validation("请至少选择一台目标设备"));
    }

    let source = find_release_by_id(source_release_id)
        .await?
        .ok_or_else(|| AppError::not_found("发布记录不存在"))?;
    let system = source
        .system
        .parse::<SystemKind>()
        .map_err(|_| AppError::validation("发布记录系统无效"))?;
    if req.system != system {
        return Err(AppError::validation("发布记录所属系统与当前系统不一致"));
    }
    if source.status != ReleaseStatus::PASS.as_ref() {
        return Err(AppError::validation("仅支持还原发布成功的历史版本"));
    }

    let requested_group = req.release_group.as_deref().unwrap_or(GROUP_ALL);
    let groups = parse_restore_groups(Some(requested_group))?;
    if groups
        .iter()
        .any(|group| !super::release_contains_group(&source.release_group, *group))
    {
        return Err(AppError::validation("来源版本不包含所选还原范围"));
    }

    let releases = find_releases_by_system(system).await?;
    for group in &groups {
        let passed = successful_releases_for_group(&releases, *group);
        if passed.len() < 2 {
            return Err(AppError::validation(format!(
                "至少存在两个 {} 发布成功版本时才能还原",
                group.as_ref()
            )));
        }
        if passed.last().map(|release| release.id) == Some(source.id) {
            return Err(AppError::validation("所选范围的最新成功版本无需还原"));
        }
    }

    if find_latest_draft_release(system).await?.is_some() {
        return Err(AppError::validation("当前存在草稿，不能执行还原"));
    }
    if let Some(active) = find_active_restore_job(system).await? {
        let active_group = find_release_by_id(active.target_release_id)
            .await?
            .map(|release| release.release_group)
            .unwrap_or_else(|| GROUP_ALL.to_string());
        if active.source_release_id == source.id && active_group == requested_group {
            return Ok(ReleaseRestoreResponse {
                success: true,
                message: "当前系统已有还原任务，已返回原任务".to_string(),
                job_id: active.id,
                target_release_id: active.target_release_id,
                source_version: active.source_version.clone(),
                target_version: active.source_version,
                release_group: active_group,
                status: active.status,
            });
        }
        return Err(AppError::validation("当前系统已有进行中的还原任务"));
    }
    if releases
        .iter()
        .any(|release| release.status == ReleaseStatus::RUNNING.as_ref())
    {
        return Err(AppError::validation("当前系统已有进行中的发布任务"));
    }

    let devices = find_devices_by_ids(&req.device_ids).await?;
    if devices.len() != req.device_ids.len()
        || devices
            .iter()
            .any(|device| device.system != system.as_ref())
    {
        return Err(AppError::validation("部分设备不存在或不属于当前系统"));
    }

    // 还原不产生新的业务版本号。任务表继续使用不可见的内部标识，
    // 避免同一历史版本再次还原时触发任务表的唯一约束。
    let target_version = format!(
        "restore-{}-{}-{}",
        source.id,
        requested_group,
        Utc::now().timestamp_millis()
    );
    let job = create_restore_job(NewRestoreJob {
        system,
        source_release_id: source.id,
        source_version: source.version.clone(),
        release_version: source.version.clone(),
        target_version: target_version.clone(),
        release_group: requested_group.to_string(),
        selected_device_ids: req.device_ids,
        note: req.note,
        created_by: None,
    })
    .await?;

    info!(
        "创建还原任务成功: job_id={}, source_release_id={}, target_release_id={}, source_version={}, target_version={}",
        job.id, source.id, job.target_release_id, source.version, target_version
    );

    Ok(ReleaseRestoreResponse {
        success: true,
        message: "还原任务已创建".to_string(),
        job_id: job.id,
        target_release_id: job.target_release_id,
        source_version: job.source_version.clone(),
        target_version: job.source_version,
        release_group: requested_group.to_string(),
        status: job.status,
    })
}

pub async fn get_restore_job_logic(id: i32) -> Result<RestoreJobDetailResponse, AppError> {
    let job = find_restore_job_by_id(id)
        .await?
        .ok_or_else(|| AppError::not_found("还原任务不存在"))?;
    let targets = find_release_targets_by_release(job.target_release_id).await?;
    let release_group = find_release_by_id(job.target_release_id)
        .await?
        .map(|release| release.release_group)
        .unwrap_or_else(|| GROUP_ALL.to_string());
    Ok(RestoreJobDetailResponse::from_models(
        job,
        targets,
        release_group,
    ))
}

pub async fn restore_attempts_for_release(
    source_release_id: i32,
) -> Result<Vec<RestoreAttemptDto>, AppError> {
    let jobs = find_restore_jobs_by_source(source_release_id).await?;
    let mut attempts = Vec::with_capacity(jobs.len());
    for job in jobs {
        let release_group = find_release_by_id(job.target_release_id)
            .await?
            .map(|release| release.release_group)
            .unwrap_or_else(|| GROUP_ALL.to_string());
        attempts.push(RestoreAttemptDto {
            job_id: job.id,
            source_release_id: job.source_release_id,
            source_version: job.source_version.clone(),
            target_release_id: job.target_release_id,
            target_version: job.source_version,
            release_group,
            status: job.status,
            phase: job.phase,
            error_message: job.error_message,
            created_at: format_beijing_time(job.created_at),
            completed_at: job.completed_at.map(format_beijing_time),
        });
    }
    Ok(attempts)
}
