//! 还原任务持久化状态机。

use chrono::Utc;

use crate::constants::release::{GROUP_ALL, GROUP_MODELS, LOOP_IDLE_SECONDS};
use crate::db::{
    NewReleaseTarget, ReleaseGroup, ReleaseTarget, ReleaseTargetStatus, RestoreJobStatus,
    RestoreJobUpdate, RestorePhase, claim_restore_job, create_release_targets, find_devices_by_ids,
    find_release_by_id, find_release_targets_by_release, find_runnable_restore_jobs,
    release_restore_job_lock, update_release_group, update_release_status, update_restore_job,
};
use crate::error::AppError;
use crate::server::release::{default_target_stage_trace, serialize_stage_trace};
use crate::server::sync::{
    cleanup_restore_candidates, prepare_restore_candidates, promote_restore_candidate,
};
use crate::utils::SystemKind;

pub fn spawn_restore_task_runner() {
    tokio::spawn(async move {
        let idle = tokio::time::Duration::from_secs(LOOP_IDLE_SECONDS);
        loop {
            if let Err(error) = tick().await {
                warn!("还原任务调度失败: error={}", error);
            }
            tokio::time::sleep(idle).await;
        }
    });
}

async fn tick() -> Result<(), AppError> {
    let owner = format!("restore-runner-{}", std::process::id());
    for job in find_runnable_restore_jobs().await? {
        let Some(claimed) = claim_restore_job(job.id, &owner, 60).await? else {
            continue;
        };
        if let Err(error) = process_job(&claimed).await {
            warn!("推进还原任务失败: job_id={}, error={}", claimed.id, error);
            // 调度异常保留任务为可运行状态，下一轮继续重试；只有明确的业务失败
            // （设备发布失败、补偿失败等）才由状态机主动收口，避免瞬时数据库/Git
            // 故障把设备停在候选版本后直接标记为终态。
        }
        release_restore_job_lock(claimed.id, &owner).await?;
    }
    Ok(())
}

async fn process_job(job: &crate::db::ReleaseRestoreJob) -> Result<(), AppError> {
    let phase = job
        .phase
        .parse::<RestorePhase>()
        .map_err(|_| AppError::internal(format!("未知还原阶段: {}", job.phase)))?;
    match phase {
        RestorePhase::Queued => prepare(job).await,
        RestorePhase::TagReady => start_group(job, first_restore_group(job).await?).await,
        RestorePhase::ModelsRunning => monitor_group(job, ReleaseGroup::Models).await,
        RestorePhase::ModelsSuccess => continue_after_group(job, ReleaseGroup::Models).await,
        RestorePhase::InfraRunning => monitor_group(job, ReleaseGroup::Infra).await,
        RestorePhase::InfraSuccess => continue_after_group(job, ReleaseGroup::Infra).await,
        RestorePhase::Promoting | RestorePhase::PromotePending => promote(job).await,
        RestorePhase::RollbackRunning => monitor_compensation(job).await,
        RestorePhase::Preparing => {
            // 准备阶段没有触发设备发布，重试前可安全清掉上一次未持久化完整的候选标签。
            cleanup_candidates(job).await;
            prepare(job).await
        }
        RestorePhase::PrepareFailed
        | RestorePhase::ModelsFailed
        | RestorePhase::InfraFailed
        | RestorePhase::RollbackFailed
        | RestorePhase::Completed => Ok(()),
    }
}

async fn restore_groups(job: &crate::db::ReleaseRestoreJob) -> Result<Vec<ReleaseGroup>, AppError> {
    let release_group = find_release_by_id(job.target_release_id)
        .await?
        .map(|release| release.release_group)
        .ok_or_else(|| AppError::internal("还原目标版本不存在"))?;
    match release_group.as_str() {
        "models" => Ok(vec![ReleaseGroup::Models]),
        "infra" => Ok(vec![ReleaseGroup::Infra]),
        GROUP_ALL => Ok(vec![ReleaseGroup::Models, ReleaseGroup::Infra]),
        value => Err(AppError::internal(format!("未知还原范围: {value}"))),
    }
}

async fn first_restore_group(job: &crate::db::ReleaseRestoreJob) -> Result<ReleaseGroup, AppError> {
    restore_groups(job)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::internal("还原任务没有有效范围"))
}

async fn continue_after_group(
    job: &crate::db::ReleaseRestoreJob,
    group: ReleaseGroup,
) -> Result<(), AppError> {
    let groups = restore_groups(job).await?;
    if groups.contains(&ReleaseGroup::Models)
        && groups.contains(&ReleaseGroup::Infra)
        && group == ReleaseGroup::Models
    {
        start_group(job, ReleaseGroup::Infra).await
    } else {
        promote(job).await
    }
}

async fn prepare(job: &crate::db::ReleaseRestoreJob) -> Result<(), AppError> {
    update_restore_job(
        job.id,
        RestoreJobUpdate {
            status: Some(RestoreJobStatus::Running),
            phase: Some(RestorePhase::Preparing),
            mark_started: true,
            ..Default::default()
        },
    )
    .await?;

    let system = parse_system(&job.system)?;
    let groups = restore_groups(job).await?;
    let candidate_tag = restore_candidate_tag(job);
    match prepare_restore_candidates(system, &job.source_version, &candidate_tag, &groups) {
        Ok(candidate) => {
            update_restore_job(
                job.id,
                RestoreJobUpdate {
                    phase: Some(RestorePhase::TagReady),
                    models_previous_head: candidate.models_previous_head.as_deref(),
                    infra_previous_head: candidate.infra_previous_head.as_deref(),
                    models_candidate_commit: candidate.models_candidate_commit.as_deref(),
                    infra_candidate_commit: candidate.infra_candidate_commit.as_deref(),
                    error_code: Some(None),
                    error_message: Some(None),
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        Err(error) => {
            cleanup_candidates(job).await;
            update_restore_job(
                job.id,
                RestoreJobUpdate {
                    status: Some(RestoreJobStatus::Fail),
                    phase: Some(RestorePhase::PrepareFailed),
                    error_code: Some(Some("RESTORE_PREPARE_FAILED")),
                    error_message: Some(Some(&error.to_string())),
                    mark_completed: true,
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
    }
}

async fn start_group(
    job: &crate::db::ReleaseRestoreJob,
    group: ReleaseGroup,
) -> Result<(), AppError> {
    let device_ids: Vec<i32> = serde_json::from_str(&job.selected_device_ids)
        .map_err(|error| AppError::internal(format!("还原设备快照无效: {error}")))?;
    let devices = find_devices_by_ids(&device_ids).await?;
    if devices.len() != device_ids.len() {
        return fail_job(job.id, "RESTORE_DEVICE_MISSING", "部分目标设备不存在").await;
    }

    let existing = find_release_targets_by_release(job.target_release_id).await?;
    let existing_group = existing
        .iter()
        .filter(|target| target.release_group == group.as_ref() && target.operation == "publish")
        .collect::<Vec<_>>();
    if !existing_group.is_empty() {
        return set_running_phase(job.id, group).await;
    }

    let trace = serialize_stage_trace(&default_target_stage_trace());
    let mut targets = Vec::with_capacity(devices.len());
    for device in devices {
        // 全量还原按 models → infra 顺序执行。infra 开始时设备状态接口可能还没有
        // 完成上一阶段的版本回写，因此优先使用 models 目标创建时保存的整机版本快照，
        // 避免把“暂时未回写”误判成“设备没有 infra 当前版本”。
        let config_snapshot = existing
            .iter()
            .find(|target| {
                target.device_id == device.id
                    && target.operation == "publish"
                    && target.release_group == ReleaseGroup::Models.as_ref()
            })
            .and_then(|target| target.current_config_version.clone())
            .or_else(|| device.config_version.clone());
        let Some(previous) = extract_group_version(config_snapshot.as_deref(), group) else {
            // 如果前置分组已经成功，当前分组又无法创建目标，必须先补偿前置分组，
            // 不能直接结束任务，否则会留下“规则已还原、设施未还原”的半成功状态。
            if group == ReleaseGroup::Infra {
                return create_compensation_targets(job, group).await;
            }
            // 当前版本缺失是不可重试的业务校验失败。若直接把错误返回给调度器，
            // 任务会停留在 TAG_READY，并在每一轮调度中重复打印相同错误。
            return fail_job(
                job.id,
                "RESTORE_CURRENT_VERSION_MISSING",
                &format!(
                    "无法获取设备 {} 的 {} 当前版本，已停止还原",
                    device.id,
                    group.as_ref()
                ),
            )
            .await;
        };
        targets.push(NewReleaseTarget {
            release_id: job.target_release_id,
            device_id: device.id,
            release_group: group.as_ref().to_string(),
            status: ReleaseTargetStatus::QUEUED,
            stage_trace: Some(trace.clone()),
            remote_job_id: None,
            rollback_job_id: None,
            current_config_version: config_snapshot,
            target_config_version: job.source_version.clone(),
            client_version: device.client_version.clone(),
            error_message: None,
            next_poll_at: Some(Utc::now()),
            poll_attempts: 0,
            attempt_no: 1,
            operation: "publish".to_string(),
            previous_group_version: Some(previous),
            request_summary: None,
            response_status: None,
            response_summary: None,
        });
    }
    create_release_targets(targets).await?;
    set_running_phase(job.id, group).await
}

async fn set_running_phase(job_id: i32, group: ReleaseGroup) -> Result<(), AppError> {
    let phase = match group {
        ReleaseGroup::Models => RestorePhase::ModelsRunning,
        ReleaseGroup::Infra => RestorePhase::InfraRunning,
    };
    update_restore_job(
        job_id,
        RestoreJobUpdate {
            phase: Some(phase),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn monitor_group(
    job: &crate::db::ReleaseRestoreJob,
    group: ReleaseGroup,
) -> Result<(), AppError> {
    let targets = publish_targets(job.target_release_id, group).await?;
    if targets.is_empty() || targets.iter().any(is_target_running) {
        return Ok(());
    }
    if targets.iter().all(is_target_success) {
        let phase = match group {
            ReleaseGroup::Models => RestorePhase::ModelsSuccess,
            ReleaseGroup::Infra => RestorePhase::InfraSuccess,
        };
        update_restore_job(
            job.id,
            RestoreJobUpdate {
                phase: Some(phase),
                ..Default::default()
            },
        )
        .await?;
        return Ok(());
    }

    create_compensation_targets(job, group).await
}

async fn create_compensation_targets(
    job: &crate::db::ReleaseRestoreJob,
    failed_group: ReleaseGroup,
) -> Result<(), AppError> {
    let all_targets = find_release_targets_by_release(job.target_release_id).await?;
    let mut compensation = Vec::new();
    for target in all_targets.iter().filter(|target| {
        target.operation == "publish"
            && is_target_success(target)
            && (target.release_group == failed_group.as_ref()
                || (matches!(failed_group, ReleaseGroup::Infra)
                    && target.release_group == GROUP_MODELS))
    }) {
        let Some(previous) = target.previous_group_version.clone() else {
            return fail_job(
                job.id,
                "RESTORE_PREVIOUS_VERSION_MISSING",
                "缺少设备发布前版本，无法自动补偿",
            )
            .await;
        };
        compensation.push(NewReleaseTarget {
            release_id: job.target_release_id,
            device_id: target.device_id,
            release_group: target.release_group.clone(),
            status: ReleaseTargetStatus::ROLLBACK_PENDING,
            stage_trace: Some(serialize_stage_trace(&default_target_stage_trace())),
            remote_job_id: None,
            rollback_job_id: None,
            current_config_version: Some(job.source_version.clone()),
            target_config_version: previous.clone(),
            client_version: target.client_version.clone(),
            error_message: None,
            next_poll_at: Some(Utc::now()),
            poll_attempts: 0,
            attempt_no: 1,
            operation: "compensate".to_string(),
            previous_group_version: Some(job.source_version.clone()),
            request_summary: None,
            response_status: None,
            response_summary: None,
        });
    }

    if compensation.is_empty() {
        cleanup_candidates(job).await;
        return fail_job(job.id, "RESTORE_DEVICE_PUBLISH_FAILED", "设备发布失败").await;
    }
    create_release_targets(compensation).await?;
    update_restore_job(
        job.id,
        RestoreJobUpdate {
            phase: Some(RestorePhase::RollbackRunning),
            error_code: Some(Some(match failed_group {
                ReleaseGroup::Models => "RESTORE_MODELS_FAILED",
                ReleaseGroup::Infra => "RESTORE_INFRA_FAILED",
            })),
            error_message: Some(Some("设备发布失败，正在恢复发布前版本")),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn monitor_compensation(job: &crate::db::ReleaseRestoreJob) -> Result<(), AppError> {
    let targets = find_release_targets_by_release(job.target_release_id)
        .await?
        .into_iter()
        .filter(|target| target.operation == "compensate")
        .collect::<Vec<_>>();
    if targets.is_empty() || targets.iter().any(is_target_running) {
        return Ok(());
    }
    let success = targets
        .iter()
        .all(|target| target.status == ReleaseTargetStatus::ROLLED_BACK.as_ref());
    update_restore_job(
        job.id,
        RestoreJobUpdate {
            status: Some(if success {
                RestoreJobStatus::Fail
            } else {
                RestoreJobStatus::RollbackFailed
            }),
            phase: Some(if success {
                RestorePhase::Completed
            } else {
                RestorePhase::RollbackFailed
            }),
            error_code: Some(Some(if success {
                "RESTORE_COMPENSATED"
            } else {
                "RESTORE_ROLLBACK_FAILED"
            })),
            error_message: Some(Some(if success {
                "发布失败，已恢复所有受影响设备"
            } else {
                "发布失败且部分设备恢复失败，请人工处理"
            })),
            mark_completed: true,
            ..Default::default()
        },
    )
    .await?;
    if success {
        cleanup_candidates(job).await;
    }
    Ok(())
}

async fn promote(job: &crate::db::ReleaseRestoreJob) -> Result<(), AppError> {
    let system = parse_system(&job.system)?;
    let groups = restore_groups(job).await?;
    update_restore_job(
        job.id,
        RestoreJobUpdate {
            phase: Some(RestorePhase::Promoting),
            ..Default::default()
        },
    )
    .await?;

    if groups.contains(&ReleaseGroup::Models) && !job.models_promoted {
        let result = promote_restore_candidate(
            system,
            ReleaseGroup::Models,
            required(&job.models_previous_head, "models previous head")?,
            required(&job.models_candidate_commit, "models candidate commit")?,
        );
        if let Err(error) = result {
            return mark_promote_pending(job.id, &error.to_string()).await;
        }
        update_restore_job(
            job.id,
            RestoreJobUpdate {
                models_promoted: Some(true),
                ..Default::default()
            },
        )
        .await?;
    }
    if groups.contains(&ReleaseGroup::Infra) && !job.infra_promoted {
        let result = promote_restore_candidate(
            system,
            ReleaseGroup::Infra,
            required(&job.infra_previous_head, "infra previous head")?,
            required(&job.infra_candidate_commit, "infra candidate commit")?,
        );
        if let Err(error) = result {
            return mark_promote_pending(job.id, &error.to_string()).await;
        }
        update_restore_job(
            job.id,
            RestoreJobUpdate {
                infra_promoted: Some(true),
                ..Default::default()
            },
        )
        .await?;
    }

    let release_group = find_release_by_id(job.target_release_id)
        .await?
        .map(|release| release.release_group)
        .ok_or_else(|| AppError::internal("还原目标版本不存在"))?;
    update_release_group(job.target_release_id, &release_group).await?;
    update_release_status(
        job.target_release_id,
        crate::db::ReleaseStatus::PASS,
        None,
        None,
    )
    .await?;
    update_restore_job(
        job.id,
        RestoreJobUpdate {
            status: Some(RestoreJobStatus::Pass),
            phase: Some(RestorePhase::Completed),
            error_code: Some(None),
            error_message: Some(None),
            mark_completed: true,
            ..Default::default()
        },
    )
    .await?;
    info!(
        "还原发布成功: job_id={}, target_release_id={}, version={}",
        job.id, job.target_release_id, job.source_version
    );
    Ok(())
}

async fn publish_targets(
    release_id: i32,
    group: ReleaseGroup,
) -> Result<Vec<ReleaseTarget>, AppError> {
    Ok(find_release_targets_by_release(release_id)
        .await?
        .into_iter()
        .filter(|target| target.operation == "publish" && target.release_group == group.as_ref())
        .collect())
}

fn is_target_running(target: &ReleaseTarget) -> bool {
    matches!(
        target.status.as_str(),
        "QUEUED" | "RUNNING" | "ROLLBACK_PENDING" | "ROLLBACKING"
    )
}

fn is_target_success(target: &ReleaseTarget) -> bool {
    target.status == ReleaseTargetStatus::SUCCESS.as_ref()
}

fn extract_group_version(raw: Option<&str>, group: ReleaseGroup) -> Option<String> {
    let raw = raw?;
    raw.split(',').find_map(|entry| {
        let (key, value) = entry.trim().split_once('=')?;
        (key.trim() == group.as_ref()).then(|| value.trim().to_string())
    })
}

fn parse_system(raw: &str) -> Result<SystemKind, AppError> {
    raw.parse()
        .map_err(|_| AppError::internal(format!("未知系统: {raw}")))
}

fn required<'a>(value: &'a Option<String>, label: &str) -> Result<&'a str, AppError> {
    value
        .as_deref()
        .ok_or_else(|| AppError::internal(format!("还原任务缺少 {label}")))
}

async fn fail_job(job_id: i32, code: &str, message: &str) -> Result<(), AppError> {
    if let Some(job) = crate::db::find_restore_job_by_id(job_id).await? {
        cleanup_candidates(&job).await;
    }
    update_restore_job(
        job_id,
        RestoreJobUpdate {
            status: Some(RestoreJobStatus::Fail),
            phase: Some(RestorePhase::Completed),
            error_code: Some(Some(code)),
            error_message: Some(Some(message)),
            mark_completed: true,
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}

async fn cleanup_candidates(job: &crate::db::ReleaseRestoreJob) {
    if job.models_promoted || job.infra_promoted {
        return;
    }
    let Ok(system) = parse_system(&job.system) else {
        return;
    };
    let Ok(groups) = restore_groups(job).await else {
        warn!("还原任务范围无效，无法清理候选标签: job_id={}", job.id);
        return;
    };
    let candidate_tag = restore_candidate_tag(job);
    if let Err(error) = cleanup_restore_candidates(system, &candidate_tag, &groups) {
        warn!(
            "清理还原候选标签失败: job_id={}, candidate_tag={}, error={}",
            job.id, candidate_tag, error
        );
    }
}

/// 生成仅供还原事务使用的临时 Git 标签，避免与源版本正式标签冲突。
fn restore_candidate_tag(job: &crate::db::ReleaseRestoreJob) -> String {
    format!(
        "restore-{}-job-{}",
        job.source_version.replace('/', "-"),
        job.id
    )
}

async fn mark_promote_pending(job_id: i32, message: &str) -> Result<(), AppError> {
    update_restore_job(
        job_id,
        RestoreJobUpdate {
            phase: Some(RestorePhase::PromotePending),
            error_code: Some(Some("RESTORE_PROMOTE_PENDING")),
            error_message: Some(Some(message)),
            ..Default::default()
        },
    )
    .await?;
    Ok(())
}
