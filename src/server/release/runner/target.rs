//! 发布目标执行与轮询逻辑。

use super::status::{StageUpdate, apply_stage_updates};
use super::*;
use crate::db::ReleaseGroup;
use crate::utils::{DeployCheckResult, DeployResult, PublishPayload, ServiceError, SystemKind};

impl ReleaseTaskRunner {
    pub(super) async fn handle_deploy(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        is_rollback: bool,
    ) -> Result<bool> {
        let device = match device {
            Some(dev) => dev,
            None => {
                self.mark_target_fail(target, "目标设备不存在").await?;
                return Ok(true);
            }
        };

        if device.token.is_empty() {
            self.mark_target_fail(target, "设备 Token 未配置").await?;
            return Ok(true);
        }

        let version = target
            .target_config_version
            .strip_prefix("v")
            .unwrap_or(&target.target_config_version);
        let release_group = ReleaseGroup::parse(&target.release_group)
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let result = self.deploy_to_device(device, version, release_group).await;

        let resp = match result {
            Ok(resp) => resp,
            Err(err) => {
                self.mark_target_fail(target, &format!("部署请求失败: {}", err))
                    .await?;
                return Ok(true);
            }
        };

        if !resp.accepted {
            let msg = resp.message.as_deref().unwrap_or("客户端拒绝本次发布");
            self.mark_target_fail(target, msg).await?;
            return Ok(true);
        }

        let request_id = resp.request_id.clone();
        if resp.completed {
            let staged_trace = apply_stage_updates(
                target.stage_trace.as_deref(),
                vec![StageUpdate::new(
                    STAGE_CALL_CLIENT,
                    "pass",
                    resp.message.clone(),
                )],
            );
            let update = ReleaseTargetUpdate {
                stage_trace: Some(Some(staged_trace.clone())),
                remote_job_id: Some(if is_rollback {
                    None
                } else {
                    request_id.clone()
                }),
                rollback_job_id: Some(if is_rollback {
                    request_id.clone()
                } else {
                    None
                }),
                ..Default::default()
            };
            update_release_target(target.id, update).await?;

            let mut completed_target = target.clone();
            completed_target.stage_trace = Some(staged_trace);
            completed_target.remote_job_id = if is_rollback {
                None
            } else {
                request_id.clone()
            };
            completed_target.rollback_job_id = if is_rollback {
                request_id.clone()
            } else {
                None
            };

            let result = DeployCheckResult {
                is_success: true,
                current_version: Some(target.target_config_version.clone()),
                config_version: Some(target.target_config_version.clone()),
                is_reloading: false,
            };
            self.mark_target_success(&completed_target, device, is_rollback, &result)
                .await?;
            return Ok(true);
        }

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

    pub(super) async fn poll_target(
        &self,
        target: &ReleaseTarget,
        device: Option<&Device>,
        is_rollback: bool,
    ) -> Result<bool> {
        let device = match device {
            Some(dev) => dev,
            None => {
                self.mark_target_fail(target, "目标设备不存在").await?;
                return Ok(true);
            }
        };

        if device.token.is_empty() {
            self.mark_target_fail(target, "设备 Token 未配置").await?;
            return Ok(true);
        }

        let expected_request_id = if is_rollback {
            target.rollback_job_id.as_deref()
        } else {
            target.remote_job_id.as_deref()
        };
        let release_group = ReleaseGroup::parse(&target.release_group)
            .map_err(|err| anyhow::anyhow!(err.to_string()))?;
        let version = target
            .target_config_version
            .strip_prefix("v")
            .unwrap_or(&target.target_config_version);

        match self
            .check_deploy_status(device, version, release_group, expected_request_id)
            .await
        {
            Ok(result) => {
                if result.is_reloading {
                    return self.schedule_next_poll(target, Some("正在重载配置")).await;
                }

                if result.is_success {
                    self.mark_target_success(target, device, is_rollback, &result)
                        .await?;
                    Ok(true)
                } else {
                    let detail = format!(
                        "配置版本不匹配，期望: {}, 实际: {:?}",
                        target.target_config_version, result.current_version
                    );
                    self.schedule_next_poll(target, Some(&detail)).await
                }
            }
            Err(err) => self.handle_poll_error(target, err).await,
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
            self.mark_target_fail(target, detail.unwrap_or("超出轮询上限，标记失败"))
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

    async fn handle_poll_error(&self, target: &ReleaseTarget, error: ServiceError) -> Result<bool> {
        let next_attempts = target.poll_attempts + 1;
        let now = Utc::now();
        if self.should_timeout(target, next_attempts, now) {
            self.mark_target_fail(target, &format!("拉取运行状态失败: {}", error))
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

        Ok(())
    }

    async fn mark_target_fail(&self, target: &ReleaseTarget, message: &str) -> Result<()> {
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

    async fn deploy_to_device(
        &self,
        device: &Device,
        version: &str,
        release_group: ReleaseGroup,
    ) -> Result<DeployResult, ServiceError> {
        if device.system == SystemKind::Wfusion.as_ref() {
            return self
                .wfusion_service
                .publish(
                    device,
                    PublishPayload {
                        version: version.to_string(),
                        release_group: release_group.as_ref().to_string(),
                    },
                )
                .await;
        }

        self.service.deploy(device, version, release_group).await
    }

    async fn check_deploy_status(
        &self,
        device: &Device,
        version: &str,
        release_group: ReleaseGroup,
        expected_request_id: Option<&str>,
    ) -> Result<DeployCheckResult, ServiceError> {
        if device.system == SystemKind::Wfusion.as_ref() {
            return self
                .wfusion_service
                .check_deploy_success(device, version, expected_request_id)
                .await;
        }

        self.service
            .check_deploy_success(device, version, release_group, expected_request_id)
            .await
    }
}
