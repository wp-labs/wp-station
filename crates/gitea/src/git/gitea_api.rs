use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};


/// Gitea创建仓库请求结构体
#[derive(Serialize)]
pub struct CreateRepoRequest {
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
    pub auto_init: bool,
}

/// Gitea 创建 git tag 请求结构体
#[derive(Serialize)]
pub struct CreateTagRequest {
    pub tag_name: String,
    pub target: String,
    pub message: Option<String>,
}
pub struct GiteaApiClient {
    pub base_url: String,
    pub user_name: String,
    pub password: String,
}

#[derive(Deserialize, Debug)]
pub struct CreateRepoResponse {
    pub clone_url: String,
    pub ssh_url: String,
    pub html_url: String,
}

impl GiteaApiClient {

    /// 创建带用户名和密码的Gitea API客户端实例
    pub fn with_credentials(base_url: &str, username: &str, password: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            user_name: username.to_string(),
            password: password.to_string(),
        }
    }
    
    /// 在指定仓库中基于 target 创建一个新的 git tag
    ///
    /// `target` 可以是分支名（如 "master"）、提交 SHA 等，Gitea 会解析为目标提交。
    pub async fn create_tag(
        &self,
        repo_name: &str,
        tag_name: &str,
        target: &str,
        message: Option<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 确保用户名和密码已提供
        if self.user_name.is_empty() || self.password.is_empty() {
            return Err("必须提供用户名和密码".into());
        }

        let client = Client::new();

        let req = CreateTagRequest {
            tag_name: tag_name.to_string(),
            target: target.to_string(),
            message: message.map(|m| m.to_string()),
        };

        // POST /api/v1/repos/{owner}/{repo}/tags
        let url = format!(
            "{}/api/v1/repos/{}/{}{}",
            self.base_url,
            self.user_name,
            repo_name,
            "/tags",
        );

        let response = client
            .post(url)
            .basic_auth(&self.user_name, Some(&self.password))
            .json(&req)
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await?;
            Err(format!("创建远程标签失败: {} - {}", status, text).into())
        }
    }

    /// 专门用于用户名密码认证的仓库创建方法
    pub async fn create_repo_with_credentials(&self, repo_name: &str) -> Result<CreateRepoResponse, Box<dyn std::error::Error>> {
        // 确保用户名和密码已提供
        if self.user_name.is_empty() || self.password.is_empty() {
            return Err("必须提供用户名和密码".into());
        }
        
        // 创建HTTP客户端
        let client = Client::new();
        
        // 准备最小化请求数据
        let req = CreateRepoRequest {
            name: repo_name.to_string(),
            description: Some("通过Rust程序自动创建的仓库".to_string()),
            private: false,
            auto_init: false,
        };
        
        // 构建请求并设置基本认证
        let request = client
            .post(format!("{}/api/v1/user/repos", self.base_url))
            .basic_auth(&self.user_name, Some(&self.password))
            .json(&req);
        
        // 发送请求
        let response = request.send().await?;
        
        // 检查响应状态
        if response.status().is_success() {
            // 解析响应
            let repo_info: CreateRepoResponse = response.json().await?;
            
            // 返回HTTPS URL，用于用户名密码认证的Git操作
            Ok(repo_info)
        } else {
            let status = response.status();
            let text = response.text().await?;
            Err(format!("创建远程仓库失败: {} - {}", status, text).into())
        }
    }

    /// 删除指定名称的仓库（使用当前用户作为所有者）
    pub async fn delete_repo(&self, repo_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 确保用户名和密码已提供
        if self.user_name.is_empty() || self.password.is_empty() {
            return Err("必须提供用户名和密码".into());
        }

        let client = Client::new();
        let url = format!("{}/api/v1/repos/{}/{}", self.base_url, self.user_name, repo_name);

        let response = client
            .delete(url)
            .basic_auth(&self.user_name, Some(&self.password))
            .send()
            .await?;

        if response.status().is_success() {
            Ok(())
        } else if response.status() == StatusCode::NOT_FOUND {
            // 仓库不存在视为成功，避免影响上层删除逻辑
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await?;
            Err(format!("删除远程仓库失败: {} - {}", status, text).into())
        }
    }
}
