//! Gitea 同步辅助模块。
//!
//! 统一处理双仓库的初始化、同步、删除同步和基线 tag 管理。
//! 双系统改造后，仓库选择全部通过 `system + area/group` 解析。

mod publish;
mod setup;

use crate::db::{ReleaseGroup, RuleType};
use crate::error::AppError;
use crate::server::{RepoLayout, Setting};
use crate::utils::{
    ProjectArea, SystemKind, all_system_layouts, init_default_connectors_to_shared,
    layout_for_system, shared_connectors_root,
};
use gitea::{GiteaClient, GiteaConfig};
use std::fs;
use std::path::{Path, PathBuf};

use crate::constants::project::DIR_CONNECTORS;

use self::publish::sync_named_repo_with_retry;

pub use self::publish::{get_next_version, push_and_tag_release};
pub use self::setup::{ensure_project_repositories, init_gitea_repo};

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

/// 将共享 connectors 镜像到当前系统的 infra 仓库。
pub(crate) fn mirror_shared_connectors_to_system_infra(system: SystemKind) -> Result<(), AppError> {
    let layout = layout_for_system(system).as_repo_layout();
    mirror_shared_connectors_into_infra_root(&layout.infra_root)
}

/// 将共享 connectors 镜像到全部系统的 infra 仓库。
pub(crate) fn mirror_shared_connectors_to_all_system_infra() -> Result<(), AppError> {
    for layout in all_system_layouts() {
        mirror_shared_connectors_into_infra_root(&layout.infra_root)?;
    }
    Ok(())
}

/// 准备共享 connectors 工作区，并同步镜像到各系统的本地 infra 仓库。
pub(crate) fn prepare_shared_connectors_workspace() -> Result<(), AppError> {
    let shared_root = shared_connectors_root();
    let shared_connectors_dir = shared_root.join(DIR_CONNECTORS);

    if !dir_has_entries(&shared_connectors_dir)? {
        if let Some(source_dir) = first_system_connectors_dir() {
            copy_named_entry(&source_dir, &shared_root, DIR_CONNECTORS)?;
            info!(
                "已从系统 infra 回填共享 connectors 目录: source={}, target={}",
                source_dir.display(),
                shared_root.display()
            );
        } else {
            init_default_connectors_to_shared(shared_root.to_string_lossy().as_ref()).map_err(
                |e| AppError::internal(format!("初始化共享 connectors 默认配置失败: {}", e)),
            )?;
            info!(
                "已初始化共享 connectors 默认配置: path={}",
                shared_root.display()
            );
        }
    }

    mirror_shared_connectors_to_all_system_infra()?;
    Ok(())
}

/// 同步共享 connectors 到各系统的 infra 仓库并推送到 Gitea。
pub async fn sync_shared_connectors_to_infra_gitea(commit_message: &str) -> Result<(), AppError> {
    mirror_shared_connectors_to_all_system_infra()?;

    if should_skip_gitea_sync() {
        info!("跳过共享 connectors infra Gitea 同步: reason=WARP_STATION_SKIP_GITEA");
        return Ok(());
    }

    sync_to_gitea(commit_message, SystemKind::Wparse, ReleaseGroup::Infra).await?;
    sync_to_gitea(commit_message, SystemKind::Wfusion, ReleaseGroup::Infra).await
}

/// 同步删除到 Gitea
pub async fn sync_delete_to_gitea(
    system: SystemKind,
    rule_type: RuleType,
    file_name: &str,
) -> Result<(), AppError> {
    let commit_message = format!("删除 {} 文件: {}", rule_type.as_ref(), file_name);
    if matches!(rule_type, RuleType::SourceConnect | RuleType::SinkConnect) {
        sync_shared_connectors_to_infra_gitea(&commit_message).await?;
    } else {
        sync_to_gitea(
            &commit_message,
            system,
            ReleaseGroup::from_rule_type(rule_type),
        )
        .await?;
    }
    Ok(())
}

/// 将指定发布版本的仓库内容还原到本地工作区，并提交推送到 Gitea。
///
/// 还原只作用于发布记录包含的仓库分组：models 还原规则配置，infra 还原
/// 设施配置。工作区会先清理 `.git` 以外的内容，避免草稿中新建的未跟踪文件
/// 在还原后继续混入下一次发布。
pub async fn restore_release_to_gitea(
    system: SystemKind,
    version: &str,
    groups: &[ReleaseGroup],
) -> Result<(), AppError> {
    if groups.is_empty() {
        return Err(AppError::validation("还原范围不能为空"));
    }

    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();
    let gitea_client = build_gitea_client(&setting)?;

    // 先检查所有目标仓库是否都能找到版本标签，避免全量还原过程中途失败而只恢复一半。
    for group in groups {
        let project_path = repo_path_for_group(&layout, *group);
        let repo_label = format!(
            "system={}, area={}, version={}",
            system.as_ref(),
            area_from_group(*group).as_ref(),
            version
        );
        ensure_repo_tag(&gitea_client, &project_path, version, &repo_label)?;
    }

    for group in groups {
        let project_path = repo_path_for_group(&layout, *group);
        let repo_label = format!(
            "system={}, area={}, version={}",
            system.as_ref(),
            area_from_group(*group).as_ref(),
            version
        );
        restore_repo_to_tag(&gitea_client, &project_path, version, &repo_label)?;
        sync_named_repo_with_retry(
            &gitea_client,
            &project_path,
            &format!("还原发布配置 {}", version),
            &repo_label,
        )?;
    }

    // connectors 是双系统共享的，Infra 发布 tag 中保存的是当时的镜像；还原后
    // 需要回写共享工作区，再同步到两个系统的 infra Gitea 仓库，避免下一次发布
    // 又被当前共享 connectors 覆盖。
    if groups.contains(&ReleaseGroup::Infra) {
        let shared_root = shared_connectors_root();
        copy_named_entry(&layout.infra_root, &shared_root, DIR_CONNECTORS)?;
        sync_shared_connectors_to_infra_gitea(&format!("还原共享 connectors {}", version)).await?;
    }

    Ok(())
}

/// 将单个本地仓库恢复到指定 tag，保留 `.git` 目录和当前分支。
fn restore_repo_to_tag(
    gitea_client: &GiteaClient,
    project_path: &Path,
    version: &str,
    repo_label: &str,
) -> Result<(), AppError> {
    let local_repo = gitea_client
        .open(project_path)
        .map_err(|e| AppError::internal(format!("打开还原仓库失败: {}", e)))?;

    ensure_repo_tag_with_open_repo(&local_repo, version, repo_label)?;

    clear_repo_worktree_preserving_git(project_path)?;
    local_repo
        .checkout(version)
        .map_err(|e| AppError::internal(format!("检出发布版本失败: {}", e)))?;
    info!("发布配置还原到本地仓库成功: repo={}", repo_label);
    Ok(())
}

/// 确保本地仓库可解析指定发布 tag；本地缺失时先同步远端标签。
fn ensure_repo_tag(
    gitea_client: &GiteaClient,
    project_path: &Path,
    version: &str,
    repo_label: &str,
) -> Result<(), AppError> {
    let local_repo = gitea_client
        .open(project_path)
        .map_err(|e| AppError::internal(format!("打开还原仓库失败: {}", e)))?;
    ensure_repo_tag_with_open_repo(&local_repo, version, repo_label)
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

/// 清空仓库工作区但保留 Git 元数据，以便 tag 检出后得到精确快照。
fn clear_repo_worktree_preserving_git(project_path: &Path) -> Result<(), AppError> {
    for entry in fs::read_dir(project_path).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        if entry.file_name() == std::ffi::OsStr::new(".git") {
            continue;
        }

        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(path).map_err(AppError::internal)?;
        } else {
            fs::remove_file(path).map_err(AppError::internal)?;
        }
    }
    Ok(())
}

fn first_system_connectors_dir() -> Option<PathBuf> {
    all_system_layouts().into_iter().find_map(|layout| {
        let connectors_dir = layout.infra_root.join(DIR_CONNECTORS);
        match dir_has_entries(&connectors_dir) {
            Ok(true) => Some(layout.infra_root),
            _ => None,
        }
    })
}

fn mirror_shared_connectors_into_infra_root(infra_root: &Path) -> Result<(), AppError> {
    let shared_root = shared_connectors_root();
    copy_named_entry(&shared_root, infra_root, DIR_CONNECTORS)
}

fn dir_has_entries(path: &Path) -> Result<bool, AppError> {
    if !path.exists() {
        return Ok(false);
    }

    let mut entries = fs::read_dir(path).map_err(AppError::internal)?;
    Ok(entries
        .next()
        .transpose()
        .map_err(AppError::internal)?
        .is_some())
}

fn copy_named_entry(source_root: &Path, target_root: &Path, name: &str) -> Result<(), AppError> {
    let source = source_root.join(name);
    if !source.exists() {
        return Err(AppError::validation(format!(
            "源目录缺少必要内容: {}",
            source.display()
        )));
    }

    let target = target_root.join(name);
    if target.exists() {
        if target.is_dir() {
            fs::remove_dir_all(&target).map_err(AppError::internal)?;
        } else {
            fs::remove_file(&target).map_err(AppError::internal)?;
        }
    }

    if source.is_dir() {
        copy_dir_recursive(&source, &target)?;
    } else {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(AppError::internal)?;
        }
        fs::copy(&source, &target).map_err(AppError::internal)?;
    }

    Ok(())
}

fn copy_dir_recursive(source_dir: &Path, target_dir: &Path) -> Result<(), AppError> {
    fs::create_dir_all(target_dir).map_err(AppError::internal)?;

    for entry in fs::read_dir(source_dir).map_err(AppError::internal)? {
        let entry = entry.map_err(AppError::internal)?;
        let source_path = entry.path();
        let target_path = target_dir.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent).map_err(AppError::internal)?;
            }
            fs::copy(&source_path, &target_path).map_err(AppError::internal)?;
        }
    }

    Ok(())
}
