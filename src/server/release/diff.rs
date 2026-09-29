//! 发布差异计算逻辑。
//!
//! 负责按发布范围读取 Gitea diff，并把结果整理为前端可直接展示的结构。

use std::path::Path;

use crate::constants::release::GROUP_DRAFT;
use crate::db::{Release, ReleaseGroup, find_latest_passed_release_by_group, find_release_by_id};
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::{SystemKind, layout_for_system};

use super::stage::parse_release_status;
use super::{
    DiffStats, ReleaseDiffFileInfo, ReleaseDiffGroupSummary, ReleaseDiffResponse, ReleaseStatus,
    all_release_groups, release_contains_group, release_system,
};

/// 单个发布分组的 diff 摘要与文件列表。
#[derive(Clone)]
struct ReleaseDiffGroupData {
    summary: ReleaseDiffGroupSummary,
    files: Vec<ReleaseDiffFileInfo>,
}

/// 获取版本差异（与上一个版本的 git diff），按文件分批返回。
pub async fn get_release_diff_logic(
    id: i32,
    offset: usize,
    limit: usize,
) -> Result<ReleaseDiffResponse, AppError> {
    let release = match find_release_by_id(id).await? {
        Some(rel) => rel,
        None => return Err(AppError::NotFound("发布记录不存在".to_string())),
    };

    let (offset, limit) = normalize_release_diff_window(offset, limit);
    let diff_groups = collect_release_diff_groups_for_release(&release).await?;
    let merged_files = diff_groups
        .iter()
        .flat_map(|group| group.files.iter().cloned())
        .collect::<Vec<_>>();
    let merged_stats = diff_groups.iter().fold(
        DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
        |mut acc, item| {
            acc.files_changed += item.summary.stats.files_changed;
            acc.insertions += item.summary.stats.insertions;
            acc.deletions += item.summary.stats.deletions;
            acc
        },
    );
    let total_files = merged_files.len();
    let files = merged_files
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    let loaded_count = offset.saturating_add(files.len());

    Ok(ReleaseDiffResponse {
        groups: diff_groups.into_iter().map(|group| group.summary).collect(),
        files,
        stats: merged_stats,
        total_files,
        offset,
        limit,
        has_more: loaded_count < total_files,
    })
}

/// 归一化分页窗口，避免前端传入过大的 diff 明细范围。
fn normalize_release_diff_window(offset: usize, limit: usize) -> (usize, usize) {
    const DEFAULT_RELEASE_DIFF_LIMIT: usize = 10;
    const MAX_RELEASE_DIFF_LIMIT: usize = 50;

    let normalized_limit = if limit == 0 {
        DEFAULT_RELEASE_DIFF_LIMIT
    } else {
        limit.min(MAX_RELEASE_DIFF_LIMIT)
    };

    (offset, normalized_limit)
}

/// 根据发布记录收集可对比的分组差异摘要。
async fn collect_release_diff_groups_for_release(
    release: &Release,
) -> Result<Vec<ReleaseDiffGroupData>, AppError> {
    let release_status = parse_release_status(release)?;
    let system = release_system(release)?;
    if release_status == ReleaseStatus::WAIT {
        return collect_draft_diff_groups(system).await;
    }

    let mut groups = Vec::new();
    for group in all_release_groups() {
        groups.push(
            collect_release_diff_for_group(
                system,
                group.as_ref(),
                if release_contains_group(&release.release_group, group) {
                    Some(&release.version)
                } else {
                    None
                },
                Some(release.id),
            )
            .await?,
        );
    }
    Ok(groups)
}

/// 草稿发布场景下收集 models / infra 两个固定分组的差异。
async fn collect_draft_diff_groups(
    system: SystemKind,
) -> Result<Vec<ReleaseDiffGroupData>, AppError> {
    let models_diff =
        collect_release_diff_for_group(system, ReleaseGroup::Models.as_ref(), None, None).await?;
    let infra_diff =
        collect_release_diff_for_group(system, ReleaseGroup::Infra.as_ref(), None, None).await?;
    Ok(vec![models_diff, infra_diff])
}

/// 计算单个发布分组相对上一基线版本的差异明细。
async fn collect_release_diff_for_group(
    system: SystemKind,
    release_group: &str,
    version: Option<&str>,
    exclude_release_id: Option<i32>,
) -> Result<ReleaseDiffGroupData, AppError> {
    use gitea::{DiffResultWithFiles, GiteaClient, GiteaConfig};

    let parsed_group = ReleaseGroup::parse(release_group)?;
    let setting = Setting::load();
    let layout = layout_for_system(system).as_repo_layout();

    let gitea_config = GiteaConfig::new(
        setting.gitea.base_url.clone(),
        setting.gitea.username.clone(),
        setting.gitea.password.clone(),
    )
    .with_branch("main".to_string());

    let gitea_client = GiteaClient::new(gitea_config).map_err(AppError::git)?;
    let repo_targets = match parsed_group {
        ReleaseGroup::Models => vec![layout.models_root],
        ReleaseGroup::Infra => vec![layout.infra_root],
    };

    let previous_release =
        find_latest_passed_release_by_group(system, release_group, exclude_release_id).await?;
    let previous_version = previous_release.as_ref().map(|rel| rel.version.clone());

    let empty_diff = || DiffResultWithFiles {
        files: vec![],
        stats: gitea::DiffStats {
            files_changed: 0,
            insertions: 0,
            deletions: 0,
        },
    };

    let mut merged_files = Vec::new();
    for project_path in repo_targets {
        let diff_result = if let Some(curr_version) = version {
            match gitea_client.diff_with_previous_version(&project_path, curr_version) {
                Ok(result) => result,
                Err(e) => {
                    warn!(
                        "获取发布版本差异失败: version={}, release_group={}, repo={}, error={}",
                        curr_version,
                        release_group,
                        project_path.display(),
                        e
                    );
                    empty_diff()
                }
            }
        } else {
            match gitea_client.diff_with_newest_tag(&project_path) {
                Ok(result) => result,
                Err(e) => {
                    warn!(
                        "获取草稿仓库差异失败: release_group={}, repo={}, error={}",
                        release_group,
                        project_path.display(),
                        e
                    );
                    empty_diff()
                }
            }
        };

        merged_files.extend(diff_result.files);
    }

    let files = filter_release_diff_files(merged_files)
        .into_iter()
        .map(|f| ReleaseDiffFileInfo {
            release_group: release_group.to_string(),
            file_path: f.file_path,
            old_path: f.old_path,
            change_type: f.change_type,
            diff_text: f.diff_text,
        })
        .collect::<Vec<_>>();
    let stats = diff_stats_from_files(&files);

    Ok(ReleaseDiffGroupData {
        summary: ReleaseDiffGroupSummary {
            release_group: release_group.to_string(),
            title: crate::constants::release::group_title(parsed_group.as_ref()).to_string(),
            current_version: version.unwrap_or(GROUP_DRAFT).to_string(),
            previous_version,
            stats,
            total_files: files.len(),
        },
        files,
    })
}

/// 判断某个文件路径是否应从发布 diff 中过滤掉。
fn should_ignore_release_diff_path(path: &str) -> bool {
    let path = Path::new(path);
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("README.md"))
        .unwrap_or(false)
    {
        return true;
    }

    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map(|name| name == ".run")
            .unwrap_or(false)
    })
}

/// 过滤不需要展示给前端的 diff 文件项。
fn filter_release_diff_files(files: Vec<gitea::FileDiffInfo>) -> Vec<gitea::FileDiffInfo> {
    files
        .into_iter()
        .filter(|file| {
            !should_ignore_release_diff_path(&file.file_path)
                && file
                    .old_path
                    .as_deref()
                    .map(|path| !should_ignore_release_diff_path(path))
                    .unwrap_or(true)
        })
        .collect()
}

/// 从文件差异列表中汇总文件数、增删行统计。
fn diff_stats_from_files(files: &[ReleaseDiffFileInfo]) -> DiffStats {
    let mut stats = DiffStats {
        files_changed: files.len(),
        insertions: 0,
        deletions: 0,
    };

    for file in files {
        for line in file.diff_text.lines() {
            if line.starts_with("+++") || line.starts_with("---") {
                continue;
            }
            if line.starts_with('+') {
                stats.insertions += 1;
            } else if line.starts_with('-') {
                stats.deletions += 1;
            }
        }
    }

    stats
}
