//! Gitea 日常同步与发布 tag 逻辑。

use std::path::Path;

use crate::constants::release::GROUP_DRAFT;
use crate::db::ReleaseGroup;
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::{ProjectArea, SystemKind, layout_for_system};
use gitea::GiteaClient;

/// 获取下一个版本号，按 semver 自增 patch（`v1.0.0`, `v1.0.1`...）。
pub async fn get_next_version() -> Result<String, AppError> {
    use crate::db::find_all_releases;

    let (releases, _) =
        find_all_releases(Some(SystemKind::Wparse), 1, 1000, None, None, None, None).await?;
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
        .filter(|r| r.release_group != GROUP_DRAFT)
        .filter_map(|r| parse_semver(&r.version))
        .max()
    {
        Ok(format!("v{}.{}.{}", major, minor, patch + 1))
    } else {
        Ok("v1.0.1".to_string())
    }
}

/// 推送代码到 git 并创建版本 tag（用于发布流程）。
pub async fn push_and_tag_release(
    version: &str,
    system: SystemKind,
    group: ReleaseGroup,
) -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let gitea_client = super::build_gitea_client(&setting)?;
    let commit_message = format!("Release {}", version);
    for (project_path, repo_label) in release_group_repo_targets(&layout, system, group) {
        push_and_tag_single_repo(
            &gitea_client,
            &project_path,
            version,
            &commit_message,
            &repo_label,
        )?;
    }

    Ok(())
}

fn release_group_repo_targets(
    layout: &crate::server::RepoLayout,
    system: SystemKind,
    group: ReleaseGroup,
) -> Vec<(std::path::PathBuf, String)> {
    match group {
        ReleaseGroup::Models => vec![(
            layout.models_root.clone(),
            format!("system={}, area=models", system.as_ref()),
        )],
        ReleaseGroup::Infra => vec![(
            layout.infra_root.clone(),
            format!("system={}, area=infra", system.as_ref()),
        )],
    }
}

fn push_and_tag_single_repo(
    gitea_client: &GiteaClient,
    project_path: &Path,
    version: &str,
    commit_message: &str,
    repo_label: &str,
) -> Result<(), AppError> {
    info!(
        "开始推送代码并创建 tag: repo={}, version={}",
        repo_label, version
    );

    let local_repo = gitea_client
        .open(project_path)
        .map_err(|e| AppError::internal(format!("打开本地仓库失败: {}", e)))?;

    let has_changes = local_repo
        .status()
        .map_err(|e| AppError::internal(format!("检查仓库状态失败: {}", e)))?
        .iter()
        .any(|status| status.status.bits() != 0);

    if has_changes {
        sync_named_repo_with_retry(gitea_client, project_path, commit_message, repo_label)?;
    } else {
        info!(
            "仓库无未提交改动，跳过 commit/push: repo={}, version={}",
            repo_label, version
        );
    }

    let tags = local_repo
        .list_tags()
        .map_err(|e| AppError::internal(format!("读取标签失败: {}", e)))?;
    if !tags.iter().any(|tag| tag == version) {
        gitea_client
            .create_push_tag(project_path, version)
            .map_err(|e| AppError::internal(format!("创建 tag 失败: {}", e)))?;
        info!(
            "Tag 创建并推送成功: repo={}, version={}",
            repo_label, version
        );
    } else {
        info!(
            "Tag 已存在，跳过创建: repo={}, version={}",
            repo_label, version
        );
    }

    Ok(())
}

/// 同步仓库到 Gitea，必要时先拉取远端再重试。
pub(super) fn sync_repo_with_retry(
    gitea_client: &GiteaClient,
    project_path: &Path,
    commit_message: &str,
    system: SystemKind,
    area: ProjectArea,
) -> Result<(), AppError> {
    let repo_label = format!("system={}, area={}", system.as_ref(), area.as_ref());
    sync_named_repo_with_retry(gitea_client, project_path, commit_message, &repo_label)
}

/// 同步具名仓库到 Gitea，必要时先拉取远端再重试。
pub(super) fn sync_named_repo_with_retry(
    gitea_client: &GiteaClient,
    project_path: &Path,
    commit_message: &str,
    repo_label: &str,
) -> Result<(), AppError> {
    match gitea_client.add_commit_push(commit_message, project_path) {
        Ok(_) => {
            info!("配置同步到 Gitea 成功: {}", repo_label);
            Ok(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("NotFastForward") || error_msg.contains("not present locally") {
                info!("检测到远程有新提交，开始拉取后重试推送: {}", repo_label);
                match gitea_client.open(project_path) {
                    Ok(local_repo) => match local_repo.pull() {
                        Ok(_) => match gitea_client.add_commit_push(commit_message, project_path) {
                            Ok(_) => {
                                info!("配置同步到 Gitea 成功: {}", repo_label);
                                Ok(())
                            }
                            Err(retry_err) => {
                                let message = format!(
                                    "重新推送到 Gitea 失败: {}, error={}",
                                    repo_label, retry_err
                                );
                                warn!("{}", message);
                                Err(AppError::git(message))
                            }
                        },
                        Err(pull_err) => {
                            let message =
                                format!("拉取远程更改失败: {}, error={}", repo_label, pull_err);
                            warn!("{}", message);
                            Err(AppError::git(message))
                        }
                    },
                    Err(open_err) => {
                        let message =
                            format!("打开本地仓库失败: {}, error={}", repo_label, open_err);
                        warn!("{}", message);
                        Err(AppError::git(message))
                    }
                }
            } else if error_msg.contains("current tip is not the first parent")
                || error_msg.contains("failed to create commit")
            {
                info!(
                    "仓库无有效文件变更，跳过同步: {}, message={}",
                    repo_label, error_msg
                );
                Ok(())
            } else {
                let message = format!("同步配置到 Gitea 失败: {}, error={}", repo_label, e);
                warn!("{}", message);
                Err(AppError::git(message))
            }
        }
    }
}
