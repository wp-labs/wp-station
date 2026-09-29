//! Gitea 仓库初始化与准备逻辑。

use std::path::Path;

use crate::constants::gitea::REPO_BASELINE_TAG;
use crate::db::ReleaseGroup;
use crate::error::AppError;
use crate::server::{RepoStartupStrategy, Setting};
use crate::utils::{SystemKind, all_system_layouts, layout_for_system, repo_name};
use gitea::GiteaClient;

/// 初始化双仓库 Gitea 仓库和基线 tag。
///
/// 当前默认先初始化 `wparse` 两个主仓库，`wfusion` 仓库由统一检查流程补齐。
pub async fn init_gitea_repo() -> Result<(), AppError> {
    if super::should_skip_gitea_sync() {
        info!("跳过 Gitea 仓库初始化: reason=WARP_STATION_SKIP_GITEA");
        return Ok(());
    }

    let setting = Setting::load();
    let layout = layout_for_system(SystemKind::Wparse).as_repo_layout();
    let gitea_client = super::build_gitea_client(&setting)
        .map_err(|e| AppError::internal(format!("无法连接 Gitea: {}", e)))?;

    crate::utils::init_default_configs_to_models(layout.models_root.to_string_lossy().as_ref())
        .map_err(|e| AppError::internal(format!("初始化 models 默认配置失败: {}", e)))?;
    crate::utils::init_default_configs_to_infra(layout.infra_root.to_string_lossy().as_ref())
        .map_err(|e| AppError::internal(format!("初始化 infra 默认配置失败: {}", e)))?;
    init_single_repo(
        &setting,
        &gitea_client,
        SystemKind::Wparse,
        ReleaseGroup::Models,
        &layout.models_root,
    )
    .await?;
    init_single_repo(
        &setting,
        &gitea_client,
        SystemKind::Wparse,
        ReleaseGroup::Infra,
        &layout.infra_root,
    )
    .await?;

    Ok(())
}

/// 按启动策略确保本地双仓库与 Gitea 远端仓库处于可用状态。
pub async fn ensure_project_repositories() -> Result<(), AppError> {
    if super::should_skip_gitea_sync() {
        info!("跳过 Gitea 仓库检查: reason=WARP_STATION_SKIP_GITEA");
        return Ok(());
    }

    let setting = Setting::load();
    let gitea_client = super::build_gitea_client(&setting)
        .map_err(|e| AppError::internal(format!("无法连接 Gitea: {}", e)))?;

    for system_layout in all_system_layouts() {
        for group in [ReleaseGroup::Models, ReleaseGroup::Infra] {
            let project_path = super::repo_path_for_group(&system_layout.as_repo_layout(), group);
            ensure_single_project_repository(
                &setting,
                &gitea_client,
                system_layout.system,
                group,
                &project_path,
            )
            .await?;
        }
    }
    Ok(())
}

/// 确保单个本地仓库与远端仓库状态满足启动要求。
async fn ensure_single_project_repository(
    setting: &Setting,
    gitea_client: &GiteaClient,
    system: SystemKind,
    group: ReleaseGroup,
    project_path: &Path,
) -> Result<(), AppError> {
    let repo_name = repo_name(system, super::area_from_group(group));
    let local_has_repo = project_path.join(".git").is_dir();
    let remote_repo = gitea_client.get_repo(repo_name).await.map_err(|e| {
        AppError::internal(format!(
            "查询远程仓库失败: repo_name={}, error={}",
            repo_name, e
        ))
    })?;
    let remote_exists = remote_repo.is_some();
    let remote_has_data = remote_repo
        .as_ref()
        .map(|repo| !repo.empty)
        .unwrap_or(false);

    info!(
        "检查项目仓库: group={}, strategy={:?}, local_has_repo={}, remote_exists={}, remote_has_data={}",
        group.as_ref(),
        setting.gitea.repo_startup_strategy,
        local_has_repo,
        remote_exists,
        remote_has_data
    );

    match (local_has_repo, remote_has_data) {
        (false, true) => {
            let repo = remote_repo.expect("remote_has_data implies remote repo exists");
            clone_remote_over_local(gitea_client, &repo.clone_url, project_path, group.as_ref())
                .await
        }
        (false, false) => {
            init_default_configs_for_group(system, group)?;
            init_single_repo(setting, gitea_client, system, group, project_path).await
        }
        (true, false) => {
            init_default_configs_for_group(system, group)?;
            prepare_single_repo(setting, gitea_client, system, group, project_path, false).await?;
            force_push_local_repo(gitea_client, project_path, group.as_ref())
        }
        (true, true) => match setting.gitea.repo_startup_strategy {
            RepoStartupStrategy::Gitea => {
                let repo = remote_repo.expect("remote_has_data implies remote repo exists");
                clone_remote_over_local(gitea_client, &repo.clone_url, project_path, group.as_ref())
                    .await
            }
            RepoStartupStrategy::Local => {
                init_default_configs_for_group(system, group)?;
                prepare_single_repo(setting, gitea_client, system, group, project_path, false)
                    .await?;
                force_push_local_repo(gitea_client, project_path, group.as_ref())
            }
        },
    }
}

/// 按 system/group 补齐仓库缺失的默认目录或默认内容。
fn init_default_configs_for_group(system: SystemKind, group: ReleaseGroup) -> Result<(), AppError> {
    let layout = layout_for_system(system);
    match (system, group) {
        (_, ReleaseGroup::Models) => crate::utils::init_default_configs_to_models_for_system(
            system,
            layout.models_root.to_string_lossy().as_ref(),
        )
        .map_err(|e| AppError::internal(format!("初始化 models 默认配置失败: {}", e))),
        (_, ReleaseGroup::Infra) => crate::utils::init_default_configs_to_infra_for_system(
            system,
            layout.infra_root.to_string_lossy().as_ref(),
        )
        .map_err(|e| AppError::internal(format!("初始化 infra 默认配置失败: {}", e))),
    }
}

/// 在本地缺仓库而远端已有数据时，用远端覆盖本地。
async fn clone_remote_over_local(
    gitea_client: &GiteaClient,
    clone_url: &str,
    project_path: &Path,
    repo_label: &str,
) -> Result<(), AppError> {
    if project_path.exists() {
        std::fs::remove_dir_all(project_path).map_err(|e| {
            AppError::internal(format!(
                "清理本地项目目录失败: path={}, error={}",
                project_path.display(),
                e
            ))
        })?;
    }
    if let Some(parent) = project_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            AppError::internal(format!(
                "创建项目父目录失败: path={}, error={}",
                parent.display(),
                e
            ))
        })?;
    }

    gitea_client
        .clone_existing(clone_url, project_path)
        .await
        .map_err(|e| {
            AppError::internal(format!(
                "从 Gitea clone 项目仓库失败: group={}, error={}",
                repo_label, e
            ))
        })?;
    info!(
        "已从 Gitea 恢复项目仓库: group={}, path={}",
        repo_label,
        project_path.display()
    );
    Ok(())
}

/// 用本地仓库内容强制覆盖远端。
fn force_push_local_repo(
    gitea_client: &GiteaClient,
    project_path: &Path,
    repo_label: &str,
) -> Result<(), AppError> {
    let local_repo = gitea_client.open(project_path).map_err(|e| {
        AppError::internal(format!(
            "打开本地仓库失败: group={}, error={}",
            repo_label, e
        ))
    })?;
    local_repo.force_push().map_err(|e| {
        AppError::internal(format!(
            "本地仓库强制覆盖 Gitea 失败: group={}, error={}",
            repo_label, e
        ))
    })?;
    info!("已用本地仓库强制覆盖 Gitea: group={}", repo_label);
    Ok(())
}

/// 初始化单个仓库并在首次准备时推送主分支。
async fn init_single_repo(
    setting: &Setting,
    gitea_client: &GiteaClient,
    system: SystemKind,
    group: ReleaseGroup,
    project_path: &Path,
) -> Result<(), AppError> {
    prepare_single_repo(setting, gitea_client, system, group, project_path, true).await
}

/// 准备单个仓库的 git 元信息、远端和基线 tag。
async fn prepare_single_repo(
    setting: &Setting,
    gitea_client: &GiteaClient,
    system: SystemKind,
    group: ReleaseGroup,
    project_path: &Path,
    push_main: bool,
) -> Result<(), AppError> {
    let repo_name = repo_name(system, super::area_from_group(group));
    prepare_named_repo(setting, gitea_client, repo_name, project_path, push_main).await
}

/// 按仓库名准备本地 git 元信息、远端和基线 tag。
async fn prepare_named_repo(
    setting: &Setting,
    gitea_client: &GiteaClient,
    repo_name: &str,
    project_path: &Path,
    push_main: bool,
) -> Result<(), AppError> {
    info!(
        "初始化 Gitea 仓库: repo_name={}, path={}",
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

    run_git(&["add", "."], project_path, "git add")?;

    let status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查 git 状态失败: {}", e)))?;

    let has_changes = !status_output.stdout.is_empty();
    let has_head = repo_has_head(project_path)?;

    if has_changes {
        run_git(&["commit", "-m", "初始化配置"], project_path, "git commit")?;
    } else if !has_head {
        // 目录里没有可提交文件时，仍创建空提交，确保后续基线 tag 有稳定的 HEAD。
        run_git(
            &["commit", "--allow-empty", "-m", "初始化空仓库"],
            project_path,
            "git commit --allow-empty",
        )?;
    }

    if !repo_has_head(project_path)? {
        run_git(
            &["commit", "--allow-empty", "-m", "初始化空仓库"],
            project_path,
            "git commit --allow-empty",
        )?;
    }

    if push_main {
        run_git(&["push", &auth_url, "main"], project_path, "git push")?;
    }

    ensure_tag_exists(project_path, REPO_BASELINE_TAG)?;
    if push_main {
        push_tag_if_needed(project_path, &auth_url, REPO_BASELINE_TAG)?;
    }

    Ok(())
}

/// 判断当前仓库是否已经存在至少一个提交。
fn repo_has_head(project_path: &Path) -> Result<bool, AppError> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(project_path)
        .output()
        .map_err(|e| AppError::internal(format!("检查 HEAD 失败: {}", e)))?;

    Ok(output.status.success())
}

/// 构造带认证信息的远端地址。
fn build_auth_url(url: &str, username: &str, password: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        format!("http://{}:{}@{}", username, password, rest)
    } else if let Some(rest) = url.strip_prefix("https://") {
        format!("https://{}:{}@{}", username, password, rest)
    } else {
        url.to_string()
    }
}

/// 运行简单 git 命令并统一包装错误。
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

/// 确保本地仓库包含指定 tag。
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

/// 仅在远端缺少 tag 时推送。
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
