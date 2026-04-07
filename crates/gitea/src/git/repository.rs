use crate::types::{FileStatus, GitError};
use crate::types::{DEFAULT_BRANCH, DEFAULT_ORIGIN};
use crate::utils::{
    configure_ssh_callbacks_with_key, configure_ssh_callbacks_with_password, ensure_ssh_url,
};
use crate::{DiffResultWithFiles, DiffStats, FileDiffInfo};
use git2::{Commit, ObjectType, Oid, Repository, Signature, Sort, StatusOptions};
use std::path::{Path, PathBuf};

enum AuthConfig {
    UserPassword {
        username: String,
        password: String,
    },
    DeployKey {
        private_key: String,
        passphrase: Option<String>,
    },
}

/// Git仓库操作的主结构体，封装了git2库的核心功能
///
/// 此结构体仅作为内部实现细节使用，外部推荐通过 LocalRepository 进行操作。
#[doc(hidden)]
pub struct GitRepository {
    pub repo: Repository,
    pub branch: String,
    pub local_path: PathBuf,
    auth: AuthConfig,
    author_name: String,
    author_email: String,
}

impl GitRepository {
    /// 创建一个新的GitRepository实例，使用用户名密码认证
    ///
    /// # 参数
    /// * `branch` - 分支名
    /// * `local_path` - 本地仓库路径
    /// * `user_name` - 用户名
    /// * `password` - 密码
    ///
    /// # 返回值
    /// * `Ok(Self)` - 成功初始化仓库时返回GitRepository实例
    /// * `Err(GitError)` - 初始化失败时返回错误
    pub fn new_with_username_password(
        branch: Option<String>,
        local_path: PathBuf,
        user_name: String,
        password: String,
    ) -> Result<Self, GitError> {
        let repo = Repository::open(local_path.as_path())
            .map_err(|e| GitError::InvalidOperation(e.to_string()))?;
        if user_name.is_empty() || password.is_empty() {
            return Err(GitError::Auth("用户名或密码不能为空".to_string()));
        }
        let author_name = user_name.clone();
        let author_email = format!("{}@localhost", user_name);
        Ok(Self {
            repo,
            branch: branch.unwrap_or(DEFAULT_BRANCH.to_string()),
            local_path,
            auth: AuthConfig::UserPassword {
                username: user_name,
                password,
            },
            author_name,
            author_email,
        })
    }

    /// 创建一个新的GitRepository实例
    ///
    /// # 参数
    /// * `branch` - 分支名
    /// * `local_path` - 本地仓库路径
    /// * `user_name` - 用户名（可选，用于设置 Git 作者信息）
    /// * `deploy_key` - 部署密钥
    ///
    /// # 返回值
    /// * `Ok(Self)` - 成功初始化仓库时返回GitRepository实例
    /// * `Err(GitError)` - 初始化失败时返回错误
    pub fn new_with_deploykey(
        branch: Option<String>,
        local_path: PathBuf,
        user_name: Option<String>,
        deploy_key: String,
    ) -> Result<Self, GitError> {
        // 确认远程地址为 SSH 方案，否则 deploy key 无法认证。
        if deploy_key.is_empty() {
            return Err(GitError::Auth("部署密钥不能为空".to_string()));
        }

        let repo = Repository::open(local_path.as_path())
            .map_err(|e| GitError::InvalidOperation(e.to_string()))?;
        let (author_name, author_email) = match &user_name {
            Some(name) => (name.clone(), format!("{}@localhost", name)),
            None => ("git".to_string(), "git@localhost".to_string()),
        };
        Ok(Self {
            repo,
            branch: branch.unwrap_or(DEFAULT_BRANCH.to_string()),
            local_path,
            auth: AuthConfig::DeployKey {
                private_key: deploy_key,
                passphrase: None,
            },
            author_name,
            author_email,
        })
    }

    /// 获取底层git2仓库引用
    ///
    /// # 返回值
    /// 返回git2::Repository类型的不可变引用，用于高级操作
    pub fn raw_repo(&self) -> &Repository {
        &self.repo
    }
}

impl GitRepository {
    /// 添加远程仓库
    /// 目前只支持 SSH 地址
    /// # 参数
    /// * `remote_name` - 远程仓库名称（如 "origin"）
    /// * `url` - 远程仓库URL
    ///
    /// # 返回值
    /// * `Ok(())` - 成功添加远程仓库
    /// * `Err(GitError)` - 添加失败时返回错误
    pub fn add_remote(&self, remote_name: &str, url: &str) -> Result<(), GitError> {
        // 验证URL是否为SSH格式
        ensure_ssh_url(url)?;
        self.raw_repo().remote(remote_name, url)?;
        Ok(())
    }

    /// 添加文件到暂存区
    ///
    /// # 参数
    /// * `path` - 要添加到暂存区的文件路径
    ///
    /// # 返回值
    /// * `Ok(())` - 成功添加文件到暂存区
    /// * `Err(GitError)` - 添加失败时返回错误
    pub fn add(&self, path: &Path) -> Result<(), GitError> {
        // 获取索引并添加文件
        let mut index = self.raw_repo().index()?;

        index.add_path(path)?;

        index.write()?;
        Ok(())
    }

    /// 添加所有未跟踪和修改的文件到暂存区（相当于git add .）
    ///
    /// # 返回值
    /// * `Ok(())` - 成功添加所有文件到暂存区
    /// * `Err(GitError)` - 添加失败时返回错误
    pub fn add_all(&self) -> Result<(), GitError> {
        // 获取索引
        let mut index = self.raw_repo().index()?;

        // 使用git add .模式添加所有文件
        // 空字符串表示当前目录（.）
        index.add_all(&["*"], git2::IndexAddOption::DEFAULT, None)?;

        // 写入索引
        index.write()?;
        Ok(())
    }

    /// 提交更改
    ///
    /// # 参数
    /// * `message` - 提交信息
    ///
    /// # 返回值
    /// * `Ok(Oid)` - 成功提交时返回新提交的OID
    /// * `Err(GitError)` - 提交失败时返回错误
    pub fn commit(&self, message: &str) -> Result<Oid, GitError> {
        // 获取HEAD引用
        let head = self.raw_repo().head()?;
        // 获取当前分支的引用
        let tree_id = self.raw_repo().index()?.write_tree()?;
        let tree = self.raw_repo().find_tree(tree_id)?;

        // 确定parent commits
        let parents = if head.name().is_some() {
            vec![head.peel_to_commit()?]
        } else {
            Vec::new()
        };

        let signature = self.signature()?;

        // 执行提交
        self.raw_repo().commit(
            Some("HEAD"),                        // 引用名称
            &signature,                          // 作者
            &signature,                          // 提交者
            message,                             // 提交信息
            &tree,                               // 树对象
            &parents.iter().collect::<Vec<_>>(), // 父提交
        )?;

        // 重新获取刚刚创建的提交 OID
        Ok(self.raw_repo().head()?.peel_to_commit()?.id())
    }

    /// 从远程仓库拉取更新
    /// 默认拉取 DEFAULT_BRANCH 分支
    ///
    /// # 返回值
    /// * `Ok(())` - 成功拉取
    /// * `Err(GitError)` - 拉取失败时返回错误
    pub fn pull(&self) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        // 配置认证回调
        let callbacks = self.configure_remote_callbacks()?;

        let mut fetch_options = git2::FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        // 拉取远程分支
        remote.fetch(&[DEFAULT_BRANCH], Some(&mut fetch_options), None)?;

        // 获取 FETCH_HEAD
        let fetch_head = self.raw_repo().find_reference("FETCH_HEAD")?;
        let fetch_commit = self.raw_repo().reference_to_annotated_commit(&fetch_head)?;

        // 执行合并分析
        let (merge_analysis, _) = self.raw_repo().merge_analysis(&[&fetch_commit])?;

        // 处理合并分析结果
        match merge_analysis {
            // 快进合并
            analysis if analysis.is_fast_forward() => {
                let refname = format!("refs/heads/{}", DEFAULT_BRANCH);
                let mut reference = self.raw_repo().find_reference(&refname)?;
                reference.set_target(fetch_commit.id(), "Fast-Forward")?;
                self.raw_repo().set_head(&refname)?;
                self.raw_repo()
                    .checkout_head(Some(git2::build::CheckoutBuilder::default().force()))?;
            }
            // 需要正常合并
            analysis if analysis.is_normal() => {
                return Err(GitError::InvalidOperation("需要手动合并".to_string()));
            }
            // 其他情况（如已经是最新的）无需特殊处理
            _ => {}
        }

        Ok(())
    }

    /// 推送到远程仓库
    ///
    /// # 参数
    /// * `remote_name` - 远程仓库名称（如 "origin"）
    /// * `branch` - 要推送的分支名称
    ///
    /// # 返回值
    /// * `Ok(())` - 成功推送
    /// * `Err(GitError)` - 推送失败时返回错误
    pub fn push(&self) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        let callbacks = self.configure_remote_callbacks()?;

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // 推送到远程分支
        let refspec = format!(
            "refs/heads/{}:refs/heads/{}",
            DEFAULT_BRANCH, DEFAULT_BRANCH
        );
        remote.push(&[&refspec], Some(&mut push_options))?;

        Ok(())
    }

    /// 强制推送到远程仓库
    ///
    /// 警告：强制推送会覆盖远程分支的历史记录，可能导致团队协作问题。
    /// 请谨慎使用此方法，确保您知道自己在做什么。
    ///
    /// # 参数
    /// * 无参数，使用默认的远程仓库和分支
    ///
    /// # 返回值
    /// * `Ok(())` - 成功强制推送
    /// * `Err(GitError)` - 推送失败时返回错误
    pub fn force_push(&self) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        let callbacks = self.configure_remote_callbacks()?;

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // 强制推送：在refspec前添加'+'符号
        let refspec = format!(
            "+refs/heads/{}:refs/heads/{}",
            DEFAULT_BRANCH, DEFAULT_BRANCH
        );
        remote.push(&[&refspec], Some(&mut push_options))?;

        Ok(())
    }

    /// 获取工作目录的状态
    ///
    /// # 返回值
    /// * `Ok(Vec<FileStatus>)` - 成功获取状态时返回文件状态列表
    /// * `Err(GitError)` - 获取状态失败时返回错误
    pub fn status(&self) -> Result<Vec<FileStatus>, GitError> {
        let mut opts = StatusOptions::new();
        opts.show(git2::StatusShow::IndexAndWorkdir);

        let statuses = self.raw_repo().statuses(Some(&mut opts))?;

        let mut result = Vec::new();
        for entry in statuses.iter() {
            if let Some(path) = entry.path() {
                result.push(FileStatus {
                    path: path.to_string(),
                    status: entry.status(),
                });
            }
        }

        Ok(result)
    }

    /// 配置远程操作的认证回调
    ///
    /// # 返回值
    /// * `Ok(RemoteCallbacks)` - 成功配置的认证回调
    /// * `Err(GitError)` - 配置失败时返回错误
    pub fn configure_remote_callbacks(&self) -> Result<git2::RemoteCallbacks<'_>, GitError> {
        let mut callbacks = git2::RemoteCallbacks::new();

        match &self.auth {
            AuthConfig::DeployKey {
                private_key,
                passphrase,
            } => {
                configure_ssh_callbacks_with_key(
                    &mut callbacks,
                    private_key.clone(),
                    passphrase.as_deref(),
                );
                Ok(callbacks)
            }
            AuthConfig::UserPassword { username, password } => {
                if username.is_empty() || password.is_empty() {
                    return Err(GitError::InvalidOperation(
                        "用户名或密码不能为空".to_string(),
                    ));
                }
                configure_ssh_callbacks_with_password(
                    &mut callbacks,
                    username.clone(),
                    password.clone(),
                );
                Ok(callbacks)
            }
        }
    }

    /// 创建签名
    ///
    /// # 参数
    /// * `name` - 用户名
    /// * `email` - 邮箱地址
    ///
    /// # 返回值
    /// * `Ok(Signature)` - 成功创建签名时返回签名对象
    /// * `Err(GitError)` - 创建签名失败时返回错误
    pub fn signature(&self) -> Result<Signature<'_>, GitError> {
        // 从系统获取签名信息
        let default_name = "git"; // 默认用户名
        let default_email = "git@localhost"; // 默认邮箱

        let name = if self.author_name.is_empty() {
            default_name
        } else {
            self.author_name.as_str()
        };
        let email = if self.author_email.is_empty() {
            default_email
        } else {
            self.author_email.as_str()
        };

        Signature::now(name, email).map_err(|e| e.into())
    }
}

impl GitRepository {
    /// 创建新分支
    ///
    /// # 参数
    /// * `branch_name` - 新分支名称
    /// * `commit` - 分支将基于的提交
    ///
    /// # 返回值
    /// * `Ok(Branch)` - 成功创建分支时返回分支对象
    /// * `Err(GitError)` - 创建分支失败时返回错误
    pub fn create_branch(
        &self,
        branch_name: &str,
        commit: &Commit,
    ) -> Result<git2::Branch<'_>, GitError> {
        self.raw_repo()
            .branch(branch_name, commit, false)
            .map_err(|e| e.into())
    }

    /// 切换到指定分支
    ///
    /// # 参数
    /// * `branch_name` - 要切换到的分支名称
    ///
    /// # 返回值
    /// * `Ok(())` - 成功切换分支
    /// * `Err(GitError)` - 切换分支失败时返回错误，包括分支不存在的情况
    pub fn checkout_branch(&self, branch_name: &str) -> Result<(), GitError> {
        let branch = match self
            .raw_repo()
            .find_branch(branch_name, git2::BranchType::Local)
        {
            Ok(branch) => branch,
            Err(_) => return Err(GitError::BranchNotFound(branch_name.to_string())),
        };

        let target = branch.get();
        let object = target.peel(ObjectType::Commit)?;

        // 检出分支
        let mut opts = git2::build::CheckoutBuilder::new();
        opts.force();
        self.raw_repo().checkout_tree(&object, Some(&mut opts))?;
        self.raw_repo()
            .set_head(&format!("refs/heads/{}", branch_name))?;

        Ok(())
    }

    /// 列出所有本地分支
    ///
    /// # 返回值
    /// * `Ok(Vec<String>)` - 成功获取分支列表时返回分支名称列表
    /// * `Err(GitError)` - 获取分支列表失败时返回错误
    pub fn list_branches(&self) -> Result<Vec<String>, GitError> {
        let mut branches = Vec::new();
        let branch_iter = self.raw_repo().branches(Some(git2::BranchType::Local))?;

        for branch_result in branch_iter {
            let (branch, _) = branch_result?;
            if let Some(name) = branch.name()? {
                branches.push(name.to_string());
            }
        }

        Ok(branches)
    }

    /// 获取提交历史
    ///
    /// # 参数
    /// * `limit` - 要获取的提交数量限制
    ///
    /// # 返回值
    /// * `Ok(Vec<Commit>)` - 成功获取提交历史时返回提交对象列表
    /// * `Err(GitError)` - 获取提交历史失败时返回错误
    pub fn get_history(&self, limit: usize) -> Result<Vec<Commit<'_>>, GitError> {
        // 处理HEAD未设置的情况
        let head_commit = match self.raw_repo().head() {
            Ok(head) => head.peel_to_commit()?,
            Err(_) => {
                return Err(GitError::InvalidOperation(
                    "HEAD引用未设置，无法获取历史".to_string(),
                ))
            }
        };

        let mut revwalk = self.raw_repo().revwalk()?;
        revwalk.push(head_commit.id())?;
        revwalk.set_sorting(Sort::TIME | Sort::REVERSE)?;

        let mut commits = Vec::new();
        for (i, oid_result) in revwalk.enumerate() {
            if i >= limit {
                break;
            }

            let oid = oid_result?;
            let commit = self.raw_repo().find_commit(oid)?;
            commits.push(commit);
        }

        Ok(commits)
    }

    /// 检出指定提交或文件
    ///
    /// # 参数
    /// * `treeish` - 可以是提交ID、分支名、标签名或路径
    ///
    /// # 返回值
    /// * `Ok(())` - 成功检出
    /// * `Err(GitError)` - 检出失败时返回错误
    pub fn checkout(&self, treeish: &str) -> Result<(), GitError> {
        let object = self.raw_repo().revparse_single(treeish)?;
        let mut opts = git2::build::CheckoutBuilder::new();
        opts.force();
        self.raw_repo().checkout_tree(&object, Some(&mut opts))?;

        Ok(())
    }
}

impl GitRepository {
    /// 基于当前 HEAD 创建标签
    ///
    /// # 参数
    /// * `name` - 标签名称
    ///
    /// # 返回值
    /// * `Ok(())` - 成功创建标签
    /// * `Err(GitError)` - 创建标签失败时返回错误
    pub fn create_tag(&self, name: &str) -> Result<(), GitError> {
        // 获取当前分支的最新提交
        let head_commit = match self.raw_repo().head() {
            Ok(head) => head.peel_to_commit()?,
            Err(_) => {
                return Err(GitError::InvalidOperation(
                    "HEAD引用未设置，无法创建标签".to_string(),
                ))
            }
        };

        // 使用获取到的提交创建带注解的标签，并在 refs/tags/<name> 下创建引用
        // 注意：必须使用 Repository::tag，而不是仅创建注解对象，否则不会生成可见的标签引用。
        let sig = head_commit.author();
        let msg = format!("Tag {}", name);
        let obj = head_commit.as_object();
        self.raw_repo().tag(name, obj, &sig, &msg, false)?;
        Ok(())
    }

    /// 获取最新标签
    ///
    /// # 返回值
    /// * `Ok(String)` - 成功获取最新标签时返回标签名称
    /// * `Err(GitError)` - 获取最新标签失败时返回错误
    pub fn get_latest_tag(&self) -> Result<String, GitError> {
        // 获取所有标签名
        let tag_names = self.raw_repo().tag_names(None)?;
        let repo = self.raw_repo();

        // 如果没有标签，返回错误
        if tag_names.is_empty() {
            return Err(GitError::InvalidOperation("没有找到任何标签".to_string()));
        }

        // 收集所有标签及其时间，使用 Vec 避免多次查询
        let mut tags_with_time: Vec<(String, i64)> = Vec::with_capacity(tag_names.len());

        for tag_name in tag_names.iter().flatten() {
            // 使用 revparse_single 一次性获取标签引用
            if let Ok(tag_ref) = repo.revparse_single(&format!("refs/tags/{}", tag_name)) {
                if let Ok(commit) = tag_ref.peel_to_commit() {
                    tags_with_time.push((tag_name.to_string(), commit.time().seconds()));
                }
            }
        }

        // 如果没有有效的标签，返回错误
        if tags_with_time.is_empty() {
            return Err(GitError::InvalidOperation("没有找到有效的标签".to_string()));
        }

        // 使用 max_by_key 找到时间最大的标签（性能更好）
        tags_with_time
            .into_iter()
            .max_by_key(|(_, time)| *time)
            .map(|(name, _)| name)
            .ok_or_else(|| GitError::InvalidOperation("无法确定最新标签".to_string()))
    }

    /// 获取指定版本之前的最新标签
    ///
    /// # 参数
    /// * `curr_version` - 当前版本标签名称
    ///
    /// # 返回值
    /// * `Ok(String)` - 成功获取前一个标签时返回标签名称
    /// * `Err(GitError)` - 获取前一个标签失败时返回错误
    pub fn get_previous_tag(&self, curr_version: &str) -> Result<String, GitError> {
        let tag_names = self.raw_repo().tag_names(None)?;
        let repo = self.raw_repo();

        if tag_names.is_empty() {
            return Err(GitError::InvalidOperation("没有找到任何标签".to_string()));
        }

        // 首先获取当前版本的时间
        let curr_tag_ref = repo
            .revparse_single(&format!("refs/tags/{}", curr_version))
            .map_err(|_| GitError::InvalidOperation(format!("找不到标签: {}", curr_version)))?;
        let curr_commit = curr_tag_ref.peel_to_commit()?;
        let curr_time = curr_commit.time().seconds();
        let mut previous_time = 0i64;
        let mut previous_tag = "";

        for tag_name in tag_names.iter().flatten() {
            if tag_name == curr_version {
                continue;
            }
            if let Ok(tag_ref) = repo.revparse_single(&format!("refs/tags/{}", tag_name)) {
                if let Ok(commit) = tag_ref.peel_to_commit() {
                    let tag_time = commit.time().seconds();
                    // 记录在当前版本之前的最新标签
                    if tag_time <= curr_time && tag_time >= previous_time {
                        previous_tag = tag_name;
                        previous_time = tag_time;
                    }
                }
            }
        }

        // 如果没有更早的标签，返回错误
        if previous_tag.is_empty() {
            return Err(GitError::InvalidOperation(format!(
                "没有找到早于 {} 的标签",
                curr_version
            )));
        }

        Ok(previous_tag.to_string())
    }

    /// 列出所有标签
    ///
    /// # 返回值
    /// * `Ok(Vec<String>)` - 成功获取标签列表时返回标签名称列表
    /// * `Err(GitError)` - 获取标签列表失败时返回错误
    pub fn list_tags(&self) -> Result<Vec<String>, GitError> {
        let tag_names = self.raw_repo().tag_names(None)?;
        let mut tags = Vec::new();

        for tag_name in tag_names.iter() {
            if let Some(name) = tag_name {
                tags.push(name.to_string());
            }
        }

        Ok(tags)
    }

    /// 删除本地标签
    ///
    /// # 参数
    /// * `tag_name` - 要删除的标签名称
    ///
    /// # 返回值
    /// * `Ok(())` - 成功删除标签
    /// * `Err(GitError)` - 删除失败时返回错误
    pub fn delete_tag(&self, tag_name: &str) -> Result<(), GitError> {
        self.raw_repo().tag_delete(tag_name)?;
        Ok(())
    }

    /// 推送标签到远程仓库,更新仓库中的标签引用
    ///
    /// # 参数
    /// * `remote_name` - 远程仓库名称（如 "origin"）
    /// * `tag_name` - 要推送的标签名称
    /// * `username` - 用户名（可选）
    /// * `password` - 密码（可选）
    ///
    /// # 返回值
    /// * `Ok(())` - 成功推送标签
    /// * `Err(GitError)` - 推送失败时返回错误
    pub fn push_tag(&self, tag_name: &str) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        // 设置认证回调
        let callbacks = self.configure_remote_callbacks()?;

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // 推送标签
        let refspec = format!("refs/tags/{}:refs/tags/{}", tag_name, tag_name);
        remote.push(&[&refspec], Some(&mut push_options))?;

        Ok(())
    }

    /// 推送所有标签到远程仓库,更新仓库中的标签引用
    ///
    ///
    /// # 返回值
    /// * `Ok(())` - 成功推送所有标签
    /// * `Err(GitError)` - 推送失败时返回错误
    pub fn push_all_tags(&self) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        // 设置认证回调
        let callbacks = self.configure_remote_callbacks()?;

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // 推送所有标签
        remote.push(&["refs/tags/*:refs/tags/*"], Some(&mut push_options))?;

        Ok(())
    }

    /// 删除远程标签
    ///
    /// # 参数
    /// * `tag_name` - 要删除的标签名称
    ///
    /// # 返回值
    /// * `Ok(())` - 成功删除远程标签
    /// * `Err(GitError)` - 删除失败时返回错误
    pub fn delete_remote_tag(&self, tag_name: &str) -> Result<(), GitError> {
        let mut remote = self.raw_repo().find_remote(DEFAULT_ORIGIN)?;

        // 设置认证回调
        let callbacks = self.configure_remote_callbacks()?;

        let mut push_options = git2::PushOptions::new();
        push_options.remote_callbacks(callbacks);

        // 删除远程标签（通过推送空引用）
        let refspec = format!(":refs/tags/{}", tag_name);
        remote.push(&[&refspec], Some(&mut push_options))?;

        Ok(())
    }
}

impl GitRepository {
    /// 查看暂存区与 HEAD 的差异（已暂存的更改）
    ///
    /// # 返回值
    /// * `Ok(String)` - 成功获取差异时返回 diff 字符串
    /// * `Err(GitError)` - 获取差异失败时返回错误
    pub fn diff_index_to_head(&self) -> Result<String, GitError> {
        let head = self.repo.head()?;
        let head_tree = head.peel_to_tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff = self
            .repo
            .diff_tree_to_index(Some(&head_tree), None, Some(&mut diff_opts))?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let origin = line.origin();
            let content = std::str::from_utf8(line.content()).unwrap_or("");

            match origin {
                '+' | '-' | ' ' => {
                    diff_text.push(origin);
                    diff_text.push_str(content);
                }
                _ => {
                    diff_text.push_str(content);
                }
            }
            true
        })?;

        Ok(diff_text)
    }

    /// 查看两个提交之间的差异
    ///
    /// # 参数
    /// * `old_commit_id` - 旧提交的 ID（字符串）
    /// * `new_commit_id` - 新提交的 ID（字符串）
    ///
    /// # 返回值
    /// * `Ok(String)` - 成功获取差异时返回 diff 字符串
    /// * `Err(GitError)` - 获取差异失败时返回错误
    pub fn diff_commits(
        &self,
        old_commit_id: &str,
        new_commit_id: &str,
    ) -> Result<String, GitError> {
        let old_oid = git2::Oid::from_str(old_commit_id)?;
        let new_oid = git2::Oid::from_str(new_commit_id)?;

        let old_commit = self.repo.find_commit(old_oid)?;
        let new_commit = self.repo.find_commit(new_oid)?;

        let old_tree = old_commit.tree()?;
        let new_tree = new_commit.tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff =
            self.repo
                .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut diff_opts))?;

        let mut diff_text = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let origin = line.origin();
            let content = std::str::from_utf8(line.content()).unwrap_or("");

            match origin {
                '+' | '-' | ' ' => {
                    diff_text.push(origin);
                    diff_text.push_str(content);
                }
                _ => {
                    diff_text.push_str(content);
                }
            }
            true
        })?;

        Ok(diff_text)
    }

    /// 查看最新标签与 HEAD 的差异，返回结构化的差异数据
    ///
    /// # 返回值
    /// * `Ok(DiffResult)` - 返回结构化的差异数据，包含每个文件的差异信息
    /// * `Err(GitError)` - 获取差异失败时返回错误
    pub fn diff_tag_with_head_and_stats(&self) -> Result<DiffResultWithFiles, GitError> {
        // 获取最新标签名
        let latest_tag = GitRepository::get_latest_tag(self)?;

        // 解析标签引用，获取标签指向的提交
        let tag_ref = self
            .repo
            .revparse_single(&format!("refs/tags/{}", latest_tag))?;
        let tag_commit = tag_ref.peel_to_commit()?;
        let tag_tree = tag_commit.tree()?;

        // 获取HEAD指向的提交
        let head_commit = self.repo.head()?.peel_to_commit()?;
        let head_tree = head_commit.tree()?;

        // 创建 diff 对象
        let mut diff_opts = git2::DiffOptions::new();
        let diff =
            self.repo
                .diff_tree_to_tree(Some(&tag_tree), Some(&head_tree), Some(&mut diff_opts))?;

        // 获取统计信息
        let stats = diff.stats()?;
        let files_changed = stats.files_changed();
        let insertions = stats.insertions();
        let deletions = stats.deletions();

        // 收集每个文件的差异信息
        let mut file_diffs = Vec::new();

        diff.foreach(
            &mut |delta, _progress| {
                let old_file = delta.old_file();
                let new_file = delta.new_file();

                // 确定文件路径
                let file_path = new_file
                    .path()
                    .or_else(|| old_file.path())
                    .and_then(|p| p.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let old_path = if delta.status() == git2::Delta::Renamed {
                    old_file
                        .path()
                        .and_then(|p| p.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                };

                // 确定变更类型
                let change_type = match delta.status() {
                    git2::Delta::Added => "add",
                    git2::Delta::Deleted => "delete",
                    git2::Delta::Modified => "modify",
                    git2::Delta::Renamed => "rename",
                    _ => "modify",
                }
                .to_string();

                file_diffs.push(FileDiffInfo {
                    file_path,
                    old_path,
                    change_type,
                    diff_text: String::new(), // 将在下一步填充
                });

                true
            },
            None,
            None,
            None,
        )?;

        // 为每个文件生成统一差异格式文本
        let mut file_index = 0;
        diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
            // 检测文件头（diff --git）来切换到下一个文件
            let origin = line.origin();
            let content = std::str::from_utf8(line.content()).unwrap_or("");

            // 如果是文件头，移动到下一个文件
            if origin == 'F' || (origin == ' ' && content.starts_with("diff --git")) {
                // 查找对应的文件
                let new_file_path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .and_then(|p| p.to_str())
                    .unwrap_or("unknown");

                if let Some(pos) = file_diffs.iter().position(|f| f.file_path == new_file_path) {
                    file_index = pos;
                }
            }

            // 添加内容到当前文件的 diff_text
            if file_index < file_diffs.len() {
                match origin {
                    '+' | '-' | ' ' => {
                        file_diffs[file_index].diff_text.push(origin);
                        file_diffs[file_index].diff_text.push_str(content);
                    }
                    _ => {
                        file_diffs[file_index].diff_text.push_str(content);
                    }
                }
            }

            true
        })?;

        Ok(DiffResultWithFiles {
            files: file_diffs,
            stats: DiffStats {
                files_changed,
                insertions,
                deletions,
            },
        })
    }

    /// 查看当前版本与之前的版本的差异，返回结构化的差异数据
    ///
    /// # 参数
    /// * `curr_version` - 当前版本标签名称
    ///
    /// # 返回值
    /// * `Ok(DiffResultWithFiles)` - 返回结构化的差异数据，包含每个文件的差异信息
    /// * `Err(GitError)` - 获取差异失败时返回错误
    pub fn diff_with_previous_version(
        &self,
        curr_version: &str,
    ) -> Result<DiffResultWithFiles, GitError> {
        // 获取前一个版本的标签名
        let previous_tag = self.get_previous_tag(curr_version)?;

        // 解析当前版本标签引用
        let curr_tag_ref = self
            .repo
            .revparse_single(&format!("refs/tags/{}", curr_version))?;
        let curr_commit = curr_tag_ref.peel_to_commit()?;
        let curr_tree = curr_commit.tree()?;

        // 解析前一个版本标签引用
        let prev_tag_ref = self
            .repo
            .revparse_single(&format!("refs/tags/{}", previous_tag))?;
        let prev_commit = prev_tag_ref.peel_to_commit()?;
        let prev_tree = prev_commit.tree()?;

        // 创建 diff 对象（从前一个版本到当前版本）
        let mut diff_opts = git2::DiffOptions::new();
        let diff = self.repo.diff_tree_to_tree(
            Some(&prev_tree),
            Some(&curr_tree),
            Some(&mut diff_opts),
        )?;

        // 获取统计信息
        let stats = diff.stats()?;
        let files_changed = stats.files_changed();
        let insertions = stats.insertions();
        let deletions = stats.deletions();

        // 收集每个文件的差异信息
        let mut file_diffs = Vec::new();

        diff.foreach(
            &mut |delta, _progress| {
                let old_file = delta.old_file();
                let new_file = delta.new_file();

                // 确定文件路径
                let file_path = new_file
                    .path()
                    .or_else(|| old_file.path())
                    .and_then(|p| p.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                let old_path = if delta.status() == git2::Delta::Renamed {
                    old_file
                        .path()
                        .and_then(|p| p.to_str())
                        .map(|s| s.to_string())
                } else {
                    None
                };

                // 确定变更类型
                let change_type = match delta.status() {
                    git2::Delta::Added => "add",
                    git2::Delta::Deleted => "delete",
                    git2::Delta::Modified => "modify",
                    git2::Delta::Renamed => "rename",
                    _ => "modify",
                }
                .to_string();

                file_diffs.push(FileDiffInfo {
                    file_path,
                    old_path,
                    change_type,
                    diff_text: String::new(), // 将在下一步填充
                });

                true
            },
            None,
            None,
            None,
        )?;

        // 为每个文件生成统一差异格式文本
        let mut file_index = 0;
        diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
            // 检测文件头（diff --git）来切换到下一个文件
            let origin = line.origin();
            let content = std::str::from_utf8(line.content()).unwrap_or("");

            // 如果是文件头，移动到下一个文件
            if origin == 'F' || (origin == ' ' && content.starts_with("diff --git")) {
                // 查找对应的文件
                let new_file_path = delta
                    .new_file()
                    .path()
                    .or_else(|| delta.old_file().path())
                    .and_then(|p| p.to_str())
                    .unwrap_or("unknown");

                if let Some(pos) = file_diffs.iter().position(|f| f.file_path == new_file_path) {
                    file_index = pos;
                }
            }

            // 添加内容到当前文件的 diff_text
            if file_index < file_diffs.len() {
                match origin {
                    '+' | '-' | ' ' => {
                        file_diffs[file_index].diff_text.push(origin);
                        file_diffs[file_index].diff_text.push_str(content);
                    }
                    _ => {
                        file_diffs[file_index].diff_text.push_str(content);
                    }
                }
            }

            true
        })?;

        Ok(DiffResultWithFiles {
            files: file_diffs,
            stats: DiffStats {
                files_changed,
                insertions,
                deletions,
            },
        })
    }

    /// 获取 diff 统计信息
    ///
    /// # 返回值
    /// * `Ok((usize, usize, usize))` - 返回 (文件数, 插入行数, 删除行数)
    /// * `Err(GitError)` - 获取统计失败时返回错误
    pub fn diff_stats(&self) -> Result<(usize, usize, usize), GitError> {
        let head = self.repo.head()?;
        let head_tree = head.peel_to_tree()?;

        let mut diff_opts = git2::DiffOptions::new();
        let diff = self
            .repo
            .diff_tree_to_workdir_with_index(Some(&head_tree), Some(&mut diff_opts))?;

        let stats = diff.stats()?;
        Ok((stats.files_changed(), stats.insertions(), stats.deletions()))
    }
}
