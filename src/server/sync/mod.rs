//! Gitea 同步辅助模块。
//!
//! 统一处理双仓库的初始化、同步、删除同步和基线 tag 管理。
//! 双系统改造后，仓库选择全部通过 `system + area/group` 解析。

mod publish;
mod setup;

use crate::db::{ReleaseGroup, RuleType};
use crate::error::AppError;
use crate::server::{RepoLayout, Setting};
use crate::utils::{ProjectArea, SystemKind, layout_for_system};
use gitea::{GiteaClient, GiteaConfig};
use std::path::PathBuf;

pub use self::publish::{get_next_version, push_and_tag_release};
pub use self::setup::{ensure_project_repositories, init_gitea_repo};

#[derive(Debug, Clone)]
pub struct RestoreCandidates {
    pub models_previous_head: Option<String>,
    pub infra_previous_head: Option<String>,
    pub models_candidate_commit: Option<String>,
    pub infra_candidate_commit: Option<String>,
}

/// 从历史仓库标签创建还原候选提交，仅推送目标标签，不移动 main。
///
/// 只有用户选择的仓库会参与候选提交。选择 models 时不读取、不校验、
/// 不创建 infra 候选；选择 all 时才按 models → infra 一起准备。
pub fn prepare_restore_candidates(
    system: SystemKind,
    source_version: &str,
    target_version: &str,
    groups: &[ReleaseGroup],
) -> Result<RestoreCandidates, AppError> {
    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let client = build_gitea_client(&setting)?;
    let mut candidates = RestoreCandidates {
        models_previous_head: None,
        infra_previous_head: None,
        models_candidate_commit: None,
        infra_candidate_commit: None,
    };

    for group in groups {
        let (path, label) = match group {
            ReleaseGroup::Models => (&layout.models_root, "models"),
            ReleaseGroup::Infra => (&layout.infra_root, "infra"),
        };
        let repo = client
            .open(path)
            .map_err(|error| AppError::git(format!("打开 {label} 仓库失败: {error}")))?;
        ensure_repo_tag_with_open_repo(&repo, source_version, label)?;
        let (previous_head, candidate_commit) = repo
            .create_candidate_from_tag(
                source_version,
                target_version,
                &format!("Restore {source_version} as {target_version}"),
            )
            .map_err(|error| AppError::git(format!("创建 {label} 候选版本失败: {error}")))?;
        match group {
            ReleaseGroup::Models => {
                candidates.models_previous_head = Some(previous_head);
                candidates.models_candidate_commit = Some(candidate_commit);
            }
            ReleaseGroup::Infra => {
                candidates.infra_previous_head = Some(previous_head);
                candidates.infra_candidate_commit = Some(candidate_commit);
            }
        }
    }

    Ok(candidates)
}

/// 将指定分组的候选提交提升为正式 main。
pub fn promote_restore_candidate(
    system: SystemKind,
    group: ReleaseGroup,
    expected_head: &str,
    candidate_commit: &str,
) -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let client = build_gitea_client(&setting)?;
    let path = repo_path_for_group(&layout, group);
    let repo = client
        .open(&path)
        .map_err(|error| AppError::git(format!("打开候选仓库失败: {error}")))?;
    repo.promote_candidate(expected_head, candidate_commit)
        .map_err(|error| AppError::git(format!("提升候选版本失败: {error}")))
}

/// 清理尚未提升为正式版本的还原候选标签。
///
/// 失败还原不能把临时版本留在 Gitea，否则后续版本选择和人工排查都会把
/// 未发布内容误认为正式版本。候选提交本身由 Git 垃圾回收处理，不需要额外删除。
pub fn cleanup_restore_candidates(
    system: SystemKind,
    target_version: &str,
    groups: &[ReleaseGroup],
) -> Result<(), AppError> {
    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let client = build_gitea_client(&setting)?;
    let mut errors = Vec::new();

    for group in groups {
        let path = match group {
            ReleaseGroup::Models => layout.models_root.clone(),
            ReleaseGroup::Infra => layout.infra_root.clone(),
        };
        let repo = client
            .open(&path)
            .map_err(|error| AppError::git(format!("打开 {} 仓库失败: {error}", group.as_ref())))?;
        if let Err(error) = repo.delete_remote_tag(target_version) {
            errors.push(format!("{} 远程标签: {error}", group.as_ref()));
        }
        if let Err(error) = repo.delete_tag(target_version) {
            errors.push(format!("{} 本地标签: {error}", group.as_ref()));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(AppError::git(format!(
            "清理还原候选标签失败: {}",
            errors.join("; ")
        )))
    }
}

/// 构造 Gitea 客户端。
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

/// 根据发布分组返回对应的本地仓库目录。
fn repo_path_for_group(layout: &RepoLayout, group: ReleaseGroup) -> PathBuf {
    match group {
        ReleaseGroup::Models => layout.models_root.clone(),
        ReleaseGroup::Infra => layout.infra_root.clone(),
    }
}

/// 将发布分组映射为固定仓库区域。
fn area_from_group(group: ReleaseGroup) -> ProjectArea {
    match group {
        ReleaseGroup::Models => ProjectArea::Models,
        ReleaseGroup::Infra => ProjectArea::Infra,
    }
}

/// 是否通过环境变量跳过 Gitea 同步。
fn should_skip_gitea_sync() -> bool {
    std::env::var("WARP_STATION_SKIP_GITEA")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// 同步指定分组仓库到 Gitea（支持自动处理冲突）
pub async fn sync_to_gitea(
    commit_message: &str,
    system: SystemKind,
    group: ReleaseGroup,
) -> Result<(), AppError> {
    if should_skip_gitea_sync() {
        info!(
            "跳过 Gitea 同步: group={}, reason=WARP_STATION_SKIP_GITEA",
            group.as_ref()
        );
        return Ok(());
    }

    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let gitea_client = build_gitea_client(&setting)?;
    let project_path = repo_path_for_group(&layout, group);
    publish::sync_repo_with_retry(
        &gitea_client,
        &project_path,
        commit_message,
        system,
        area_from_group(group),
    )
}

/// 同步所有仓库到 Gitea。
pub async fn sync_to_gitea_all(commit_message: &str, system: SystemKind) -> Result<(), AppError> {
    sync_to_gitea(commit_message, system, ReleaseGroup::Models).await?;
    sync_to_gitea(commit_message, system, ReleaseGroup::Infra).await?;
    Ok(())
}

/// 同步删除到 Gitea
pub async fn sync_delete_to_gitea(
    system: SystemKind,
    rule_type: RuleType,
    file_name: &str,
) -> Result<(), AppError> {
    let commit_message = format!("删除 {} 文件: {}", rule_type.as_ref(), file_name);
    sync_to_gitea(
        &commit_message,
        system,
        ReleaseGroup::from_rule_type(rule_type),
    )
    .await?;
    Ok(())
}

fn ensure_repo_tag_with_open_repo(
    local_repo: &gitea::LocalRepository,
    version: &str,
    repo_label: &str,
) -> Result<(), AppError> {
    if !local_repo
        .list_tags()
        .map_err(|e| AppError::internal(format!("读取仓库标签失败: {}", e)))?
        .iter()
        .any(|tag| tag == version)
    {
        local_repo
            .fetch_tags()
            .map_err(|e| AppError::internal(format!("拉取发布标签失败: {}", e)))?;
    }

    let has_tag = local_repo
        .list_tags()
        .map_err(|e| AppError::internal(format!("读取仓库标签失败: {}", e)))?
        .iter()
        .any(|tag| tag == version);
    if !has_tag {
        return Err(AppError::validation(format!(
            "Gitea 仓库缺少发布标签: repo={}, version={}",
            repo_label, version
        )));
    }
    Ok(())
}
