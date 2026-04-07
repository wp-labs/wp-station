use std::path::Path;

use super::api::GiteaApiClient;
use crate::{types::GitError, DiffResultWithFiles};

use super::local_repo::LocalRepository;
pub use crate::types::{GiteaConfig, RepoInfo};

/// Gitea 客户端 - 管理远程仓库和本地仓库
pub struct GiteaClient {
    config: GiteaConfig,
    api_client: GiteaApiClient,
}

impl GiteaClient {
    /// 创建新的 Gitea 客户端，并缓存底层 GiteaApiClient
    pub fn new(config: GiteaConfig) -> Result<Self, GitError> {
        let api_client =
            GiteaApiClient::with_credentials(&config.base_url, &config.username, &config.password);

        Ok(Self { config, api_client })
    }

    pub fn config(&self) -> &GiteaConfig {
        &self.config
    }

    // ========== 远程仓库操作 ==========

    /// 在 Gitea 上创建远程仓库
    pub async fn create_repo(&self, repo_name: &str) -> Result<RepoInfo, GitError> {
        let resp = self
            .api_client
            .create_repo_with_credentials(repo_name)
            .await
            .map_err(|e| GitError::InvalidOperation(e.to_string()))?;

        Ok(RepoInfo::new(
            repo_name.to_string(),
            resp.clone_url,
            resp.ssh_url,
            resp.html_url,
        ))
    }

    /// 删除 Gitea 上的远程仓库
    pub async fn delete_repo(&self, repo_name: &str) -> Result<(), GitError> {
        self.api_client
            .delete_repo(repo_name)
            .await
            .map_err(|e| GitError::InvalidOperation(e.to_string()))
    }

    /// 在 Gitea 上创建远程标签（不依赖本地仓库）
    pub async fn create_remote_tag(
        &self,
        repo_name: &str,
        tag_name: &str,
        target: &str,
        message: Option<&str>,
    ) -> Result<(), GitError> {
        self.api_client
            .create_tag(repo_name, tag_name, target, message)
            .await
            .map_err(|e| GitError::InvalidOperation(e.to_string()))
    }

    // ========== 本地仓库操作 ==========

    /// 创建远程仓库并克隆到本地
    pub async fn create_and_clone<P: AsRef<Path>>(
        &self,
        repo_name: &str,
        local_path: P,
    ) -> Result<LocalRepository, GitError> {
        // 1. 创建远程仓库
        let repo_info = self.create_repo(repo_name).await?;

        // 2. 克隆到本地
        git2::Repository::clone(&repo_info.clone_url, local_path.as_ref())
            .map_err(|e| GitError::InvalidOperation(e.to_string()))?;

        // 3. 打开本地仓库
        LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())
    }

    /// 克隆已存在的远程仓库到本地
    pub async fn clone_existing<P: AsRef<Path>>(
        &self,
        clone_url: &str,
        local_path: P,
    ) -> Result<LocalRepository, GitError> {
        git2::Repository::clone(clone_url, local_path.as_ref())
            .map_err(|e| GitError::InvalidOperation(e.to_string()))?;

        LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())
    }

    /// 打开已存在的本地仓库
    pub fn open<P: AsRef<Path>>(&self, local_path: P) -> Result<LocalRepository, GitError> {
        LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())
    }

    /// add 以及 commit 以及push
    pub fn add_commit_push<P: AsRef<Path>>(
        &self,
        commit: &str,
        local_path: P,
    ) -> Result<(), GitError> {
        let local = LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())?;
        local.add_all()?;
        let _ = local.commit(commit)?;
        local.push()?;
        Ok(())
    }

    /// 与最新的tag做diff操作
    pub fn diff_with_newest_tag<P: AsRef<Path>>(
        &self,
        local_path: P,
    ) -> Result<DiffResultWithFiles, GitError> {
        let local = LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())?;
        local.diff_latest_tag_to_head()
    }

    /// 与前一个版本进行对比
    pub fn diff_with_previous_version<P: AsRef<Path>>(
        &self,
        local_path: P,
        curr_version: &str
    ) -> Result<DiffResultWithFiles, GitError> {
        let local = LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())?;
        local.diff_with_previous_version(curr_version)
    }

    pub fn create_push_tag<P: AsRef<Path>>(
        &self,
        local_path: P,
        version: &str,
    ) -> Result<(), GitError> {
        let local = LocalRepository::new(self.config.clone(), local_path.as_ref().to_path_buf())?;
        local.create_tag(version)?;
        local.push_tag(version)
    }
}
