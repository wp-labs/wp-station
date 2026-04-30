//! WarpParse 服务统一封装层。
//!
//! 封装设备在线检查、配置部署、部署状态验证等 WarpParse API 调用，是设备通信的唯一入口。

use crate::db::Device;
use crate::db::ReleaseGroup;
use crate::server::setting::WarparseConf;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

// ============ 错误类型 ============

#[derive(Debug)]
pub enum ServiceError {
    Network(String),
    Response(String),
    InvalidState(String),
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::Network(msg) => write!(f, "网络错误: {}", msg),
            ServiceError::Response(msg) => write!(f, "响应错误: {}", msg),
            ServiceError::InvalidState(msg) => write!(f, "状态异常: {}", msg),
        }
    }
}

impl std::error::Error for ServiceError {}

// ============ 请求/响应结构 ============

#[derive(Serialize, Debug)]
struct ReloadRequest<'a> {
    wait: bool,
    update: bool,
    version: &'a str,
    group: &'a str,
    timeout_ms: u64,
    reason: &'a str,
}

#[derive(Deserialize)]
struct ReloadResponse {
    accepted: Option<bool>,
    message: Option<String>,
    request_id: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
struct ProjectVersionEntry {
    tag: Option<String>,
    version: Option<String>,
}

#[derive(Deserialize, Clone, Debug, Default)]
struct ProjectVersionMap {
    models: Option<ProjectVersionEntry>,
    infra: Option<ProjectVersionEntry>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum ProjectVersionPayload {
    LegacyString(String),
    Grouped(ProjectVersionMap),
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct StatusResponse {
    version: Option<String>,
    project_version: Option<ProjectVersionPayload>,
    accepting_commands: Option<bool>,
    reloading: Option<bool>,
    current_request_id: Option<String>,
    last_reload_request_id: Option<String>,
    last_reload_result: Option<String>,
}

// ============ 服务结果类型 ============

/// 设备在线检查结果
pub struct OnlineStatus {
    pub is_online: bool,
    pub client_version: Option<String>,
    pub config_version: Option<String>,
}

/// 部署操作结果
pub struct DeployResult {
    pub accepted: bool,
    pub request_id: Option<String>,
    pub message: Option<String>,
}

/// 部署成功检查结果
pub struct DeployCheckResult {
    pub is_success: bool,
    pub current_version: Option<String>,
    pub config_version: Option<String>,
    pub is_reloading: bool,
}

fn normalize_version_for_compare(version: &str) -> &str {
    version
        .strip_prefix('v')
        .or_else(|| version.strip_prefix('V'))
        .unwrap_or(version)
}

fn normalize_version_for_display(version: &str) -> String {
    if version.starts_with('v') || version.starts_with('V') {
        version.to_string()
    } else {
        format!("v{}", version)
    }
}

fn version_entry_value(entry: &ProjectVersionEntry) -> Option<String> {
    entry
        .tag
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            entry
                .version
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(normalize_version_for_display)
        })
}

fn project_version_summary(project_version: Option<&ProjectVersionPayload>) -> Option<String> {
    match project_version {
        Some(ProjectVersionPayload::LegacyString(value)) if !value.trim().is_empty() => {
            Some(value.to_string())
        }
        Some(ProjectVersionPayload::Grouped(grouped)) => {
            let mut parts = Vec::new();
            if let Some(models) = grouped.models.as_ref().and_then(version_entry_value) {
                parts.push(format!("models={}", models));
            }
            if let Some(infra) = grouped.infra.as_ref().and_then(version_entry_value) {
                parts.push(format!("infra={}", infra));
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(", "))
            }
        }
        _ => None,
    }
}

fn project_version_for_group(
    project_version: Option<&ProjectVersionPayload>,
    group: ReleaseGroup,
) -> Option<String> {
    match project_version {
        Some(ProjectVersionPayload::LegacyString(value)) if !value.trim().is_empty() => {
            Some(value.to_string())
        }
        Some(ProjectVersionPayload::Grouped(grouped)) => match group {
            ReleaseGroup::Models => grouped.models.as_ref().and_then(version_entry_value),
            ReleaseGroup::Infra => grouped.infra.as_ref().and_then(version_entry_value),
        },
        _ => None,
    }
}

// ============ WarpParseService ============

pub struct WarpParseService {
    client: Client,
}

impl WarpParseService {
    pub fn new() -> Result<Self, ServiceError> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .map_err(|e| ServiceError::Network(e.to_string()))?;

        Ok(WarpParseService { client })
    }

    pub fn with_timeout(timeout: Duration) -> Result<Self, ServiceError> {
        let client = Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(timeout)
            .build()
            .map_err(|e| ServiceError::Network(e.to_string()))?;

        Ok(WarpParseService { client })
    }

    /// 检查设备是否在线
    /// 判断标准：accepting_commands == true
    pub async fn check_online(
        &self,
        device: &Device,
        conf: &WarparseConf,
    ) -> Result<OnlineStatus, ServiceError> {
        let status = self.fetch_status(device, conf).await?;

        let is_online = status.accepting_commands.unwrap_or(false);

        Ok(OnlineStatus {
            is_online,
            client_version: status.version,
            config_version: project_version_summary(status.project_version.as_ref()),
        })
    }

    /// 发起配置部署
    pub async fn deploy(
        &self,
        device: &Device,
        conf: &WarparseConf,
        target_version: &str,
        group: ReleaseGroup,
    ) -> Result<DeployResult, ServiceError> {
        let url = self.build_url(device, &conf.deploy_path)?;

        let body = ReloadRequest {
            wait: true,
            update: true,
            version: target_version,
            group: group.as_ref(),
            timeout_ms: 15000,
            reason: "wp-station deployment",
        };

        info!(
            "调用 WarpParse 部署 API: url={}, target_version={}",
            url, target_version
        );
        debug!(
            "部署请求参数: {:?}",
            serde_json::to_string(&body).unwrap_or_default()
        );

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", device.token))
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                warn!("部署 API 网络请求失败: {}", e);
                ServiceError::Network(e.to_string())
            })?;

        let status = resp.status();
        info!("部署 API 响应状态: {}", status);

        if !status.is_success() {
            // 读取响应体以获取详细错误信息
            let error_body = resp
                .text()
                .await
                .unwrap_or_else(|_| "无法读取响应体".to_string());

            let error_msg = format!("HTTP {} - {}", status, error_body);
            warn!("部署 API 失败: {}", error_msg);
            return Err(ServiceError::Response(error_msg));
        }

        let parsed: ReloadResponse = resp
            .json()
            .await
            .map_err(|e| ServiceError::Response(e.to_string()))?;

        info!(
            "部署 API 响应: accepted={:?}, request_id={:?}, message={:?}",
            parsed.accepted, parsed.request_id, parsed.message
        );

        Ok(DeployResult {
            accepted: parsed.accepted.unwrap_or(false),
            request_id: parsed.request_id,
            message: parsed.message,
        })
    }

    /// 检查部署是否成功
    /// 判断标准：last_reload_result == "reload_done" && project_version == target_version
    pub async fn check_deploy_success(
        &self,
        device: &Device,
        conf: &WarparseConf,
        target_version: &str,
        group: ReleaseGroup,
        expected_request_id: Option<&str>,
    ) -> Result<DeployCheckResult, ServiceError> {
        let status = self.fetch_status(device, conf).await?;
        let config_version = project_version_summary(status.project_version.as_ref());
        let current_version = project_version_for_group(status.project_version.as_ref(), group);

        info!(
            "检查部署状态: group={}, target_version={}, current_version={:?}, config_version={:?}, reload_result={:?}, reloading={:?}",
            group.as_ref(),
            target_version,
            current_version,
            config_version,
            status.last_reload_result,
            status.reloading
        );

        let normalized_target_version = normalize_version_for_compare(target_version);
        let normalized_current_version = current_version
            .as_deref()
            .map(normalize_version_for_compare);

        // 验证 request_id（如果提供）
        // 兼容部分设备状态接口不稳定回传 request_id：如果版本已切到目标版本，则允许继续走成功判定。
        if let Some(expected) = expected_request_id
            && status.last_reload_request_id.as_deref() != Some(expected)
            && normalized_current_version != Some(normalized_target_version)
        {
            warn!(
                "request_id 不匹配: expected={}, actual={:?}",
                expected, status.last_reload_request_id
            );
            return Ok(DeployCheckResult {
                is_success: false,
                current_version,
                config_version,
                is_reloading: status.reloading.unwrap_or(false),
            });
        }

        // 检查是否正在重载
        let is_reloading = status.reloading.unwrap_or(false);
        if is_reloading {
            info!("配置正在重载中");
            return Ok(DeployCheckResult {
                is_success: false,
                current_version,
                config_version,
                is_reloading: true,
            });
        }

        // 检查重载结果和版本。
        // 部分设备在成功后不一定稳定返回 reload_done，但版本切到目标版本即可认为发布已完成。
        let reload_done = matches!(
            status.last_reload_result.as_deref(),
            Some("reload_done") | Some("success")
        );
        let version_matched = normalized_current_version == Some(normalized_target_version);

        let is_success = version_matched && (reload_done || expected_request_id.is_some());

        if is_success {
            info!("部署成功验证通过");
        } else {
            warn!(
                "部署未成功: reload_done={}, version_matched={}, last_reload_result={:?}, request_id={:?}",
                reload_done,
                version_matched,
                status.last_reload_result,
                status.last_reload_request_id
            );
        }

        Ok(DeployCheckResult {
            is_success,
            current_version,
            config_version,
            is_reloading: false,
        })
    }

    /// 获取设备状态（内部方法）
    async fn fetch_status(
        &self,
        device: &Device,
        conf: &WarparseConf,
    ) -> Result<StatusResponse, ServiceError> {
        let url = self.build_url(device, &conf.status_path)?;

        debug!("调用 WarpParse 状态 API: url={}", url);

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", device.token))
            .send()
            .await
            .map_err(|e| ServiceError::Network(e.to_string()))?;

        let status = resp.status();
        debug!("状态 API 响应状态: {}", status);

        if !status.is_success() {
            let error_msg = format!("HTTP {}", status);
            warn!("状态 API 失败: {}", error_msg);
            return Err(ServiceError::Response(error_msg));
        }

        let status_resp: StatusResponse = resp
            .json()
            .await
            .map_err(|e| ServiceError::Response(e.to_string()))?;

        debug!(
            "状态 API 响应: project_version={:?}, current_request_id={:?}, accepting_commands={:?}, reloading={:?}, last_reload_result={:?}",
            project_version_summary(status_resp.project_version.as_ref()),
            status_resp.current_request_id,
            status_resp.accepting_commands,
            status_resp.reloading,
            status_resp.last_reload_result
        );

        Ok(status_resp)
    }

    /// 构建完整 URL
    fn build_url(&self, device: &Device, path: &str) -> Result<String, ServiceError> {
        let base = Self::device_endpoint(device)?;
        Ok(format!("{}{}", base, path))
    }

    fn device_endpoint(device: &Device) -> Result<String, ServiceError> {
        if device.ip.trim().is_empty() {
            return Err(ServiceError::InvalidState(
                "设备未配置 IP，无法连接".to_string(),
            ));
        }
        if device.port <= 0 {
            return Err(ServiceError::InvalidState(
                "设备未配置有效端口，无法连接".to_string(),
            ));
        }

        Ok(format!("http://{}:{}", device.ip.trim(), device.port))
    }
}

impl Default for WarpParseService {
    fn default() -> Self {
        Self::new().expect("创建 WarpParseService 失败")
    }
}
