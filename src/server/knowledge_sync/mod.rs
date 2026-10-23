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
        info!("知识库同步启动检查开始: startup_sync=false");
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
        info!(
            "知识库同步启动检查完成: asset_configured={}, asset_cron_active={:?}, geo_configured={}, geo_index_ready={}, intel_configured={}, intel_timer_started={}",
            asset_startup_status.configured,
            asset_startup_status.task_active,
            geo_startup_status.configured,
            geo_startup_status.index_ready,
            intel_startup_status.configured,
            intel_startup_status.configured && intel_startup_status.error.is_none()
        );
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

    /// 按类别清空执行历史，不删除同步任务或正式数据。
    pub async fn clear_history(&self, kind: &str) -> Result<u64, AppError> {
        let deleted = match kind {
            "asset" => self
                .asset_service
                .as_ref()
                .ok_or_else(|| AppError::validation("资产同步未配置"))?
                .clear_history()
                .await
                .map_err(AppError::internal)?,
            "intel" => self
                .intel_service
                .as_ref()
                .ok_or_else(|| AppError::validation("情报同步未配置"))?
                .clear_history()
                .await
                .map_err(AppError::internal)?,
            _ => return Err(AppError::validation("无效的同步历史类型")),
        };
        info!("知识库同步历史清空完成: kind={}, deleted={}", kind, deleted);
        Ok(deleted)
    }
}

async fn initialize_intel(
    run_store: Option<&RunStore>,
) -> (Option<IntelSyncService>, IntelSyncStatus) {
    let config = match IntelSyncConfig::from_env() {
        Ok(Some(config)) => config,
        Ok(None) => {
            info!("情报同步未配置，跳过定时器初始化");
            return (None, IntelSyncStatus::unconfigured());
        }
        Err(error) => {
            warn!("情报同步环境配置无效: error={}", error);
            return (None, IntelSyncStatus::unavailable(error.to_string()));
        }
    };
    let Some(run_store) = run_store else {
        warn!("情报同步初始化失败: error=Station SQLite 运行记录库不可用");
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
                "情报同步初始化完成，定时器已启动: interval_seconds={:?}, ip_count={:?}, domain_count={:?}, latest_run_status={:?}, latest_success_at={:?}, startup_sync=false",
                status.interval_seconds,
                status.ip_count,
                status.domain_count,
                status.latest_run.as_ref().map(|run| run.status.as_str()),
                status
                    .latest_success_run
                    .as_ref()
                    .and_then(|run| run.finished_at)
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
        Ok(None) => {
            info!("资产同步未配置，跳过对象和定时任务初始化");
            return (None, AssetSyncStatus::unconfigured());
        }
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
                "资产同步对象初始化完成: database={}, source_verified=true, rows={:?}, cron_task={:?}, cron_active={:?}, schedule={:?}, startup_sync=false",
                status.database,
                status.current_row_count,
                status.task_name,
                status.task_active,
                status.task_schedule
            );
            info!(
                "资产同步历史状态: latest_run_status={:?}, latest_run_finished_at={:?}, latest_success_at={:?}, latest_written_count={:?}",
                status.latest_run_status,
                status.latest_run_finished_at,
                status.latest_success_at,
                status.latest_written_count
            );
            if let Some(error) = &status.error {
                warn!("资产定时任务初始化异常: error={}", error);
            }
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
        Ok(None) => {
            info!("Geo 同步未配置，跳过目标表检查");
            return (None, GeoSyncStatus::unconfigured());
        }
        Err(error) => {
            warn!("Geo 环境配置无效: error={}", error);
            return (None, GeoSyncStatus::unavailable(error.to_string()));
        }
    };
    let Some(run_store) = run_store else {
        warn!("Geo 初始化失败: error=Station SQLite 运行记录库不可用");
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
                "Geo 初始化检查完成: table_exists={}, index_ready={}, rows={:?}, mmdb_version={:?}, latest_run_status={:?}, startup_import=false",
                status.table_exists,
                status.index_ready,
                status.current_row_count,
                status.database_type,
                status.latest_run.as_ref().map(|run| run.status.as_str())
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
    let asset = match &state.asset_service {
        Some(service) => match service.status().await {
            Ok(mut status) => {
                // 初始化失败时仍读取已有任务历史，同时保留真正阻断初始化的错误。
                if let Some(error) = &state.asset_startup_status.error {
                    status.error = Some(error.clone());
                }
                status
            }
            Err(error) => {
                let mut status = state.asset_startup_status.clone();
                if status.error.is_none() {
                    status = AssetSyncStatus::unavailable(format!("{error:#}"));
                }
                status
            }
        },
        None => state.asset_startup_status.clone(),
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
