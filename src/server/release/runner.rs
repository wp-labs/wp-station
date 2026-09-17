//! 发布任务调度器。
//!
//! 周期轮询 `release_targets`，并通过系统对应的设备服务
//! 驱动设备端发布、轮询结果和聚合状态刷新。

mod status;
mod target;

use crate::constants::release::{
    FIRST_POLL_DELAY_SECONDS, GROUP_ALL, GROUP_INFRA, GROUP_MODELS, LOOP_IDLE_SECONDS,
    MAX_BATCH_SIZE, STAGE_CALL_CLIENT, STAGE_RUNTIME,
};
use crate::db::{
    Device, ReleaseStatus, ReleaseTarget, ReleaseTargetStatus, ReleaseTargetUpdate,
    find_devices_by_ids, find_due_release_targets, find_release_by_id,
    find_release_targets_by_release, update_device_runtime_state, update_release_group,
    update_release_status, update_release_target,
};
use crate::server::release::{
    parse_stage_trace, serialize_stage_summary, serialize_stage_trace, stage_summary_for_release,
};
use crate::server::setting::AdminApiConf;
use crate::utils::{WarpParseService, WfusionService};
use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::collections::{HashMap, HashSet};

/// 启动发布任务后台轮询协程。
pub fn spawn_release_task_runner(conf: AdminApiConf) {
    tokio::spawn(async move { ReleaseTaskRunner::new(conf).run().await });
}

struct ReleaseTaskRunner {
    poll_interval: ChronoDuration,
    poll_timeout: ChronoDuration,
    max_retries: i32,
    service: WarpParseService,
    wfusion_service: WfusionService,
}

impl ReleaseTaskRunner {
    fn new(conf: AdminApiConf) -> Self {
        let poll_interval = ChronoDuration::seconds(conf.poll_interval_seconds.max(1) as i64);
        let poll_timeout = ChronoDuration::seconds(conf.poll_timeout_seconds.max(1) as i64);
        let max_retries = conf.max_retries.max(1) as i32;
        let service = WarpParseService::default();
        let wfusion_service = WfusionService::default();

        ReleaseTaskRunner {
            poll_interval,
            poll_timeout,
            max_retries,
            service,
            wfusion_service,
        }
    }

    async fn run(self) {
        let idle = tokio::time::Duration::from_secs(LOOP_IDLE_SECONDS);
        loop {
            let had_work = match self.tick().await {
                Ok(has_work) => has_work,
                Err(err) => {
                    warn!("发布任务调度器执行失败: error={}", err);
                    false
                }
            };

            if !had_work {
                tokio::time::sleep(idle).await;
            }
        }
    }

    async fn tick(&self) -> Result<bool> {
        let now = Utc::now();
        let targets = find_due_release_targets(now, MAX_BATCH_SIZE).await?;
        if targets.is_empty() {
            return Ok(false);
        }

        let device_ids: Vec<i32> = targets.iter().map(|t| t.device_id).collect();
        let devices = find_devices_by_ids(&device_ids).await?;
        let device_map: HashMap<i32, Device> = devices.into_iter().map(|d| (d.id, d)).collect();

        let mut touched_releases = HashSet::new();
        for target in targets {
            let device = device_map.get(&target.device_id);
            match self.process_target(&target, device).await {
                Ok(need_refresh) => {
                    if need_refresh {
                        touched_releases.insert(target.release_id);
                    }
                }
                Err(err) => {
                    warn!(
                        "处理发布子任务失败: target_id={}, release_id={}, error={}",
                        target.id, target.release_id, err
                    );
                }
            }
        }

        for release_id in touched_releases {
            if let Err(err) = self.refresh_release_status(release_id).await {
                warn!(
                    "刷新发布单状态失败: release_id={}, error={}",
                    release_id, err
                );
            }
        }

        Ok(true)
    }

    async fn process_target(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
    ) -> Result<bool> {
        let status = match target.status.parse::<ReleaseTargetStatus>() {
            Ok(s) => s,
            Err(_) => {
                warn!(
                    "未知发布子任务状态: target_id={}, status={}",
                    target.id, target.status
                );
                return Ok(false);
            }
        };

        match status {
            ReleaseTargetStatus::QUEUED => self.handle_deploy(target, device, false).await,
            ReleaseTargetStatus::ROLLBACK_PENDING => self.handle_deploy(target, device, true).await,
            ReleaseTargetStatus::RUNNING => self.poll_target(target, device, false).await,
            ReleaseTargetStatus::ROLLBACKING => self.poll_target(target, device, true).await,
            _ => Ok(false),
        }
    }
}
