//! 双系统固定目录布局工具。
//!
//! 只服务当前两个固定系统：
//! - `wparse`
//! - `wfusion`
//!
//! 不尝试做动态系统注册，而是直接返回约定好的仓库路径。

use crate::constants::project::{
    DIR_GITEA_ROOT, REPO_WFUSION_INFRA, REPO_WFUSION_MODELS, REPO_WPARSE_INFRA,
    REPO_WPARSE_MODELS,
};
use crate::server::{RepoLayout, Setting};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use strum::{AsRefStr, Display, EnumString};

/// 测试进程可通过该变量将项目仓库重定向到隔离目录，避免集成测试改写真实仓库。
const TEST_WORKSPACE_ROOT_ENV: &str = "WP_STATION_TEST_WORKSPACE_ROOT";

fn project_workspace_root() -> PathBuf {
    std::env::var_os(TEST_WORKSPACE_ROOT_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| Setting::workspace_root().clone())
}

/// 当前平台支持的固定系统类型。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SystemKind {
    /// 现有 WarpParse 系统。
    Wparse,
    /// 预留接入的 WFusion 系统。
    Wfusion,
}

/// 双仓库中的顶层区域。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString, AsRefStr,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum ProjectArea {
    /// 规则模型类仓库。
    Models,
    /// 基础设施配置类仓库。
    Infra,
}

/// 单个系统在本地工作区中的双仓库布局。
#[derive(Debug, Clone)]
pub struct SystemProjectLayout {
    /// 所属系统标识。
    pub system: SystemKind,
    /// 规则模型仓库根目录。
    pub models_root: PathBuf,
    /// 基础设施配置仓库根目录。
    pub infra_root: PathBuf,
}

impl SystemProjectLayout {
    /// 返回指定区域对应的根目录。
    pub fn area_root(&self, area: ProjectArea) -> PathBuf {
        match area {
            ProjectArea::Models => self.models_root.clone(),
            ProjectArea::Infra => self.infra_root.clone(),
        }
    }

    /// 转换为通用的 `RepoLayout` 结构。
    pub fn as_repo_layout(&self) -> RepoLayout {
        RepoLayout {
            models_root: self.models_root.clone(),
            infra_root: self.infra_root.clone(),
            // connectors 是 infra 仓库的一部分，不再使用跨系统共享工作区。
            connectors_root: self.infra_root.clone(),
        }
    }
}

/// 返回系统与区域对应的固定仓库名。
pub fn repo_name(system: SystemKind, area: ProjectArea) -> &'static str {
    match (system, area) {
        (SystemKind::Wparse, ProjectArea::Models) => REPO_WPARSE_MODELS,
        (SystemKind::Wparse, ProjectArea::Infra) => REPO_WPARSE_INFRA,
        (SystemKind::Wfusion, ProjectArea::Models) => REPO_WFUSION_MODELS,
        (SystemKind::Wfusion, ProjectArea::Infra) => REPO_WFUSION_INFRA,
    }
}

/// 根据 system 解析固定目录布局。
pub fn layout_for_system(system: SystemKind) -> SystemProjectLayout {
    let root = project_workspace_root().join(DIR_GITEA_ROOT);
    SystemProjectLayout {
        system,
        models_root: root.join(repo_name(system, ProjectArea::Models)),
        infra_root: root.join(repo_name(system, ProjectArea::Infra)),
    }
}

/// 枚举当前支持的全部系统布局。
pub fn all_system_layouts() -> Vec<SystemProjectLayout> {
    [SystemKind::Wparse, SystemKind::Wfusion]
        .into_iter()
        .map(layout_for_system)
        .collect()
}
