//! gitea - Git操作库
//!
//! 一个封装了git2库的高级Git操作接口，提供简洁易用的API进行Git仓库管理。

#[path = "git/gitea_api.rs"]
mod api;
#[path = "git/client.rs"]
pub mod client;
#[path = "git/local_repo.rs"]
pub mod local_repo;
#[path = "git/repository.rs"]
mod repository;
#[path = "git/types.rs"]
pub mod types;
#[path = "git/util.rs"]
mod utils;

// 重新导出核心类型（仅新接口）
pub use client::GiteaClient;
pub use local_repo::{
    CommitInfo, DiffResult, DiffResultWithFiles, DiffStats, FileDiffInfo, LocalRepository,
};
pub use types::{FileStatus, GitError, GiteaConfig, RepoInfo};
