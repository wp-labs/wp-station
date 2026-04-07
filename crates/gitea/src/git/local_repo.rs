use std::path::PathBuf;

use serde::Serialize;

use crate::types::{FileStatus, GitError, GiteaConfig};
use crate::repository::GitRepository;

/// 本地 Git 仓库操作对象
///
/// 内部复用现有的 GitRepository，实现 add_all/commit/push 等本地操作。
pub struct LocalRepository {
    repo: GitRepository,
    local_path: PathBuf,
}

impl LocalRepository {
    /// 内部使用的构造函数，根据配置和本地路径打开仓库
    pub(crate) fn new(config: GiteaConfig, local_path: PathBuf) -> Result<Self, GitError> {
        let repo = GitRepository::new_with_username_password(
            Some(config.default_branch()),
            local_path.clone(),
            config.username.clone(),
            config.password.clone(),
        )?;

        Ok(Self { repo, local_path })
    }

    pub fn add_all(&self) -> Result<(), GitError> {
        self.repo.add_all()
    }

    pub fn add(&self, path: &std::path::Path) -> Result<(), GitError> {
        self.repo.add(path)
    }

    /// 提交更改，返回提交 OID 字符串
    pub fn commit(&self, message: &str) -> Result<String, GitError> {
        let oid = self.repo.commit(message)?;
        Ok(oid.to_string())
    }

    pub fn push(&self) -> Result<(), GitError> {
        self.repo.push()
    }

    pub fn pull(&self) -> Result<(), GitError> {
        self.repo.pull()
    }

    pub fn force_push(&self) -> Result<(), GitError> {
        self.repo.force_push()
    }

    /// 查看暂存区与 HEAD 的差异（已暂存的更改）
    pub fn diff_index_to_head(&self) -> Result<DiffResult, GitError> {
        let content = self.repo.diff_index_to_head()?;
        let (files_changed, insertions, deletions) = self.repo.diff_stats()?;
        Ok(DiffResult {
            content,
            stats: DiffStats {
                files_changed,
                insertions,
                deletions,
            },
        })
    }

    /// 查看两个提交之间的差异
    pub fn diff_commits(
        &self,
        old_commit_id: &str,
        new_commit_id: &str,
    ) -> Result<DiffResult, GitError> {
        let content = self.repo.diff_commits(old_commit_id, new_commit_id)?;
        let (files_changed, insertions, deletions) = self.repo.diff_stats()?;
        Ok(DiffResult {
            content,
            stats: DiffStats {
                files_changed,
                insertions,
                deletions,
            },
        })
    }

    /// 查看最新标签与 HEAD 的差异
    pub fn diff_latest_tag_to_head(&self) -> Result<DiffResultWithFiles, GitError> {
        let diff_result = self.repo.diff_tag_with_head_and_stats()?;        
        Ok(diff_result)
    }

    /// 查看当前版本与前一个版本的差异
    ///
    /// # 参数
    /// * `curr_version` - 当前版本标签名称
    ///
    /// # 返回值
    /// * `Ok(DiffResultWithFiles)` - 返回差异结果
    /// * `Err(GitError)` - 获取差异失败时返回错误
    pub fn diff_with_previous_version(&self, curr_version: &str) -> Result<DiffResultWithFiles, GitError> {
        let diff_result = self.repo.diff_with_previous_version(curr_version)?;
        Ok(diff_result)
    }

    /// 获取 diff 统计信息 (文件数, 插入行数, 删除行数)
    pub fn diff_stats(&self) -> Result<DiffStats, GitError> {
        let (files_changed, insertions, deletions) = self.repo.diff_stats()?;
        Ok(DiffStats {
            files_changed,
            insertions,
            deletions,
        })
    }

    pub fn status(&self) -> Result<Vec<FileStatus>, GitError> {
        self.repo.status()
    }

    pub fn create_tag(&self, name: &str) -> Result<(), GitError> {
        self.repo.create_tag(name)
    }

    pub fn get_latest_tag(&self) -> Result<String, GitError> {
        self.repo.get_latest_tag()
    }

    pub fn list_tags(&self) -> Result<Vec<String>, GitError> {
        self.repo.list_tags()
    }

    pub fn delete_tag(&self, tag_name: &str) -> Result<(), GitError> {
        self.repo.delete_tag(tag_name)
    }

    pub fn push_tag(&self, tag_name: &str) -> Result<(), GitError> {
        self.repo.push_tag(tag_name)
    }

    /// 推送所有标签到远程
    pub fn push_tags(&self) -> Result<(), GitError> {
        self.repo.push_all_tags()
    }

    pub fn delete_remote_tag(&self, tag_name: &str) -> Result<(), GitError> {
        self.repo.delete_remote_tag(tag_name)
    }

    /// 创建新分支（基于当前 HEAD 提交），不暴露 git2 底层类型
    pub fn create_branch(&self, branch_name: &str) -> Result<(), GitError> {
        let head = self.repo.raw_repo().head()?;
        let commit = head.peel_to_commit()?;
        self.repo.create_branch(branch_name, &commit)?;
        Ok(())
    }

    pub fn list_branches(&self) -> Result<Vec<String>, GitError> {
        self.repo.list_branches()
    }

    pub fn get_history(&self, limit: usize) -> Result<Vec<git2::Commit<'_>>, GitError> {
        self.repo.get_history(limit)
    }

    /// 获取最新一次提交的信息
    pub fn latest_commit(&self) -> Result<CommitInfo, GitError> {
        let head = self.repo.raw_repo().head()?;
        let commit = head.peel_to_commit()?;
        let author_sig = commit.author();
        let author = author_sig.name().unwrap_or("Unknown").to_string();
        let email = author_sig.email().unwrap_or("").to_string();
        let message = commit.message().unwrap_or("").to_string();
        let timestamp = commit.time().seconds();

        Ok(CommitInfo {
            oid: commit.id().to_string(),
            author,
            email,
            message,
            timestamp,
        })
    }

    /// 获取提交历史（返回简化的 CommitInfo 列表）
    pub fn history(&self, limit: usize) -> Result<Vec<CommitInfo>, GitError> {
        let commits = self.repo.get_history(limit)?;
        let mut result = Vec::with_capacity(commits.len());

        for commit in commits {
            result.push(CommitInfo {
                oid: commit.id().to_string(),
                author: commit
                    .author()
                    .name()
                    .unwrap_or("Unknown")
                    .to_string(),
                email: commit
                    .author()
                    .email()
                    .unwrap_or("")
                    .to_string(),
                message: commit.message().unwrap_or("").to_string(),
                timestamp: commit.time().seconds(),
            });
        }

        Ok(result)
    }

    pub fn checkout(&self, treeish: &str) -> Result<(), GitError> {
        self.repo.checkout(treeish)
    }

    pub fn raw_repo(&self) -> &GitRepository {
        &self.repo
    }

    pub fn local_path(&self) -> &PathBuf {
        &self.local_path
    }
}

/// 提交信息（简化返回，而不是 git2::Commit）
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,
    pub author: String,
    pub email: String,
    pub message: String,
    pub timestamp: i64,
}

/// Diff 统计信息（封装已有的 (usize, usize, usize) 返回）
#[derive(Debug, Clone, Serialize)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
}

/// Diff 结果（包含原始 diff 文本和统计信息）
#[derive(Debug, Clone, Serialize)]
pub struct DiffResult {
    pub content: String,
    pub stats: DiffStats,
}

impl DiffResult {
    /// 以字符串形式获取原始 diff 内容
    pub fn as_str(&self) -> &str {
        &self.content
    }
}

/// Diff 结果（包含文件列表和统计信息）- 用于结构化差异数据
#[derive(Debug, Clone, Serialize)]
pub struct DiffResultWithFiles {
    pub files: Vec<FileDiffInfo>,
    pub stats: DiffStats,
}

/// 单个文件的差异信息（用于序列化）
#[derive(Debug, Clone, Serialize)]
pub struct FileDiffInfo {
    pub file_path: String,
    pub old_path: Option<String>,
    pub change_type: String,
    pub diff_text: String,
}
