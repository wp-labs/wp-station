use reqwest::Client;
use serde::Serialize;

/// AI 分析请求体
#[derive(Debug, Serialize)]
pub struct AiAnalyzeRequest {
    pub task_id: String,
    pub target_rule: String,
    pub log_data: String,
    pub current_rule: Option<String>,
    pub callback_url: String,
}

/// 人工工单请求体
#[derive(Debug, Serialize)]
pub struct ManualTicketRequest {
    pub task_id: String,
    pub target_rule: String,
    pub log_data: String,
    pub current_rule: Option<String>,
    pub extra_note: Option<String>,
    pub callback_url: String,
}

/// Assist 外部服务错误
#[derive(Debug)]
pub enum AssistServiceError {
    RequestBuild(String),
    RequestFailed(String),
    ResponseError {
        status: u16,
        body_preview: Option<String>,
    },
}

impl std::fmt::Display for AssistServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssistServiceError::RequestBuild(msg) => write!(f, "构建 HTTP 客户端失败: {}", msg),
            AssistServiceError::RequestFailed(msg) => write!(f, "请求外部服务失败: {}", msg),
            AssistServiceError::ResponseError {
                status,
                body_preview,
            } => {
                if let Some(preview) = body_preview {
                    write!(f, "外部服务响应错误: status={}, body={}", status, preview)
                } else {
                    write!(f, "外部服务响应错误: status={}", status)
                }
            }
        }
    }
}

impl std::error::Error for AssistServiceError {}

/// Assist 外部服务统一封装
pub struct AssistService {
    client: Client,
}

impl AssistService {
    pub fn new() -> Result<Self, AssistServiceError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|err| AssistServiceError::RequestBuild(err.to_string()))?;

        Ok(Self { client })
    }

    /// 提交 AI 分析任务
    pub async fn submit_ai_task(
        &self,
        base_url: &str,
        payload: &AiAnalyzeRequest,
    ) -> Result<(), AssistServiceError> {
        self.post_json(format!("{}/analyze", base_url), payload)
            .await
    }

    /// 提交人工工单
    pub async fn submit_manual_ticket(
        &self,
        base_url: &str,
        payload: &ManualTicketRequest,
    ) -> Result<(), AssistServiceError> {
        self.post_json(format!("{}/ticket", base_url), payload)
            .await
    }

    async fn post_json<T: Serialize>(
        &self,
        url: String,
        payload: &T,
    ) -> Result<(), AssistServiceError> {
        let response = self
            .client
            .post(url)
            .json(payload)
            .send()
            .await
            .map_err(|err| AssistServiceError::RequestFailed(err.to_string()))?;

        if response.status().is_success() {
            return Ok(());
        }

        let status = response.status().as_u16();
        let body_preview = response
            .text()
            .await
            .ok()
            .map(|body| truncate_body(&body, 200));

        Err(AssistServiceError::ResponseError {
            status,
            body_preview,
        })
    }
}

fn truncate_body(body: &str, limit: usize) -> String {
    let mut chars = body.chars();
    let preview: String = chars.by_ref().take(limit).collect();
    if chars.next().is_some() {
        format!("{}...", preview)
    } else {
        preview
    }
}
