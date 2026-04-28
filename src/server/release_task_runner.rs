// 发布任务调度器：周期轮询 release_targets 并驱动 WarpParse 客户端

use crate::db::{
    Device, ReleaseStatus, ReleaseTarget, ReleaseTargetStatus, ReleaseTargetUpdate,
    find_devices_by_ids, find_due_release_targets, find_release_targets_by_release,
    update_device_runtime_state, update_release_status, update_release_target,
};
use crate::server::release::{
    parse_stage_trace, serialize_stage_summary, serialize_stage_trace, stage_summary_for_status,
};
use crate::server::setting::WarparseConf;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, OperationLogStatus,
    write_operation_log,
};
use crate::utils::WarpParseService;
use crate::utils::common::{
    FIRST_POLL_DELAY_SECONDS, LOOP_IDLE_SECONDS, MAX_BATCH_SIZE, STAGE_CALL_CLIENT, STAGE_RUNTIME,
};
use anyhow::Result;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use std::collections::{HashMap, HashSet};

pub fn spawn_release_task_runner(conf: WarparseConf) {
    tokio::spawn(async move { ReleaseTaskRunner::new(conf).run().await });
}

struct ReleaseTaskRunner {
    conf: WarparseConf,
    poll_interval: ChronoDuration,
    poll_timeout: ChronoDuration,
    max_retries: i32,
    service: WarpParseService,
}

impl ReleaseTaskRunner {
    fn new(conf: WarparseConf) -> Self {
        let poll_interval = ChronoDuration::seconds(conf.poll_interval_seconds.max(1) as i64);
        let poll_timeout = ChronoDuration::seconds(conf.poll_timeout_seconds.max(1) as i64);
        let max_retries = conf.max_retries.max(1) as i32;
        let service = WarpParseService::default();

        ReleaseTaskRunner {
            conf,
            poll_interval,
            poll_timeout,
            max_retries,
            service,
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

    async fn handle_deploy(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        is_rollback: bool,
    ) -> Result<bool> {
        let device = match device {
            Some(dev) => dev,
            None => {
                self.mark_target_fail(target, None, "目标设备不存在")
                    .await?;
                return Ok(true);
            }
        };

        if device.token.is_empty() {
            self.mark_target_fail(target, Some(device), "设备 Token 未配置")
                .await?;
            return Ok(true);
        }

        // 在发布前先推送代码并创建 tag（回滚操作跳过此步骤）
        if !is_rollback {
            info!(
                "发布前推送代码并打 tag: release_id={}, device_id={}, version={}",
                target.release_id, target.device_id, target.target_config_version
            );

            match crate::server::push_and_tag_release(&target.target_config_version).await {
                Ok(_) => {
                    info!(
                        "代码和 tag 推送成功: version={}",
                        target.target_config_version
                    );
                }
                Err(e) => {
                    let error_msg = format!("推送代码或创建 tag 失败: {}", e);
                    warn!("{}", error_msg);
                    self.mark_target_fail(target, Some(device), &error_msg)
                        .await?;
                    return Ok(true);
                }
            }
        }

        // 去除版本号 "v" 前缀（例如 "v1.0.0" -> "1.0.0"）
        let version = target
            .target_config_version
            .strip_prefix("v")
            .unwrap_or(&target.target_config_version);

        // 使用新服务发起部署
        let result = self.service.deploy(device, &self.conf, version).await;

        let resp = match result {
            Ok(resp) => resp,
            Err(err) => {
                self.mark_target_fail(target, Some(device), &format!("部署请求失败: {}", err))
                    .await?;
                return Ok(true);
            }
        };

        if !resp.accepted {
            let msg = resp.message.as_deref().unwrap_or("客户端拒绝本次发布");
            self.mark_target_fail(target, Some(device), msg).await?;
            return Ok(true);
        }

        let request_id = resp.request_id.clone();

        let update = ReleaseTargetUpdate {
            status: Some(if is_rollback {
                ReleaseTargetStatus::ROLLBACKING
            } else {
                ReleaseTargetStatus::RUNNING
            }),
            stage_trace: Some(Some(apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![
                    StageUpdate::new(STAGE_CALL_CLIENT, "pass", resp.message.clone()),
                    StageUpdate::new(
                        STAGE_RUNTIME,
                        "running",
                        Some("等待运行状态反馈".to_string()),
                    ),
                ],
            ))),
            remote_job_id: Some(if is_rollback {
                None
            } else {
                request_id.clone()
            }),
            rollback_job_id: Some(if is_rollback { request_id } else { None }),
            error_message: Some(None),
            // 首轮轮询提前，尽快刷新已完成的设备状态。
            next_poll_at: Some(Some(
                Utc::now() + ChronoDuration::seconds(FIRST_POLL_DELAY_SECONDS),
            )),
            poll_attempts: Some(0),
            completed_at: Some(None),
            ..Default::default()
        };

        update_release_target(target.id, update).await?;
        Ok(false)
    }

    async fn poll_target(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        is_rollback: bool,
    ) -> Result<bool> {
        let device = match device {
            Some(dev) => dev,
            None => {
                self.mark_target_fail(target, None, "目标设备不存在")
                    .await?;
                return Ok(true);
            }
        };

        if device.token.is_empty() {
            self.mark_target_fail(target, Some(device), "设备 Token 未配置")
                .await?;
            return Ok(true);
        }

        // 获取期望的 request_id
        let expected_request_id = if is_rollback {
            target.rollback_job_id.as_deref()
        } else {
            target.remote_job_id.as_deref()
        };

        // 去除版本号 "v" 前缀（例如 "v1.0.0" -> "1.0.0"）
        let version = target
            .target_config_version
            .strip_prefix("v")
            .unwrap_or(&target.target_config_version);

        // 使用新服务检查部署成功
        match self
            .service
            .check_deploy_success(device, &self.conf, version, expected_request_id)
            .await
        {
            Ok(result) => {
                if result.is_reloading {
                    // 仍在重载中，继续轮询
                    return self.schedule_next_poll(target, Some("正在重载配置")).await;
                }

                if result.is_success {
                    // 部署成功
                    self.mark_target_success(target, device, is_rollback, &result)
                        .await?;
                    Ok(true)
                } else {
                    // 版本不匹配或其他原因，继续轮询
                    let detail = format!(
                        "配置版本不匹配，期望: {}, 实际: {:?}",
                        target.target_config_version, result.current_version
                    );
                    self.schedule_next_poll(target, Some(&detail)).await
                }
            }
            Err(err) => {
                // API 调用失败
                self.handle_poll_error(target, Some(device), err).await
            }
        }
    }

    async fn schedule_next_poll(
        &self,
        target: &ReleaseTarget,
        detail: Option<&str>,
    ) -> Result<bool> {
        let next_attempts = target.poll_attempts + 1;
        let now = Utc::now();

        if self.should_timeout(target, next_attempts, now) {
            self.mark_target_fail(target, None, detail.unwrap_or("超出轮询上限，标记失败"))
                .await?;
            return Ok(true);
        }

        let update = ReleaseTargetUpdate {
            next_poll_at: Some(Some(now + self.poll_interval)),
            poll_attempts: Some(next_attempts),
            stage_trace: Some(Some(apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![StageUpdate::new(
                    STAGE_RUNTIME,
                    "running",
                    detail.map(|d| d.to_string()),
                )],
            ))),
            ..Default::default()
        };

        update_release_target(target.id, update).await?;
        Ok(false)
    }

    async fn handle_poll_error(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        error: crate::utils::ServiceError,
    ) -> Result<bool> {
        let next_attempts = target.poll_attempts + 1;
        let now = Utc::now();
        if self.should_timeout(target, next_attempts, now) {
            self.mark_target_fail(target, device, &format!("拉取运行状态失败: {}", error))
                .await?;
            return Ok(true);
        }

        let update = ReleaseTargetUpdate {
            next_poll_at: Some(Some(now + self.poll_interval)),
            poll_attempts: Some(next_attempts),
            stage_trace: Some(Some(apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![StageUpdate::new(
                    STAGE_RUNTIME,
                    "running",
                    Some(format!("状态查询失败: {}", error)),
                )],
            ))),
            ..Default::default()
        };

        update_release_target(target.id, update).await?;
        Ok(false)
    }

    async fn mark_target_success(
        &self,
        target: &ReleaseTarget,
        device: &Device,
        is_rollback: bool,
        result: &crate::utils::DeployCheckResult,
    ) -> Result<()> {
        let update = ReleaseTargetUpdate {
            status: Some(if is_rollback {
                ReleaseTargetStatus::ROLLED_BACK
            } else {
                ReleaseTargetStatus::SUCCESS
            }),
            stage_trace: Some(Some(apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![StageUpdate::new(
                    STAGE_RUNTIME,
                    "pass",
                    Some("配置重载成功".to_string()),
                )],
            ))),
            client_version: Some(result.current_version.clone()),
            error_message: Some(None),
            next_poll_at: Some(None),
            poll_attempts: Some(target.poll_attempts + 1),
            completed_at: Some(Some(Utc::now())),
            ..Default::default()
        };

        update_release_target(target.id, update).await?;

        let _ = update_device_runtime_state(
            device.id,
            None,
            Some(&target.target_config_version),
            Some(target.release_id),
            Some(Utc::now()),
        )
        .await;

        self.log_device_event(
            target,
            Some(device),
            if is_rollback {
                "ROLLED_BACK"
            } else {
                "SUCCESS"
            },
            "配置重载成功",
        )
        .await;

        Ok(())
    }

    async fn mark_target_fail(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        message: &str,
    ) -> Result<()> {
        let update = ReleaseTargetUpdate {
            status: Some(ReleaseTargetStatus::FAIL),
            stage_trace: Some(Some(apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![StageUpdate::new(
                    STAGE_RUNTIME,
                    "fail",
                    Some(message.to_string()),
                )],
            ))),
            error_message: Some(Some(message.to_string())),
            next_poll_at: Some(None),
            completed_at: Some(Some(Utc::now())),
            ..Default::default()
        };

        update_release_target(target.id, update).await?;

        self.log_device_event(target, device, "FAIL", message).await;
        Ok(())
    }

    fn should_timeout(
        &self,
        target: &ReleaseTarget,
        next_attempts: i32,
        now: DateTime<Utc>,
    ) -> bool {
        if next_attempts >= self.max_retries {
            return true;
        }

        let elapsed = now - target.updated_at;
        elapsed >= self.poll_timeout
    }

    async fn refresh_release_status(&self, release_id: i32) -> Result<()> {
        let targets = find_release_targets_by_release(release_id).await?;
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
                ReleaseTargetStatus::QUEUED
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

        let summary = serialize_stage_summary(&stage_summary_for_status(&new_status));
        let error_text = if fail_messages.is_empty() {
            None
        } else {
            Some(fail_messages.join("; "))
        };

        update_release_status(
            release_id,
            new_status,
            error_text.as_deref(),
            Some(&summary),
        )
        .await?;
        Ok(())
    }

    async fn log_device_event(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        status: &str,
        message: &str,
    ) {
        let label = device
            .and_then(|d| d.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| format!("设备{}", target.device_id));

        let (action, log_status, description) = match status {
            "SUCCESS" => (
                OperationLogAction::Publish,
                OperationLogStatus::Success,
                "发布成功",
            ),
            "ROLLED_BACK" => (
                OperationLogAction::Rollback,
                OperationLogStatus::Success,
                "回滚成功",
            ),
            _ => (
                OperationLogAction::Publish,
                OperationLogStatus::Error,
                "发布失败",
            ),
        };

        write_operation_log(
            OperationLogBiz::ReleaseTarget,
            action,
            OperationLogParams::new()
                .with_target_name(format!("发布{} -> {}", target.release_id, label))
                .with_target_id(target.device_id.to_string())
                .with_field("target_id", target.id.to_string())
                .with_field("release_id", target.release_id.to_string())
                .with_field("version", &target.target_config_version)
                .with_field("detail", format!("{}: {}", description, message)),
            log_status,
        )
        .await;
    }
}

struct StageUpdate<'a> {
    label: &'a str,
    status: &'a str,
    detail: Option<String>,
}

impl<'a> StageUpdate<'a> {
    fn new(label: &'a str, status: &'a str, detail: Option<String>) -> Self {
        StageUpdate {
            label,
            status,
            detail,
        }
    }
}

fn apply_stage_updates(raw: Option<&str>, updates: Vec<StageUpdate<'_>>) -> String {
    let mut stages = parse_stage_trace(raw);
    for update in updates {
        if let Some(stage) = stages.iter_mut().find(|s| s.label == update.label) {
            stage.status = update.status.to_string();
            stage.detail = update.detail.clone();
        }
    }
    serialize_stage_trace(&stages)
}
