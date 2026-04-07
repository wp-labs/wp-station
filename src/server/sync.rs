// 配置同步辅助模块 - 统一处理 Gitea 同步和草稿发布记录

use crate::db::{
    NewRelease, ReleaseStatus, RuleType, create_release, find_draft_release,
    update_release_timestamp,
};
use crate::error::AppError;
use crate::server::Setting;
use gitea::{GiteaClient, GiteaConfig};
use std::path::{Path, PathBuf};

/// 将配置中的项目根目录解析为可操作的本地绝对路径。
fn resolve_project_path(project_root: &str) -> PathBuf {
    let project_root = PathBuf::from(project_root);
    if project_root.is_absolute() {
        project_root
    } else {
        Setting::workspace_root().join(project_root)
    }
}

/// 构建统一配置的 Gitea 客户端。
fn build_gitea_client(setting: &Setting) -> Result<GiteaClient, AppError> {
    let gitea_config = GiteaConfig::new(
        setting.gitea.base_url.clone(),
        setting.gitea.username.clone(),
        setting.gitea.password.clone(),
    )
    .with_branch("main".to_string());

    GiteaClient::new(gitea_config)
        .map_err(|e| AppError::internal(format!("创建 Gitea 客户端失败: {}", e)))
}

/// 同步配置到 Gitea（支持自动处理冲突）
pub async fn sync_to_gitea(commit_message: &str) {
    let setting = Setting::load();

    match build_gitea_client(&setting) {
        Ok(gitea_client) => {
            let project_path = resolve_project_path(&setting.project_root);

            // 尝试推送，如果失败则先拉取再推送
            match gitea_client.add_commit_push(commit_message, project_path.clone()) {
                Ok(_) => {
                    info!("配置同步到 Gitea 成功");
                }
                Err(e) => {
                    // 检查是否是 NotFastForward 错误
                    let error_msg = e.to_string();
                    if error_msg.contains("NotFastForward")
                        || error_msg.contains("not present locally")
                    {
                        info!("检测到远程有新提交，开始拉取后重试推送");

                        // 打开本地仓库并拉取
                        match gitea_client.open(&project_path) {
                            Ok(local_repo) => match local_repo.pull() {
                                Ok(_) => {
                                    info!("拉取远程更改成功，重新推送配置");
                                    // 重新推送
                                    match gitea_client.add_commit_push(commit_message, project_path)
                                    {
                                        Ok(_) => {
                                            info!("配置同步到 Gitea 成功");
                                        }
                                        Err(e2) => {
                                            warn!("重新推送失败（配置已保存）: error={}", e2);
                                        }
                                    }
                                }
                                Err(pull_err) => {
                                    warn!("拉取远程更改失败（配置已保存）: error={}", pull_err);
                                }
                            },
                            Err(open_err) => {
                                warn!("打开本地仓库失败（配置已保存）: error={}", open_err);
                            }
                        }
                    } else {
                        warn!("同步配置到 Gitea 失败（配置已保存）: error={}", e);
                    }
                }
            }
        }
        Err(e) => {
            warn!("创建 Gitea 客户端失败（配置已保存）: error={}", e);
        }
    }
}

/// 同步删除到 Gitea
pub async fn sync_delete_to_gitea(rule_type: RuleType, file_name: &str) {
    let commit_message = format!("删除 {} 文件: {}", rule_type.as_ref(), file_name);
    sync_to_gitea(&commit_message).await;
}

/// 处理草稿发布记录（创建或更新）
pub async fn handle_draft_release(operator: Option<&str>) -> Result<(), AppError> {
    let draft_release = find_draft_release().await?;

    if draft_release.is_none() {
        let next_version = get_next_version().await?;
        info!("创建草稿发布记录: version={}", next_version);

        let new_release = NewRelease {
            version: next_version.clone(),
            pipeline: Some("草稿".to_string()),
            created_by: operator.map(|name| name.to_string()),
            stages: Some("自动创建的草稿版本".to_string()),
            status: Some(ReleaseStatus::WAIT),
        };

        let release_id = create_release(new_release).await?;
        info!(
            "草稿发布记录创建成功: release_id={}, version={}",
            release_id, next_version
        );
    } else if let Some(draft) = draft_release {
        update_release_timestamp(draft.id).await?;
        info!("更新草稿发布记录时间戳: release_id={}", draft.id);
    }

    Ok(())
}

/// 初始化 Gitea 仓库和 v1.0.0 tag（系统首次启动且本地 .git 不存在时调用）
pub async fn init_gitea_repo() -> Result<(), AppError> {
    let setting = Setting::load();
    let project_path = resolve_project_path(&setting.project_root);

    let repo_name = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project_root");

    let gitea_client = build_gitea_client(&setting)
        .map_err(|e| AppError::internal(format!("无法连接 Gitea: {}", e)))?;

    // 1. 在 Gitea 上创建远程仓库（已存在则跳过）
    info!("在 Gitea 上创建远程仓库: {}", repo_name);
    let repo_info = match gitea_client.create_repo(repo_name).await {
        Ok(info) => {
            info!("远程仓库创建成功");
            info
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("409") || error_msg.contains("already exists") {
                info!("远程仓库已存在，跳过创建");
                use gitea::RepoInfo;
                let clone_url = format!(
                    "{}/{}/{}.git",
                    setting.gitea.base_url.trim_end_matches('/'),
                    setting.gitea.username,
                    repo_name
                );
                RepoInfo::new(
                    repo_name.to_string(),
                    clone_url.clone(),
                    clone_url.clone(),
                    clone_url,
                )
            } else {
                return Err(AppError::internal(format!("创建远程仓库失败: {}", e)));
            }
        }
    };

    let auth_url = build_auth_url(
        &repo_info.clone_url,
        &setting.gitea.username,
        &setting.gitea.password,
    );

    // 2. 确保目录存在并初始化本地仓库
    info!("初始化本地 Git 仓库: path={}", project_path.display());
    std::fs::create_dir_all(&project_path)
        .map_err(|e| AppError::internal(format!("创建项目目录失败: {}", e)))?;
    run_git(&["init", "-b", "main"], &project_path, "git init")?;

    // 3. 添加远程（已存在则跳过）
    info!("添加远程仓库: {}", repo_info.clone_url);
    match run_git(
        &["remote", "add", "origin", &auth_url],
        &project_path,
        "git remote add",
    ) {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {}
        Err(e) => return Err(e),
    }

    // 4. 确保项目目录有内容：先从数据库导出配置
    info!("从数据库导出配置到项目目录");
    use crate::db::get_pool;
    let pool = get_pool();
    crate::utils::export_project_from_db(pool.inner(), &setting.project_root)
        .await
        .map_err(|e| AppError::internal(format!("导出配置失败: {}", e)))?;
    info!("配置导出完成");

    // 5. 创建 README.md（如果不存在）
    let readme_path = project_path.join("README.md");
    if !readme_path.exists() {
        std::fs::write(&readme_path, "# WarpStation Configuration\n\nThis repository contains WarpStation configuration files.\n")
            .map_err(|e| AppError::internal(format!("创建 README.md 失败: {}", e)))?;
    }

    // 6. 添加文件并提交
    info!("添加文件并提交");
    run_git(&["add", "."], &project_path, "git add")?;

    // 检查是否有文件需要提交
    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查 git 状态失败: {}", e)))?;

    if !status_output.stdout.is_empty() {
        run_git(&["commit", "-m", "初始化配置"], &project_path, "git commit")?;

        // 7. 推送到远程
        info!("推送到远程 main 分支");
        run_git(&["push", &auth_url, "main"], &project_path, "git push")?;

        // 8. 创建 v1.0.0 tag 并推送
        info!("创建并推送 v1.0.0 tag");
        run_git(&["tag", "v1.0.0"], &project_path, "git tag")?;
        run_git(
            &["push", &auth_url, "v1.0.0"],
            &project_path,
            "git push tag",
        )?;

        info!("Gitea 仓库初始化完成，已创建 v1.0.0 tag");
    } else {
        debug!("没有文件需要提交，跳过 commit 和 push");
    }

    Ok(())
}

// ── 私有辅助函数 ──────────────────────────────────────────────

/// 构建带认证信息的 Git URL
fn build_auth_url(url: &str, username: &str, password: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        format!("http://{}:{}@{}", username, password, rest)
    } else if let Some(rest) = url.strip_prefix("https://") {
        format!("https://{}:{}@{}", username, password, rest)
    } else {
        url.to_string()
    }
}

/// 执行 git 子命令
fn run_git(args: &[&str], dir: &Path, label: &str) -> Result<(), AppError> {
    use std::process::Command;

    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| AppError::internal(format!("执行 {} 失败: {}", label, e)))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::internal(format!(
            "{} 失败: {}",
            label,
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// 获取下一个版本号，按 semver 自增 patch（v1.0.1, v1.0.2...）
/// 基于数据库中所有版本（不限状态）的最大版本号递增
/// 如果数据库无版本，返回 v1.0.1（因为 v1.0.0 是初始 tag）
pub async fn get_next_version() -> Result<String, AppError> {
    use crate::db::find_all_releases;

    // 查询所有发布记录，不限状态
    let (releases, _) = find_all_releases(1, 1000, None, None, None, None).await?;

    fn parse_semver(raw: &str) -> Option<(u32, u32, u32)> {
        // 同时支持大写 V 和小写 v 前缀
        let trimmed = raw.strip_prefix('v').or_else(|| raw.strip_prefix('V'))?;
        let mut parts = trimmed.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some((major, minor, patch))
    }

    if let Some((major, minor, patch)) = releases
        .iter()
        .filter_map(|r| parse_semver(&r.version))
        .max()
    {
        // 统一返回小写 v 前缀
        Ok(format!("v{}.{}.{}", major, minor, patch + 1))
    } else {
        // 数据库无版本时返回 v1.0.1（v1.0.0 是初始 tag）
        Ok("v1.0.1".to_string())
    }
}

/// 推送代码到 git 并创建版本 tag（用于发布流程）
pub async fn push_and_tag_release(version: &str) -> Result<(), AppError> {
    let setting = Setting::load();
    let gitea_client = build_gitea_client(&setting)?;
    let project_path = resolve_project_path(&setting.project_root);

    info!("开始推送代码并创建 tag: version={}", version);

    // 1. 先提交并推送代码
    let commit_message = format!("Release {}", version);
    gitea_client
        .add_commit_push(&commit_message, project_path.clone())
        .map_err(|e| AppError::internal(format!("推送代码失败: {}", e)))?;

    info!("代码推送成功: version={}", version);

    // 2. 创建并推送 tag
    gitea_client
        .create_push_tag(&project_path, version)
        .map_err(|e| AppError::internal(format!("创建 tag 失败: {}", e)))?;

    info!("Tag 创建并推送成功: version={}", version);

    Ok(())
}
