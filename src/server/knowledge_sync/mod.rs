//! 知识数据同步任务的初始化和状态编排。

use serde::Serialize;
use wp_station_knowledge_sync::geo::{
    GeoRun, GeoSyncConfig, GeoSyncService, GeoSyncStatus, MmdbFile,
};
use wp_station_knowledge_sync::intel::{
    IntelRun, IntelSyncConfig, IntelSyncService, IntelSyncStatus,
};
use wp_station_knowledge_sync::{
    AssetRun, AssetSyncConfig, AssetSyncService, AssetSyncStatus, RunHistory, RunStore,
    open_run_store,
};

use crate::error::AppError;

/// 进程级知识同步状态。
#[derive(Clone)]
pub struct KnowledgeSyncState {
    asset_service: Option<AssetSyncService>,
    asset_startup_status: AssetSyncStatus,
    geo_service: Option<GeoSyncService>,
    geo_startup_status: GeoSyncStatus,
    intel_service: Option<IntelSyncService>,
    intel_startup_status: IntelSyncStatus,
}

impl KnowledgeSyncState {
    /// 启动时分别初始化资产与 Geo，不让单项故障阻断 Station。
    pub async fn initialize(station_sqlite_url: Option<&str>) -> Self {
        let (asset_service, asset_startup_status) = initialize_asset().await;
        let run_store = match station_sqlite_url {
            Some(url) => match open_run_store(url).await {
                Ok(store) => Some(store),
                Err(error) => {
                    warn!("连接 Station 同步运行记录库失败: error={:#}", error);
                    None
                }
            },
            None => None,
        };
        if let Some(store) = &run_store {
            let mut targets = Vec::new();
            for name in [
                "WP_KSYNC_TARGET_URL",
                "WP_KSYNC_GEO_TARGET_URL",
                "WP_KSYNC_INTEL_TARGET_URL",
                "WP_KSYNC_ASSET_TARGET_URL",
            ] {
                if let Ok(url) = std::env::var(name)
                    && !url.trim().is_empty()
                    && !targets.contains(&url)
                {
                    targets.push(url);
                }
            }
            for url in targets {
                if let Err(error) =
                    wp_station_knowledge_sync::legacy::migrate_legacy_runs(&url, store).await
                {
                    warn!("迁移旧知识同步运行历史失败: error={:#}", error);
                }
            }
        }
        let (geo_service, geo_startup_status) = initialize_geo(run_store.as_ref()).await;
        let (intel_service, intel_startup_status) = initialize_intel(run_store.as_ref()).await;
        Self {
            asset_service,
            asset_startup_status,
            geo_service,
            geo_startup_status,
            intel_service,
            intel_startup_status,
        }
    }

    /// 列出固定目录中的 MMDB 文件。
    pub fn mmdb_files(&self) -> Result<Vec<MmdbFile>, AppError> {
        match self.geo_service.as_ref() {
            Some(service) => service.files().map_err(AppError::internal),
            None => Ok(Vec::new()),
        }
    }

    /// 创建并启动一次异步 MMDB 导入。
    pub async fn start_geo_import(&self, file_name: &str) -> Result<GeoRun, AppError> {
        let service = self
            .geo_service
            .as_ref()
            .ok_or_else(|| AppError::validation("Geo 导入未配置"))?;
        let run = service
            .start_import(file_name)
            .await
            .map_err(|error| AppError::validation(error.to_string()))?;
        let task_service = service.clone();
        let task_run = run.clone();
        tokio::spawn(async move { task_service.execute_import(task_run).await });
        Ok(run)
    }

    /// 查询导入任务。
    pub async fn geo_run(&self, id: i64) -> Result<GeoRun, AppError> {
        let service = self
            .geo_service
            .as_ref()
            .ok_or_else(|| AppError::validation("Geo 导入未配置"))?;
        service
            .run(id)
            .await
            .map_err(AppError::internal)?
            .ok_or_else(|| AppError::not_found(format!("Geo 导入任务 {id}")))
    }

    /// 直接查询 pg_cron 中的资产任务历史。
    pub async fn asset_history(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<RunHistory<AssetRun>, AppError> {
        let service = self
            .asset_service
            .as_ref()
            .ok_or_else(|| AppError::validation("资产同步未配置"))?;
        service
            .history(limit, offset)
            .await
            .map_err(AppError::internal)
    }

    /// 查询目标库中持久化的 Geo 导入历史。
    pub async fn geo_history(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<RunHistory<GeoRun>, AppError> {
        let service = self
            .geo_service
            .as_ref()
            .ok_or_else(|| AppError::validation("Geo 导入未配置"))?;
        service
            .history(limit, offset)
            .await
            .map_err(AppError::internal)
    }

    /// 查询目标库中的情报定时同步历史。
    pub async fn intel_history(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<RunHistory<IntelRun>, AppError> {
        let service = self
            .intel_service
            .as_ref()
            .ok_or_else(|| AppError::validation("情报同步未配置"))?;
        service
            .history(limit, offset)
            .await
            .map_err(AppError::internal)
    }
}

async fn initialize_intel(
    run_store: Option<&RunStore>,
) -> (Option<IntelSyncService>, IntelSyncStatus) {
    let config = match IntelSyncConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return (None, IntelSyncStatus::unconfigured()),
        Err(error) => {
            warn!("情报同步环境配置无效: error={}", error);
            return (None, IntelSyncStatus::unavailable(error.to_string()));
        }
    };
    let Some(run_store) = run_store else {
        return (
            None,
            IntelSyncStatus::unavailable("Station SQLite 运行记录库不可用"),
        );
    };
    let service = match IntelSyncService::connect(config, run_store.clone()).await {
        Ok(service) => service,
        Err(error) => {
            warn!("连接情报同步服务失败: error={:#}", error);
            return (None, IntelSyncStatus::unavailable(error.to_string()));
        }
    };
    match service.initialize().await {
        Ok(status) => {
            service.spawn_timer();
            info!(
                "情报同步定时器已创建: interval_seconds={:?}",
                status.interval_seconds
            );
            (Some(service), status)
        }
        Err(error) => {
            warn!("情报同步初始化失败: error={:#}", error);
            (
                Some(service),
                IntelSyncStatus::unavailable(error.to_string()),
            )
        }
    }
}

async fn initialize_asset() -> (Option<AssetSyncService>, AssetSyncStatus) {
    let config = match AssetSyncConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return (None, AssetSyncStatus::unconfigured()),
        Err(error) => {
            warn!("资产同步环境配置无效: error={}", error);
            return (None, AssetSyncStatus::unavailable(error.to_string()));
        }
    };
    let service = match AssetSyncService::connect(config).await {
        Ok(service) => service,
        Err(error) => {
            warn!("连接资产同步数据库失败: error={:#}", error);
            return (None, AssetSyncStatus::unavailable(error.to_string()));
        }
    };
    match service.initialize().await {
        Ok(status) => {
            info!(
                "资产同步初始化完成: database={}, rows={:?}, cron_active={:?}",
                status.database, status.current_row_count, status.task_active
            );
            (Some(service), status)
        }
        Err(error) => {
            warn!("资产同步初始化失败: error={:#}", error);
            (
                Some(service),
                AssetSyncStatus::unavailable(format!("{error:#}")),
            )
        }
    }
}

async fn initialize_geo(run_store: Option<&RunStore>) -> (Option<GeoSyncService>, GeoSyncStatus) {
    let config = match GeoSyncConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => return (None, GeoSyncStatus::unconfigured()),
        Err(error) => {
            warn!("Geo 环境配置无效: error={}", error);
            return (None, GeoSyncStatus::unavailable(error.to_string()));
        }
    };
    let Some(run_store) = run_store else {
        return (
            None,
            GeoSyncStatus::unavailable("Station SQLite 运行记录库不可用"),
        );
    };
    let service = match GeoSyncService::connect(config, run_store.clone()).await {
        Ok(service) => service,
        Err(error) => {
            warn!("连接 Geo 目标数据库失败: error={:#}", error);
            return (None, GeoSyncStatus::unavailable(error.to_string()));
        }
    };
    match service.initialize().await {
        Ok(status) => {
            info!(
                "Geo 状态检查完成: table_exists={}, rows={:?}",
                status.table_exists, status.current_row_count
            );
            (Some(service), status)
        }
        Err(error) => {
            warn!("Geo 状态检查失败: error={:#}", error);
            (
                Some(service),
                GeoSyncStatus::unavailable(format!("{error:#}")),
            )
        }
    }
}

/// 知识同步状态接口响应。
#[derive(Debug, Serialize)]
pub struct KnowledgeSyncStatusResponse {
    pub asset: AssetSyncStatus,
    pub geo: GeoSyncStatus,
    pub intel: IntelSyncStatus,
}

/// 返回资产与 Geo 状态。
pub async fn get_knowledge_sync_status_logic(
    state: &KnowledgeSyncState,
) -> KnowledgeSyncStatusResponse {
    let asset = if state.asset_startup_status.error.is_some()
        && state.asset_startup_status.current_row_count.is_none()
    {
        state.asset_startup_status.clone()
    } else {
        match &state.asset_service {
            Some(service) => service
                .status()
                .await
                .unwrap_or_else(|error| AssetSyncStatus::unavailable(error.to_string())),
            None => state.asset_startup_status.clone(),
        }
    };
    let geo = if state.geo_startup_status.error.is_some() {
        state.geo_startup_status.clone()
    } else {
        match &state.geo_service {
            Some(service) => service
                .status()
                .await
                .unwrap_or_else(|error| GeoSyncStatus::unavailable(error.to_string())),
            None => state.geo_startup_status.clone(),
        }
    };
    let intel = if state.intel_startup_status.error.is_some() {
        state.intel_startup_status.clone()
    } else {
        match &state.intel_service {
            Some(service) => service
                .status()
                .await
                .unwrap_or_else(|error| IntelSyncStatus::unavailable(error.to_string())),
            None => state.intel_startup_status.clone(),
        }
    };
    KnowledgeSyncStatusResponse { asset, geo, intel }
}
