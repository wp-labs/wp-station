// WarpParse 服务统一封装层 - 简化所有 WarpParse API 调用

use crate::db::Device;
use crate::server::setting::WarparseConf;
use reqwest::Client;
use serde::{Deserialize, Serialize};

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
    timeout_ms: u64,
    reason: &'a str,
}

#[derive(Deserialize)]
struct ReloadResponse {
    accepted: Option<bool>,
    message: Option<String>,
    request_id: Option<String>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct StatusResponse {
    version: Option<String>,
    project_version: Option<String>,
    accepting_commands: Option<bool>,
    reloading: Option<bool>,
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
    pub is_reloading: bool,
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
            config_version: status.project_version,
        })
    }

    /// 发起配置部署
    pub async fn deploy(
        &self,
        device: &Device,
        conf: &WarparseConf,
        target_version: &str,
    ) -> Result<DeployResult, ServiceError> {
        let url = self.build_url(device, conf, &conf.deploy_path);

        let body = ReloadRequest {
            wait: true,
            update: true,
            version: target_version,
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
        expected_request_id: Option<&str>,
    ) -> Result<DeployCheckResult, ServiceError> {
        let status = self.fetch_status(device, conf).await?;

        info!(
            "检查部署状态: target_version={}, current_version={:?}, reload_result={:?}, reloading={:?}",
            target_version, status.project_version, status.last_reload_result, status.reloading
        );

        // 验证 request_id（如果提供）
        if let Some(expected) = expected_request_id
            && status.last_reload_request_id.as_deref() != Some(expected)
        {
            warn!(
                "request_id 不匹配: expected={}, actual={:?}",
                expected, status.last_reload_request_id
            );
            return Ok(DeployCheckResult {
                is_success: false,
                current_version: status.project_version,
                is_reloading: status.reloading.unwrap_or(false),
            });
        }

        // 检查是否正在重载
        let is_reloading = status.reloading.unwrap_or(false);
        if is_reloading {
            info!("配置正在重载中");
            return Ok(DeployCheckResult {
                is_success: false,
                current_version: status.project_version,
                is_reloading: true,
            });
        }

        // 检查重载结果和版本
        let reload_done = status.last_reload_result.as_deref() == Some("reload_done");
        let version_matched = status
            .project_version
            .as_deref()
            .map(|v| v == target_version)
            .unwrap_or(false);

        let is_success = reload_done && version_matched;

        if is_success {
            info!("部署成功验证通过");
        } else {
            warn!(
                "部署未成功: reload_done={}, version_matched={}",
                reload_done, version_matched
            );
        }

        Ok(DeployCheckResult {
            is_success,
            current_version: status.project_version,
            is_reloading: false,
        })
    }

    /// 获取设备状态（内部方法）
    async fn fetch_status(
        &self,
        device: &Device,
        conf: &WarparseConf,
    ) -> Result<StatusResponse, ServiceError> {
        let url = self.build_url(device, conf, &conf.status_path);

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
            "状态 API 响应: project_version={:?}, accepting_commands={:?}, reloading={:?}, last_reload_result={:?}",
            status_resp.project_version,
            status_resp.accepting_commands,
            status_resp.reloading,
            status_resp.last_reload_result
        );

        Ok(status_resp)
    }

    /// 构建完整 URL
    fn build_url(&self, device: &Device, conf: &WarparseConf, path: &str) -> String {
        let base = if !device.ip.is_empty() && device.port > 0 {
            format!("http://{}:{}", device.ip, device.port)
        } else {
            conf.base_url.clone()
        };

        format!("{}{}", base, path)
    }
}

impl Default for WarpParseService {
    fn default() -> Self {
        Self::new().expect("创建 WarpParseService 失败")
    }
}
