use git2::Status;
use std::error::Error;
use std::fmt;

/// Gitea 客户端配置
#[derive(Clone, Debug)]
pub struct GiteaConfig {
    /// Gitea 服务器基础 URL（如 "http://gitea.example.com"）
    pub base_url: String,
    /// 用户名
    pub username: String,
    /// 密码或访问令牌
    pub password: String,
    /// 默认分支名（可选，默认 "main"）
    pub default_branch: Option<String>,
}

impl GiteaConfig {
    /// 使用基础字段创建配置，默认分支为空（后续可通过 with_branch 设置）
    pub fn new(base_url: String, username: String, password: String) -> Self {
        Self {
            base_url,
            username,
            password,
            default_branch: None,
        }
    }

    /// 设置默认分支名，返回新的配置
    pub fn with_branch(mut self, branch: String) -> Self {
        self.default_branch = Some(branch);
        self
    }

    pub fn default_branch(&self) -> String {
        self.default_branch
            .clone()
            .unwrap_or_else(|| DEFAULT_BRANCH.to_string())
    }
}

/// 仓库信息（远程仓库的简化视图）
#[derive(Clone, Debug)]
pub struct RepoInfo {
    pub name: String,
    pub clone_url: String,
    pub ssh_url: String,
    pub html_url: String,
}

impl RepoInfo {
    pub fn new(name: String, clone_url: String, ssh_url: String, html_url: String) -> Self {
        Self {
            name,
            clone_url,
            ssh_url,
            html_url,
        }
    }
}

/// 文件状态信息
#[derive(Debug, Clone)]
pub struct FileStatus {
    pub path: String,
    pub status: Status,
}

/// Git 操作错误类型
#[derive(Debug)]
pub enum GitError {
    Git2(git2::Error),
    Auth(String),
    Network(String),
    NotFound(String),
    InvalidPath(String),
    InvalidOperation(String),
    RepositoryNotInitialized,
    BranchNotFound(String),
}

impl From<git2::Error> for GitError {
    fn from(err: git2::Error) -> Self {
        GitError::Git2(err)
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            GitError::Git2(err) => write!(f, "Git错误: {}", err),
            GitError::Auth(msg) => write!(f, "认证错误: {}", msg),
            GitError::Network(msg) => write!(f, "网络错误: {}", msg),
            GitError::NotFound(msg) => write!(f, "未找到资源: {}", msg),
            GitError::InvalidPath(msg) => write!(f, "路径错误: {}", msg),
            GitError::InvalidOperation(msg) => write!(f, "无效操作: {}", msg),
            GitError::RepositoryNotInitialized => write!(f, "仓库未初始化"),
            GitError::BranchNotFound(name) => write!(f, "分支不存在: {}", name),
        }
    }
}

impl Error for GitError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            GitError::Git2(err) => Some(err),
            _ => None,
        }
    }
}

impl fmt::Display for FileStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status_str = if self.status.is_wt_new() {
            "未跟踪"
        } else if self.status.is_wt_modified() || self.status.is_index_modified() {
            "已修改"
        } else if self.status.is_wt_deleted() || self.status.is_index_deleted() {
            "已删除"
        } else if self.status.is_conflicted() {
            "冲突"
        } else if self.status.is_index_new() {
            "已添加"
        } else {
            "未变更"
        };

        write!(f, "{:10} {}", status_str, self.path)
    }
}

/// 单个文件的差异信息
#[derive(Debug, Clone)]
pub struct FileDiff {
    /// 文件路径
    pub file_path: String,
    /// 旧文件路径（仅在重命名时使用）
    pub old_path: Option<String>,
    /// 变更类型：add, delete, modify, rename
    pub change_type: String,
    /// 统一差异格式文本
    pub diff_text: String,
}

/// 差异结果，包含所有文件的差异信息和统计数据
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// 所有文件的差异列表
    pub files: Vec<FileDiff>,
    /// 变更的文件数
    pub files_changed: usize,
    /// 插入的行数
    pub insertions: usize,
    /// 删除的行数
    pub deletions: usize,
}

/// 默认分支名
pub const DEFAULT_BRANCH: &str = "main";
/// 默认远程名
pub const DEFAULT_ORIGIN: &str = "origin";
