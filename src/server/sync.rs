// 配置同步辅助模块 - 统一处理双仓库 Gitea 同步与发布 tag

use crate::db::{ReleaseGroup, RuleType};
use crate::error::AppError;
use crate::server::{ProjectLayout, Setting};
use gitea::{GiteaClient, GiteaConfig};
use std::path::{Path, PathBuf};

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

fn project_path_for_group(layout: &ProjectLayout, group: ReleaseGroup) -> PathBuf {
    match group {
        ReleaseGroup::Models => layout.models_root.clone(),
        ReleaseGroup::Infra => layout.infra_root.clone(),
    }
}

fn repo_name_for_group(group: ReleaseGroup) -> &'static str {
    match group {
        ReleaseGroup::Models => "project_models",
        ReleaseGroup::Infra => "project_infra",
    }
}

/// 同步指定分组仓库到 Gitea（支持自动处理冲突）
pub async fn sync_to_gitea(commit_message: &str, group: ReleaseGroup) {
    let setting = Setting::load();
    let layout = setting.project_layout();

    match build_gitea_client(&setting) {
        Ok(gitea_client) => {
            let project_path = project_path_for_group(&layout, group);
            sync_repo_with_retry(&gitea_client, &project_path, commit_message, group);
        }
        Err(e) => {
            warn!(
                "创建 Gitea 客户端失败（配置已保存）: group={}, error={}",
                group.as_ref(),
                e
            );
        }
    }
}

/// 同步所有仓库到 Gitea。
pub async fn sync_to_gitea_all(commit_message: &str) {
    sync_to_gitea(commit_message, ReleaseGroup::Models).await;
    sync_to_gitea(commit_message, ReleaseGroup::Infra).await;
}

/// 同步删除到 Gitea
pub async fn sync_delete_to_gitea(rule_type: RuleType, file_name: &str) {
    let commit_message = format!("删除 {} 文件: {}", rule_type.as_ref(), file_name);
    sync_to_gitea(&commit_message, ReleaseGroup::from_rule_type(rule_type)).await;
}

const REPO_BASELINE_TAG: &str = "baseline";

/// 初始化双仓库 Gitea 仓库和基线 tag（系统首次启动且本地 .git 不存在时调用）
pub async fn init_gitea_repo() -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = setting.project_layout();
    let gitea_client = build_gitea_client(&setting)
        .map_err(|e| AppError::internal(format!("无法连接 Gitea: {}", e)))?;

    crate::db::init_default_configs_to_models(&setting.project_models)
        .map_err(|e| AppError::internal(format!("初始化 models 默认配置失败: {}", e)))?;
    crate::db::init_default_configs_to_infra(&setting.project_infra)
        .map_err(|e| AppError::internal(format!("初始化 infra 默认配置失败: {}", e)))?;

    init_single_repo(
        &setting,
        &gitea_client,
        ReleaseGroup::Models,
        &layout.models_root,
    )
    .await?;
    init_single_repo(
        &setting,
        &gitea_client,
        ReleaseGroup::Infra,
        &layout.infra_root,
    )
    .await?;

    Ok(())
}

async fn init_single_repo(
    setting: &Setting,
    gitea_client: &GiteaClient,
    group: ReleaseGroup,
    project_path: &Path,
) -> Result<(), AppError> {
    let repo_name = repo_name_for_group(group);

    info!(
        "初始化 Gitea 仓库: group={}, repo_name={}, path={}",
        group.as_ref(),
        repo_name,
        project_path.display()
    );

    match gitea_client.create_repo(repo_name).await {
        Ok(_) => info!("远程仓库创建成功: repo_name={}", repo_name),
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("409") || error_msg.contains("already exists") {
                info!("远程仓库已存在，跳过创建: repo_name={}", repo_name);
            } else {
                return Err(AppError::internal(format!(
                    "创建远程仓库失败: repo_name={}, error={}",
                    repo_name, e
                )));
            }
        }
    };

    let clone_url = format!(
        "{}/{}/{}.git",
        setting.gitea.base_url.trim_end_matches('/'),
        setting.gitea.username,
        repo_name
    );
    let auth_url = build_auth_url(&clone_url, &setting.gitea.username, &setting.gitea.password);

    std::fs::create_dir_all(project_path)
        .map_err(|e| AppError::internal(format!("创建项目目录失败: {}", e)))?;

    if !project_path.join(".git").exists() {
        run_git(&["init", "-b", "main"], project_path, "git init")?;
    }

    match run_git(
        &["remote", "add", "origin", &auth_url],
        project_path,
        "git remote add",
    ) {
        Ok(_) => {}
        Err(e) if e.to_string().contains("already exists") => {}
        Err(e) => return Err(e),
    }

    let readme_path = project_path.join("README.md");
    if !readme_path.exists() {
        std::fs::write(
            &readme_path,
            format!(
                "# WarpStation {}\n\nThis repository contains WarpStation configuration files.\n",
                group.as_ref()
            ),
        )
        .map_err(|e| AppError::internal(format!("创建 README.md 失败: {}", e)))?;
    }

    run_git(&["add", "."], project_path, "git add")?;

    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查 git 状态失败: {}", e)))?;

    if !status_output.stdout.is_empty() {
        run_git(&["commit", "-m", "初始化配置"], project_path, "git commit")?;
        run_git(&["push", &auth_url, "main"], project_path, "git push")?;
    }

    ensure_tag_exists(project_path, REPO_BASELINE_TAG)?;
    push_tag_if_needed(project_path, &auth_url, REPO_BASELINE_TAG)?;

    Ok(())
}

fn build_auth_url(url: &str, username: &str, password: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        format!("http://{}:{}@{}", username, password, rest)
    } else if let Some(rest) = url.strip_prefix("https://") {
        format!("https://{}:{}@{}", username, password, rest)
    } else {
        url.to_string()
    }
}

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

/// 获取下一个版本号，按 semver 自增 patch（v1.0.0, v1.0.1...）
pub async fn get_next_version() -> Result<String, AppError> {
    use crate::db::find_all_releases;

    let (releases, _) = find_all_releases(1, 1000, None, None, None, None).await?;
    fn parse_semver(raw: &str) -> Option<(u32, u32, u32)> {
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
        .filter(|r| r.release_group != "draft")
        .filter_map(|r| parse_semver(&r.version))
        .max()
    {
        Ok(format!("v{}.{}.{}", major, minor, patch + 1))
    } else {
        Ok("v1.0.1".to_string())
    }
}

/// 推送代码到 git 并创建版本 tag（用于发布流程）
pub async fn push_and_tag_release(version: &str, group: ReleaseGroup) -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = setting.project_layout();
    let gitea_client = build_gitea_client(&setting)?;
    let project_path = project_path_for_group(&layout, group);

    info!(
        "开始推送代码并创建 tag: group={}, version={}",
        group.as_ref(),
        version
    );

    let local_repo = gitea_client
        .open(&project_path)
        .map_err(|e| AppError::internal(format!("打开本地仓库失败: {}", e)))?;

    let has_changes = local_repo
        .status()
        .map_err(|e| AppError::internal(format!("检查仓库状态失败: {}", e)))?
        .iter()
        .any(|status| status.status.bits() != 0);

    if has_changes {
        let commit_message = format!("Release {}", version);
        sync_repo_with_retry(&gitea_client, &project_path, &commit_message, group);
    } else {
        info!(
            "仓库无未提交改动，跳过 commit/push: group={}, version={}",
            group.as_ref(),
            version
        );
    }

    let tags = local_repo
        .list_tags()
        .map_err(|e| AppError::internal(format!("读取标签失败: {}", e)))?;
    if !tags.iter().any(|tag| tag == version) {
        gitea_client
            .create_push_tag(&project_path, version)
            .map_err(|e| AppError::internal(format!("创建 tag 失败: {}", e)))?;
        info!(
            "Tag 创建并推送成功: group={}, version={}",
            group.as_ref(),
            version
        );
    } else {
        info!(
            "Tag 已存在，跳过创建: group={}, version={}",
            group.as_ref(),
            version
        );
    }

    Ok(())
}

fn sync_repo_with_retry(
    gitea_client: &GiteaClient,
    project_path: &Path,
    commit_message: &str,
    group: ReleaseGroup,
) {
    match gitea_client.add_commit_push(commit_message, project_path) {
        Ok(_) => {
            info!("配置同步到 Gitea 成功: group={}", group.as_ref());
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("NotFastForward") || error_msg.contains("not present locally") {
                info!(
                    "检测到远程有新提交，开始拉取后重试推送: group={}",
                    group.as_ref()
                );
                match gitea_client.open(project_path) {
                    Ok(local_repo) => match local_repo.pull() {
                        Ok(_) => match gitea_client.add_commit_push(commit_message, project_path) {
                            Ok(_) => info!("配置同步到 Gitea 成功: group={}", group.as_ref()),
                            Err(retry_err) => warn!(
                                "重新推送失败（配置已保存）: group={}, error={}",
                                group.as_ref(),
                                retry_err
                            ),
                        },
                        Err(pull_err) => warn!(
                            "拉取远程更改失败（配置已保存）: group={}, error={}",
                            group.as_ref(),
                            pull_err
                        ),
                    },
                    Err(open_err) => warn!(
                        "打开本地仓库失败（配置已保存）: group={}, error={}",
                        group.as_ref(),
                        open_err
                    ),
                }
            } else if error_msg.contains("current tip is not the first parent")
                || error_msg.contains("failed to create commit")
            {
                info!(
                    "仓库无有效文件变更，跳过同步: group={}, message={}",
                    group.as_ref(),
                    error_msg
                );
            } else {
                warn!(
                    "同步配置到 Gitea 失败（配置已保存）: group={}, error={}",
                    group.as_ref(),
                    e
                );
            }
        }
    }
}

fn ensure_tag_exists(project_path: &Path, version: &str) -> Result<(), AppError> {
    let output = std::process::Command::new("git")
        .args(["tag", "--list", version])
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查标签失败: {}", e)))?;

    if String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        run_git(&["tag", version], project_path, "git tag")?;
    }

    Ok(())
}

fn push_tag_if_needed(project_path: &Path, auth_url: &str, version: &str) -> Result<(), AppError> {
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--tags", auth_url, version])
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查远程标签失败: {}", e)))?;

    if String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        run_git(&["push", auth_url, version], project_path, "git push tag")?;
    }

    Ok(())
}
