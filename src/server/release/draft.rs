//! 发布草稿与版本号逻辑。

use crate::constants::release::GROUP_DRAFT;
use crate::db::{
    NewRelease, Release, ReleaseStatus, archive_extra_draft_releases,
    create_release as db_create_release, find_all_releases, find_latest_draft_release,
    find_release_by_id, touch_release_as_draft,
};
use crate::error::AppError;
use crate::utils::SystemKind;

use super::{
    CreateReleaseResponse, release_has_any_published_scope, serialize_stage_summary,
    stage_summary_for_status,
};

/// 解析 `v1.2.3` 形式的版本号，供草稿版本自动递增使用。
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

/// 返回草稿发布使用的固定分组值。
fn draft_release_group() -> String {
    GROUP_DRAFT.to_string()
}

/// 将语义化版本号的 patch 位加一。
fn next_semver_version(version: &str) -> Option<String> {
    let (major, minor, patch) = parse_semver(version)?;
    Some(format!("v{}.{}.{}", major, minor, patch + 1))
}

/// 查询指定系统最近一条非初始化用途的发布记录。
async fn find_latest_non_init_release(system: SystemKind) -> Result<Option<Release>, AppError> {
    let (releases, _) = find_all_releases(Some(system), 1, 1, None, None, None, None).await?;
    Ok(releases.into_iter().next())
}

/// 推导新草稿默认使用的版本号。
async fn next_draft_release_version(system: SystemKind) -> Result<String, AppError> {
    if let Some(existing) = find_latest_draft_release(system).await? {
        return Ok(existing.version);
    }

    let (releases, _) = find_all_releases(Some(system), 1, 1000, None, None, None, None).await?;
    if let Some((major, minor, patch)) = releases
        .iter()
        .filter(|release| release.release_group != GROUP_DRAFT)
        .filter_map(|release| parse_semver(&release.version))
        .max()
    {
        Ok(format!("v{}.{}.{}", major, minor, patch + 1))
    } else {
        Ok("v1.0.1".to_string())
    }
}

/// 确保每个系统始终只有一条可复用的草稿发布记录。
async fn ensure_single_draft_release(system: SystemKind) -> Result<Release, AppError> {
    let stages = serialize_stage_summary(&stage_summary_for_status(&ReleaseStatus::WAIT));
    let draft = if let Some(existing) = find_latest_draft_release(system).await? {
        let refreshed = touch_release_as_draft(
            existing.id,
            &existing.version,
            &draft_release_group(),
            Some(&stages),
        )
        .await?;
        find_release_by_id(refreshed.id)
            .await?
            .ok_or_else(|| AppError::NotFound("草稿发布记录不存在".to_string()))?
    } else {
        let version = match find_latest_non_init_release(system).await? {
            Some(latest) if release_has_any_published_scope(&latest) => {
                next_semver_version(&latest.version).unwrap_or_else(|| "v1.0.1".to_string())
            }
            Some(latest) => latest.version,
            None => next_draft_release_version(system).await?,
        };
        let new_rel = NewRelease {
            system,
            version,
            release_group: draft_release_group(),
            pipeline: None,
            created_by: None,
            stages: Some(stages.clone()),
            status: Some(ReleaseStatus::WAIT),
        };
        let draft_id = db_create_release(new_rel).await?;
        find_release_by_id(draft_id)
            .await?
            .ok_or_else(|| AppError::NotFound("草稿发布记录不存在".to_string()))?
    };

    archive_extra_draft_releases(system, draft.id).await?;

    Ok(draft)
}

/// 在保存配置后刷新唯一草稿记录。
pub async fn refresh_draft_release_logic(
    system: SystemKind,
    _note: Option<&str>,
) -> Result<Release, AppError> {
    ensure_single_draft_release(system).await
}

/// 创建或刷新唯一草稿发布记录。
pub async fn create_release_logic(
    system: SystemKind,
    _pipeline: Option<String>,
    _note: Option<String>,
) -> Result<CreateReleaseResponse, AppError> {
    async {
        let release = ensure_single_draft_release(system).await?;
        Ok::<_, AppError>(CreateReleaseResponse {
            id: release.id,
            success: true,
        })
    }
    .await
}
