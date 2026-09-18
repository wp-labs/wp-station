//! 发布状态汇总与阶段轨迹辅助。

use super::*;

/// 对同一设备同一发布分组只保留最新一条子任务记录。
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

impl ReleaseTaskRunner {
    pub(super) async fn refresh_release_status(&self, release_id: i32) -> Result<()> {
        // 还原发布由独立状态机按 models → infra → promote 编排，不能在 models
        // 单阶段成功后由普通聚合器提前把 INIT 目标版本标记为 PASS。
        if find_restore_job_by_target_release(release_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        let release = match find_release_by_id(release_id).await? {
            Some(release) => release,
            None => return Ok(()),
        };
        let targets =
            latest_target_per_device_group(find_release_targets_by_release(release_id).await?);
        if targets.is_empty() {
            return Ok(());
        }

        let mut success = 0;
        let mut fail = 0;
        let mut running = 0;
        let mut fail_messages = Vec::new();

        for target in &targets {
            let status = match target.status.parse::<ReleaseTargetStatus>() {
                Ok(s) => s,
                Err(_) => continue,
            };
            match status {
                ReleaseTargetStatus::SUCCESS | ReleaseTargetStatus::ROLLED_BACK => success += 1,
                ReleaseTargetStatus::FAIL => {
                    fail += 1;
                    let msg = target.error_message.as_deref().unwrap_or("未知错误");
                    fail_messages.push(format!("设备{}: {}", target.device_id, msg));
                }
                ReleaseTargetStatus::PENDING
                | ReleaseTargetStatus::QUEUED
                | ReleaseTargetStatus::RUNNING
                | ReleaseTargetStatus::ROLLBACK_PENDING
                | ReleaseTargetStatus::ROLLBACKING => running += 1,
            }
        }

        let new_status = if running > 0 {
            if fail > 0 {
                ReleaseStatus::PARTIAL_FAIL
            } else {
                ReleaseStatus::RUNNING
            }
        } else if fail == 0 {
            ReleaseStatus::PASS
        } else if success == 0 {
            ReleaseStatus::FAIL
        } else {
            ReleaseStatus::PARTIAL_FAIL
        };

        let aggregated_group = match (
            targets
                .iter()
                .any(|target| target.release_group == GROUP_MODELS),
            targets
                .iter()
                .any(|target| target.release_group == GROUP_INFRA),
        ) {
            (true, true) => GROUP_ALL.to_string(),
            (true, false) => GROUP_MODELS.to_string(),
            (false, true) => GROUP_INFRA.to_string(),
            (false, false) => release.release_group.clone(),
        };

        let summary =
            serialize_stage_summary(&stage_summary_for_release(&new_status, &aggregated_group));
        let error_text = if fail_messages.is_empty() {
            None
        } else {
            Some(fail_messages.join("; "))
        };

        info!(
            "刷新发布单状态: release_id={}, previous_status={}, new_status={}, aggregated_group={}, success={}, fail={}, running={}",
            release_id,
            release.status,
            new_status.as_ref(),
            aggregated_group,
            success,
            fail,
            running
        );

        update_release_group(release_id, &aggregated_group).await?;
        update_release_status(
            release_id,
            new_status,
            error_text.as_deref(),
            Some(&summary),
        )
        .await?;
        Ok(())
    }
}

pub(super) struct StageUpdate<'a> {
    label: &'a str,
    status: &'a str,
    detail: Option<String>,
}

impl<'a> StageUpdate<'a> {
    pub(super) fn new(label: &'a str, status: &'a str, detail: Option<String>) -> Self {
        StageUpdate {
            label,
            status,
            detail,
        }
    }
}

pub(super) fn apply_stage_updates(raw: Option<&str>, updates: Vec<StageUpdate<'_>>) -> String {
    let mut stages = parse_stage_trace(raw);
    for update in updates {
        if let Some(stage) = stages.iter_mut().find(|s| s.label == update.label) {
            stage.status = update.status.to_string();
            stage.detail = update.detail.clone();
        }
    }
    serialize_stage_trace(&stages)
}
