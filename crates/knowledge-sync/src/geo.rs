//! MMDB 固定目录扫描、Geo 数据导入与运行状态。

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use ipnetwork::IpNetwork;
use maxminddb::{Reader, geoip2};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use sqlx::postgres::PgPoolCopyExt;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::{ConnectOptions, Row};
use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tracing::{info, warn};

use crate::RunHistory;

const GEO_CACHE_LIMIT: usize = 32_768;

/// Geo 目标库和 MMDB 文件目录。
#[derive(Clone)]
pub struct GeoSyncConfig {
    pub target_database_url: String,
    pub mmdb_dir: PathBuf,
    pub work_dir: PathBuf,
    pub max_rows: Option<u64>,
}

impl GeoSyncConfig {
    /// 目标库未注入时不启用 Geo 功能；相对目录以 Station 启动目录为基准。
    pub fn from_env() -> Result<Option<Self>> {
        let target_database_url = env::var("WP_KSYNC_TARGET_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| env::var("WP_KSYNC_GEO_TARGET_URL").ok())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| env::var("WP_KSYNC_ASSET_TARGET_URL").ok())
            .filter(|value| !value.trim().is_empty());
        let Some(target_database_url) = target_database_url else {
            return Ok(None);
        };
        let configured_dir = env::var("WP_KSYNC_MMDB_DIR")
            .ok()
            .filter(|value| !value.trim().is_empty());
        let mmdb_dir = resolve_dir(
            configured_dir
                .clone()
                .unwrap_or_else(|| "/app/mmdb".to_string()),
        )?;
        if !mmdb_dir.is_dir() {
            if configured_dir.is_none() {
                return Ok(None);
            }
            bail!("配置的 MMDB 目录不存在或不是目录");
        }
        let work_dir = resolve_dir(env::var("WP_KSYNC_MMDB_WORK_DIR").unwrap_or_else(|_| {
            if Path::new("/app/tmp/sandbox").is_dir() {
                "/app/tmp/sandbox".to_string()
            } else {
                env::temp_dir().to_string_lossy().to_string()
            }
        }))?;
        let max_rows = env::var("WP_KSYNC_GEO_MAX_ROWS")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .parse::<u64>()
                    .context("WP_KSYNC_GEO_MAX_ROWS 必须是正整数")
            })
            .transpose()?;
        if max_rows == Some(0) {
            bail!("WP_KSYNC_GEO_MAX_ROWS 必须大于 0");
        }
        Ok(Some(Self {
            target_database_url,
            mmdb_dir,
            work_dir,
            max_rows,
        }))
    }
}

fn resolve_dir(value: String) -> Result<PathBuf> {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(env::current_dir()
            .context("读取 Station 启动目录失败")?
            .join(path))
    }
}

/// 映射目录中可选择的 MMDB 文件。
#[derive(Clone, Debug, Serialize)]
pub struct MmdbFile {
    pub name: String,
    pub size: u64,
    pub modified_at: DateTime<Utc>,
}

/// Geo 表、版本与最近导入状态。
#[derive(Clone, Debug, Serialize)]
pub struct GeoSyncStatus {
    pub configured: bool,
    pub sample_limit: Option<u64>,
    pub table_exists: bool,
    pub index_ready: bool,
    pub current_row_count: Option<i64>,
    pub source_row_count: Option<i64>,
    pub ipv6_source_row_count: Option<i64>,
    pub latest_data_at: Option<DateTime<Utc>>,
    pub file_name: Option<String>,
    pub file_hash: Option<String>,
    pub database_type: Option<String>,
    pub build_epoch: Option<i64>,
    pub latest_run: Option<GeoRun>,
    pub error: Option<String>,
}

impl GeoSyncStatus {
    /// 未设置目标库时的状态。
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            sample_limit: None,
            table_exists: false,
            index_ready: false,
            current_row_count: None,
            source_row_count: None,
            ipv6_source_row_count: None,
            latest_data_at: None,
            file_name: None,
            file_hash: None,
            database_type: None,
            build_epoch: None,
            latest_run: None,
            error: None,
        }
    }

    /// 连接或初始化失败时保留错误，供页面展示。
    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            configured: true,
            error: Some(error.into()),
            ..Self::unconfigured()
        }
    }
}

/// 一次 MMDB 导入的持久化运行摘要。
#[derive(Clone, Debug, Serialize)]
pub struct GeoRun {
    pub id: i64,
    pub status: String,
    pub stage: String,
    pub file_name: String,
    pub file_hash: Option<String>,
    pub row_count: Option<i64>,
    pub source_row_count: Option<i64>,
    pub ipv6_source_row_count: Option<i64>,
    pub row_limit: Option<i64>,
    pub progress_rows: i64,
    pub progress_percent: Option<i32>,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// Geo 导入服务。
#[derive(Clone)]
pub struct GeoSyncService {
    pool: PgPool,
    runs: SqlitePool,
    config: GeoSyncConfig,
}

impl GeoSyncService {
    /// 建立目标库连接池。
    pub async fn connect(config: GeoSyncConfig, runs: SqlitePool) -> Result<Self> {
        let options = PgConnectOptions::from_str(&config.target_database_url)
            .context("解析 Geo 目标 PostgreSQL URL 失败")?
            .log_statements(log::LevelFilter::Off)
            .log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(10));
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(options)
            .await
            .context("连接 Geo 目标 PostgreSQL 失败")?;
        Ok(Self { pool, runs, config })
    }

    /// 启动时创建 Geo 正式表及查询索引，并恢复中断记录。
    pub async fn initialize(&self) -> Result<GeoSyncStatus> {
        self.ensure_target_table().await?;
        self.reconcile_committed_import().await?;
        sqlx::query(
            "UPDATE geo_sync_runs SET status='interrupted', stage='interrupted',
             error='Station 重启导致导入中断', finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE status='running'",
        )
        .execute(&self.runs)
        .await
        .context("恢复中断的 Geo 导入记录失败")?;
        self.cleanup_abandoned_stages().await?;
        self.status().await
    }

    // PostgreSQL 与 Station SQLite 无法共用事务；重启后只修复已实际提交的导入。
    async fn reconcile_committed_import(&self) -> Result<()> {
        let candidate = sqlx::query(
            "SELECT id, started_at, progress_rows FROM geo_sync_runs \
             WHERE status IN ('running','failed') AND file_hash IS NOT NULL \
             ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.runs)
        .await?;
        let Some(candidate) = candidate else {
            return Ok(());
        };
        let id: i64 = candidate.try_get("id")?;
        let started_at: DateTime<Utc> = candidate.try_get("started_at")?;
        let exported: i64 = candidate.try_get("progress_rows")?;
        let row = sqlx::query(
            "SELECT count(*)::bigint AS total, min(imported_at) AS first_imported_at \
             FROM public.ip_geo",
        )
        .fetch_one(&self.pool)
        .await?;
        let total: i64 = row.try_get("total")?;
        let first_imported_at: Option<DateTime<Utc>> = row.try_get("first_imported_at")?;
        if total > 0
            && total == exported
            && first_imported_at.is_some_and(|time| time >= started_at)
        {
            sqlx::query(
                "UPDATE geo_sync_runs SET status='succeeded', stage='completed', \
                 row_count=$2, progress_percent=100, error=NULL, \
                 finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1",
            )
            .bind(id)
            .bind(total)
            .execute(&self.runs)
            .await
            .context("恢复已提交的 Geo 导入摘要失败")?;
        }
        Ok(())
    }

    async fn ensure_target_table(&self) -> Result<()> {
        sqlx::query("CREATE EXTENSION IF NOT EXISTS btree_gist")
            .execute(&self.pool)
            .await
            .context("创建 Geo 索引依赖扩展失败")?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS public.ip_geo (\
               network cidr PRIMARY KEY, ip_version smallint NOT NULL CHECK (ip_version IN (4,6)),\
               start_ip_num numeric(40,0) NOT NULL, end_ip_num numeric(40,0) NOT NULL,\
               continent_name text NOT NULL DEFAULT '', country_iso_code text NOT NULL DEFAULT '',\
               country_name text NOT NULL DEFAULT '', province_iso_code text NOT NULL DEFAULT '',\
               province_name text NOT NULL DEFAULT '', city_name text NOT NULL DEFAULT '',\
               latitude double precision, longitude double precision,\
               time_zone text NOT NULL DEFAULT '', imported_at timestamptz NOT NULL DEFAULT now()\
             )",
        )
        .execute(&self.pool)
        .await
        .context("创建 ip_geo 正式表失败")?;
        validate_ip_geo_columns(&self.pool).await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS ip_geo_num_lookup_gist_idx ON public.ip_geo \
             USING gist (start_ip_num, end_ip_num)\
             INCLUDE (country_iso_code, country_name, province_name, city_name,\
                      continent_name, latitude, longitude)",
        )
        .execute(&self.pool)
        .await
        .context("创建 Geo 查询索引失败")?;
        Ok(())
    }

    /// 读取 Geo 表和版本信息。
    pub async fn status(&self) -> Result<GeoSyncStatus> {
        let latest_success = sqlx::query(
            "SELECT file_name, file_hash, database_type, build_epoch, row_count, source_row_count, ipv6_source_row_count, finished_at \
             FROM geo_sync_runs WHERE status='succeeded' ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.runs)
        .await
        .context("读取 Geo 最近导入版本失败")?;
        let table_exists: bool =
            sqlx::query_scalar("SELECT to_regclass('public.ip_geo') IS NOT NULL")
                .fetch_one(&self.pool)
                .await
                .context("检查 ip_geo 表失败")?;
        let (current_row_count, latest_data_at, index_ready) = if table_exists {
            validate_ip_geo_columns(&self.pool).await?;
            let index_ready: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname='public' \
                 AND tablename='ip_geo' AND indexname='ip_geo_num_lookup_gist_idx' \
                 AND indexdef ILIKE '%USING gist (start_ip_num, end_ip_num)%')",
            )
            .fetch_one(&self.pool)
            .await
            .context("检查 ip_geo 查询索引失败")?;
            if let Some(row) = &latest_success {
                let has_data: bool =
                    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM public.ip_geo LIMIT 1)")
                        .fetch_one(&self.pool)
                        .await
                        .context("检查 ip_geo 数据是否存在失败")?;
                if has_data {
                    (
                        Some(row.try_get("row_count")?),
                        row.try_get("finished_at")?,
                        index_ready,
                    )
                } else {
                    (Some(0), None, index_ready)
                }
            } else {
                let row = sqlx::query(
                    "SELECT count(*)::bigint AS row_count, max(imported_at) AS latest_data_at FROM public.ip_geo",
                )
                .fetch_one(&self.pool)
                .await
                .context("读取 ip_geo 数据状态失败")?;
                (
                    Some(row.try_get("row_count")?),
                    row.try_get("latest_data_at")?,
                    index_ready,
                )
            }
        } else {
            (None, None, false)
        };
        let latest_run = self.latest_run().await?;
        Ok(GeoSyncStatus {
            configured: true,
            sample_limit: self.config.max_rows,
            table_exists,
            index_ready,
            current_row_count,
            source_row_count: latest_success
                .as_ref()
                .map(|row| row.try_get("source_row_count"))
                .transpose()?
                .flatten(),
            ipv6_source_row_count: latest_success
                .as_ref()
                .map(|row| row.try_get("ipv6_source_row_count"))
                .transpose()?
                .flatten(),
            latest_data_at,
            file_name: latest_success
                .as_ref()
                .map(|row| row.try_get("file_name"))
                .transpose()?,
            file_hash: latest_success
                .as_ref()
                .map(|row| row.try_get("file_hash"))
                .transpose()?,
            database_type: latest_success
                .as_ref()
                .map(|row| row.try_get("database_type"))
                .transpose()?,
            build_epoch: latest_success
                .as_ref()
                .map(|row| row.try_get("build_epoch"))
                .transpose()?,
            latest_run,
            error: None,
        })
    }

    /// 枚举固定目录中可读、非符号链接的普通 MMDB 文件。
    pub fn files(&self) -> Result<Vec<MmdbFile>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.config.mmdb_dir)
            .with_context(|| format!("读取 MMDB 目录失败: {}", self.config.mmdb_dir.display()))?
        {
            let entry = entry.context("读取 MMDB 目录项失败")?;
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with(".mmdb") || !valid_file_name(&name) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).context("读取 MMDB 文件属性失败")?;
            if !metadata.file_type().is_file() {
                continue;
            }
            files.push(MmdbFile {
                name,
                size: metadata.len(),
                modified_at: metadata
                    .modified()
                    .context("读取 MMDB 修改时间失败")?
                    .into(),
            });
        }
        files.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(files)
    }

    /// 创建导入记录；同一时刻只允许一个运行中的任务。
    pub async fn start_import(&self, file_name: &str) -> Result<GeoRun> {
        let path = self.validate_file(file_name)?;
        let (hash, database_type, build_epoch) =
            tokio::task::spawn_blocking(move || fingerprint_mmdb(&path))
                .await
                .context("检查 MMDB 文件任务异常结束")??;
        let mut transaction = self.runs.begin().await.context("创建 Geo 导入事务失败")?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM geo_sync_runs WHERE status='running')",
        )
        .fetch_one(&mut *transaction)
        .await
        .context("检查 Geo 导入并发状态失败")?;
        if running {
            bail!("已有 Geo 导入正在运行");
        }
        self.reject_duplicate(
            &mut transaction,
            file_name,
            &hash,
            &database_type,
            build_epoch,
        )
        .await?;
        let row = sqlx::query(
            "INSERT INTO geo_sync_runs(status, stage, file_name, file_hash, database_type, build_epoch, row_limit, started_at) \
             VALUES ('running', 'queued', $1, $2, $3, $4, $5, strftime('%Y-%m-%dT%H:%M:%fZ','now')) RETURNING id, started_at",
        )
        .bind(file_name)
        .bind(&hash)
        .bind(&database_type)
        .bind(build_epoch)
        .bind(self.config.max_rows.map(|value| value as i64))
        .fetch_one(&mut *transaction)
        .await
        .context("创建 Geo 导入记录失败")?;
        transaction
            .commit()
            .await
            .context("提交 Geo 导入记录失败")?;
        Ok(GeoRun {
            id: row.try_get("id")?,
            status: "running".to_string(),
            stage: "queued".to_string(),
            file_name: file_name.to_string(),
            file_hash: Some(hash),
            row_count: None,
            source_row_count: None,
            ipv6_source_row_count: None,
            row_limit: self.config.max_rows.map(|value| value as i64),
            progress_rows: 0,
            progress_percent: None,
            error: None,
            started_at: row.try_get("started_at")?,
            finished_at: None,
        })
    }

    /// 查询一次导入的运行状态。
    pub async fn run(&self, id: i64) -> Result<Option<GeoRun>> {
        let row = sqlx::query(
            "SELECT id, status, stage, file_name, file_hash, row_count, source_row_count, ipv6_source_row_count, row_limit, progress_rows, progress_percent, error, started_at, finished_at \
             FROM geo_sync_runs WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.runs)
        .await
        .context("查询 Geo 导入记录失败")?;
        row.map(parse_run).transpose()
    }

    /// 从目标库中的导入记录表读取 Geo 历史。
    pub async fn history(&self, limit: i64, offset: i64) -> Result<RunHistory<GeoRun>> {
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM geo_sync_runs")
            .fetch_one(&self.runs)
            .await
            .context("统计 Geo 导入历史失败")?;
        let rows = sqlx::query(
            "SELECT id, status, stage, file_name, file_hash, row_count, source_row_count, ipv6_source_row_count, row_limit, progress_rows, progress_percent, error, started_at, finished_at \
             FROM geo_sync_runs ORDER BY id DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.runs)
        .await
        .context("查询 Geo 导入历史失败")?;
        let items = rows
            .into_iter()
            .map(parse_run)
            .collect::<Result<Vec<_>>>()?;
        Ok(RunHistory { items, total })
    }

    /// 后台执行导入，并将成功或失败持久化。
    pub async fn execute_import(&self, run: GeoRun) {
        let result = self.execute_import_inner(&run).await;
        match result {
            Ok((count, _, _, _)) => {
                info!("Geo 导入完成: run_id={}, rows={}", run.id, count);
            }
            Err(error) => {
                warn!("Geo 导入失败: run_id={}, error={:#}", run.id, error);
                if let Err(cleanup_error) = self.cleanup_stage(run.id).await {
                    warn!(
                        "清理 Geo 临时表失败: run_id={}, error={:#}",
                        run.id, cleanup_error
                    );
                }
                let message = format!("{error:#}");
                let _ = sqlx::query(
                    "UPDATE geo_sync_runs SET status='failed', stage='failed',\
                     error=$2, finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1",
                )
                .bind(run.id)
                .bind(message.chars().take(500).collect::<String>())
                .execute(&self.runs)
                .await;
            }
        }
    }

    async fn latest_run(&self) -> Result<Option<GeoRun>> {
        let row = sqlx::query(
            "SELECT id, status, stage, file_name, file_hash, row_count, source_row_count, ipv6_source_row_count, row_limit, progress_rows, progress_percent, error, started_at, finished_at \
             FROM geo_sync_runs ORDER BY CASE WHEN status='running' THEN 0 ELSE 1 END, id DESC LIMIT 1",
        )
        .fetch_optional(&self.runs)
        .await
        .context("读取 Geo 最近导入状态失败")?;
        row.map(parse_run).transpose()
    }

    fn validate_file(&self, file_name: &str) -> Result<PathBuf> {
        if !valid_file_name(file_name) || !file_name.ends_with(".mmdb") {
            bail!("MMDB 文件名无效");
        }
        let path = self.config.mmdb_dir.join(file_name);
        let metadata = fs::symlink_metadata(&path).context("读取 MMDB 文件失败")?;
        if !metadata.file_type().is_file() {
            bail!("MMDB 必须是固定目录下的普通文件");
        }
        Ok(path)
    }

    async fn reject_duplicate(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        file_name: &str,
        hash: &str,
        database_type: &str,
        build_epoch: i64,
    ) -> Result<()> {
        let duplicate: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM geo_sync_runs WHERE status='succeeded' \
             AND (file_name=$1 OR file_hash=$2 OR (database_type=$3 AND build_epoch=$4)))",
        )
        .bind(file_name)
        .bind(hash)
        .bind(database_type)
        .bind(build_epoch)
        .fetch_one(&mut **transaction)
        .await
        .context("检查 MMDB 已导入版本失败")?;
        if duplicate {
            bail!("该 MMDB 版本或 SHA-256 已导入，请选择新版本文件");
        }
        Ok(())
    }

    async fn set_stage(&self, id: i64, stage: &str) -> Result<()> {
        sqlx::query("UPDATE geo_sync_runs SET stage=$2 WHERE id=$1")
            .bind(id)
            .bind(stage)
            .execute(&self.runs)
            .await
            .context("更新 Geo 导入阶段失败")?;
        Ok(())
    }

    async fn set_progress(&self, id: i64, percent: i32) -> Result<()> {
        sqlx::query("UPDATE geo_sync_runs SET progress_percent=max(coalesce(progress_percent, 0), $2) WHERE id=$1")
            .bind(id)
            .bind(percent)
            .execute(&self.runs)
            .await
            .context("更新 Geo 导入进度失败")?;
        Ok(())
    }

    // 建索引耗时远大于 MMDB 解析；用 PostgreSQL 的块/元组计数更新进度。
    fn monitor_index_progress(
        &self,
        id: i64,
        stage: &str,
        start: i32,
        end: i32,
    ) -> tokio::task::JoinHandle<()> {
        let pg = self.pool.clone();
        let runs = self.runs.clone();
        let relation = format!("public.{stage}");
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let progress = sqlx::query(
                    "SELECT blocks_done, blocks_total, tuples_done, tuples_total \
                     FROM pg_stat_progress_create_index WHERE relid=to_regclass($1) LIMIT 1",
                )
                .bind(&relation)
                .fetch_optional(&pg)
                .await;
                let Ok(Some(row)) = progress else { continue };
                let blocks_done: i64 = row.try_get("blocks_done").unwrap_or(0);
                let blocks_total: i64 = row.try_get("blocks_total").unwrap_or(0);
                let tuples_done: i64 = row.try_get("tuples_done").unwrap_or(0);
                let tuples_total: i64 = row.try_get("tuples_total").unwrap_or(0);
                let ratio = if blocks_total > 0 {
                    blocks_done as f64 / blocks_total as f64
                } else if tuples_total > 0 {
                    tuples_done as f64 / tuples_total as f64
                } else {
                    continue;
                };
                // 每个索引阶段的计数可能重置；SQL 中的 max 保证页面进度不回退。
                let percent = start + ((end - start - 1) as f64 * ratio.clamp(0.0, 1.0)) as i32;
                let _ = sqlx::query(
                    "UPDATE geo_sync_runs SET progress_percent=max(coalesce(progress_percent, 0), $2) \
                     WHERE id=$1 AND status='running'",
                )
                .bind(id)
                .bind(percent)
                .execute(&runs)
                .await;
            }
        })
    }

    fn monitor_copy_progress(
        &self,
        id: i64,
        stage: &str,
        expected_rows: i64,
    ) -> tokio::task::JoinHandle<()> {
        let pg = self.pool.clone();
        let runs = self.runs.clone();
        let relation = format!("public.{stage}");
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let active: Option<String> = sqlx::query_scalar(
                    "SELECT stage FROM geo_sync_runs WHERE id=$1 AND status='running'",
                )
                .bind(id)
                .fetch_optional(&runs)
                .await
                .ok()
                .flatten();
                if active.as_deref() != Some("loading") {
                    break;
                }
                let progress = sqlx::query(
                    "SELECT tuples_processed FROM pg_stat_progress_copy \
                     WHERE relid=to_regclass($1) LIMIT 1",
                )
                .bind(&relation)
                .fetch_optional(&pg)
                .await;
                let Ok(Some(row)) = progress else { continue };
                let processed: i64 = row.try_get("tuples_processed").unwrap_or(0);
                let percent = 50
                    + (processed.max(0) as i128 * 9 / i128::from(expected_rows.max(1))).clamp(0, 9)
                        as i32;
                let _ = sqlx::query(
                    "UPDATE geo_sync_runs SET progress_percent=max(coalesce(progress_percent, 0), $2) \
                     WHERE id=$1 AND status='running'",
                )
                .bind(id)
                .bind(percent)
                .execute(&runs)
                .await;
            }
        })
    }

    async fn cleanup_stage(&self, id: i64) -> Result<()> {
        let stage = geo_stage_name(id);
        sqlx::query(&format!("DROP TABLE IF EXISTS public.{stage}"))
            .execute(&self.pool)
            .await
            .context("删除 Geo 临时表失败")?;
        Ok(())
    }

    async fn cleanup_abandoned_stages(&self) -> Result<()> {
        let ids: Vec<i64> = sqlx::query_scalar(
            "SELECT id FROM geo_sync_runs WHERE status IN ('failed','interrupted')",
        )
        .fetch_all(&self.runs)
        .await
        .context("查询 Geo 中断任务失败")?;
        for id in ids {
            self.cleanup_stage(id).await?;
        }
        Ok(())
    }

    // 新表保留正式表授权；ip_geo_app 等只读角色在切换后仍须能查询。
    async fn copy_table_grants(&self, stage: &str) -> Result<()> {
        let grants = sqlx::query(
            "SELECT CASE WHEN acl.grantee=0 THEN 'PUBLIC' ELSE quote_ident(pg_get_userbyid(acl.grantee)) END AS grantee, \
             acl.privilege_type, acl.is_grantable FROM pg_class c, \
             LATERAL aclexplode(coalesce(c.relacl, acldefault('r',c.relowner))) acl \
             WHERE c.oid='public.ip_geo'::regclass",
        ).fetch_all(&self.pool).await.context("读取 Geo 表权限失败")?;
        for grant in grants {
            let privilege: String = grant.try_get("privilege_type")?;
            if !matches!(
                privilege.as_str(),
                "SELECT"
                    | "INSERT"
                    | "UPDATE"
                    | "DELETE"
                    | "TRUNCATE"
                    | "REFERENCES"
                    | "TRIGGER"
                    | "MAINTAIN"
            ) {
                bail!("Geo 表包含不支持复制的权限类型");
            }
            let grantee: String = grant.try_get("grantee")?;
            let grant_option: bool = grant.try_get("is_grantable")?;
            let statement = format!(
                "GRANT {privilege} ON TABLE public.{stage} TO {grantee}{}",
                if grant_option {
                    " WITH GRANT OPTION"
                } else {
                    ""
                },
            );
            sqlx::query(&statement)
                .execute(&self.pool)
                .await
                .context("复制 Geo 表权限失败")?;
        }
        Ok(())
    }

    // NOWAIT 避免切换锁排队阻塞新查询；30 秒未找到空档就保留旧版本。
    async fn switch_table(&self, id: i64) -> Result<()> {
        let stage = geo_stage_name(id);
        let old = format!("ip_geo_old_{id}");
        for attempt in 0..30 {
            let mut transaction = self.pool.begin().await.context("开启 Geo 切换事务失败")?;
            // 给元数据切换设置语句上限，避免异常 DDL 长时间占住业务查询。
            sqlx::query("SET LOCAL statement_timeout = '5s'")
                .execute(&mut *transaction)
                .await
                .context("设置 Geo 切换超时失败")?;
            match sqlx::query("LOCK TABLE public.ip_geo IN ACCESS EXCLUSIVE MODE NOWAIT")
                .execute(&mut *transaction)
                .await
            {
                Ok(_) => {}
                Err(sqlx::Error::Database(error)) if error.code().as_deref() == Some("55P03") => {
                    transaction.rollback().await?;
                    if attempt == 29 {
                        bail!("Geo 表持续有查询，30 秒内未取得切换锁；旧版本保持可用");
                    }
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
                Err(error) => return Err(error).context("锁定 Geo 正式表失败"),
            }
            sqlx::query(&format!("ALTER TABLE public.ip_geo RENAME TO {old}"))
                .execute(&mut *transaction)
                .await
                .context("备份旧版 Geo 表失败")?;
            sqlx::query(&format!("ALTER TABLE public.{stage} RENAME TO ip_geo"))
                .execute(&mut *transaction)
                .await
                .context("启用新版 Geo 表失败")?;
            // 不使用 CASCADE；若有依赖正式表 OID 的对象，整个事务回滚。
            sqlx::query(&format!("DROP TABLE public.{old}"))
                .execute(&mut *transaction)
                .await
                .context("删除旧版 Geo 表失败；请检查依赖视图或外键")?;
            sqlx::query(&format!(
                "ALTER TABLE public.ip_geo RENAME CONSTRAINT {stage}_pkey TO ip_geo_pkey"
            ))
            .execute(&mut *transaction)
            .await
            .context("恢复 Geo 主键名称失败")?;
            sqlx::query(&format!(
                "ALTER INDEX public.{stage}_gist_idx RENAME TO ip_geo_num_lookup_gist_idx"
            ))
            .execute(&mut *transaction)
            .await
            .context("恢复 Geo 查询索引名称失败")?;
            transaction.commit().await.context("提交 Geo 表切换失败")?;
            return Ok(());
        }
        unreachable!()
    }

    async fn execute_import_inner(&self, run: &GeoRun) -> Result<(i64, String, String, i64)> {
        self.set_stage(run.id, "snapshot").await?;
        let source = self.validate_file(&run.file_name)?;
        let workspace = tempfile::Builder::new()
            .prefix("wp-geo-")
            .tempdir_in(&self.config.work_dir)
            .context("创建 MMDB 导入临时目录失败")?;
        let snapshot = workspace.path().join("source.mmdb");
        let tsv = workspace.path().join("ip_geo.tsv");
        let progress = Arc::new(AtomicU64::new(0));
        let ipv4_progress = Arc::new(AtomicU64::new(0));
        let ipv6_progress = Arc::new(AtomicU64::new(0));
        let percent = Arc::new(AtomicU64::new(0));
        let done = Arc::new(AtomicBool::new(false));
        let progress_reader = Arc::clone(&progress);
        let ipv4_reader = Arc::clone(&ipv4_progress);
        let ipv6_reader = Arc::clone(&ipv6_progress);
        let percent_reader = Arc::clone(&percent);
        let done_reader = Arc::clone(&done);
        let pool = self.runs.clone();
        let run_id = run.id;
        let monitor = tokio::spawn(async move {
            while !done_reader.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_secs(2)).await;
                let rows = progress_reader.load(Ordering::Relaxed) as i64;
                let current_percent = percent_reader.load(Ordering::Relaxed) as i32;
                let _ = sqlx::query(
                    "UPDATE geo_sync_runs SET progress_rows=$2, progress_percent=$3, ipv6_source_row_count=$4, source_row_count=$5 \
                         WHERE id=$1 AND status='running'",
                )
                .bind(run_id)
                .bind(rows)
                .bind(current_percent)
                .bind(ipv6_reader.load(Ordering::Relaxed) as i64)
                .bind(ipv4_reader.load(Ordering::Relaxed) as i64)
                .execute(&pool)
                .await;
            }
        });
        let max_rows = self.config.max_rows;
        let export_result = tokio::task::spawn_blocking(move || {
            snapshot_and_export(
                &source,
                &snapshot,
                &tsv,
                &progress,
                &ipv4_progress,
                &ipv6_progress,
                &percent,
                max_rows,
            )
        })
        .await
        .context("MMDB 解析任务异常结束")?;
        done.store(true, Ordering::Relaxed);
        monitor.abort();
        let (hash, database_type, build_epoch, exported, source_rows, ipv6_rows) = export_result?;
        if run.file_hash.as_deref() != Some(hash.as_str()) {
            bail!("MMDB 文件在点击导入后发生变化，请重新选择文件并重试");
        }
        let mut check = self.runs.begin().await?;
        self.reject_duplicate(
            &mut check,
            &run.file_name,
            &hash,
            &database_type,
            build_epoch,
        )
        .await?;
        check.rollback().await?;
        sqlx::query(
            "UPDATE geo_sync_runs SET progress_rows=$2, progress_percent=35, \
             file_hash=$3, source_row_count=$4, ipv6_source_row_count=$5, \
             database_type=$6, build_epoch=$7 WHERE id=$1",
        )
        .bind(run.id)
        .bind(exported)
        .bind(&hash)
        .bind(source_rows)
        .bind(ipv6_rows)
        .bind(&database_type)
        .bind(build_epoch)
        .execute(&self.runs)
        .await
        .context("记录 Geo 解析进度失败")?;

        self.set_stage(run.id, "loading").await?;
        let stage = geo_stage_name(run.id);
        sqlx::query(&format!(
            "CREATE TABLE public.{stage} (LIKE public.ip_geo INCLUDING DEFAULTS INCLUDING CONSTRAINTS)"
        ))
        .execute(&self.pool).await.context("创建 Geo 临时表失败")?;
        let mut copy = self
            .pool
            .copy_in_raw(&format!(
                r"COPY public.{stage} (
                    network, ip_version, start_ip_num, end_ip_num, continent_name,
                    country_iso_code, country_name, province_iso_code, province_name,
                    city_name, latitude, longitude, time_zone
                ) FROM STDIN WITH (FORMAT text, DELIMITER E'\t')"
            ))
            .await
            .context("启动 Geo COPY 失败")?;
        let copy_monitor = self.monitor_copy_progress(run.id, &stage, exported);
        let mut file = tokio::fs::File::open(workspace.path().join("ip_geo.tsv"))
            .await
            .context("打开 Geo 导入数据失败")?;
        let total_bytes = file
            .metadata()
            .await
            .context("读取 Geo 导入数据大小失败")?
            .len();
        let mut sent = 0u64;
        let mut buffer = vec![0u8; 1024 * 1024];
        let mut last_report = tokio::time::Instant::now();
        loop {
            let len = file
                .read(&mut buffer)
                .await
                .context("读取 Geo 导入数据失败")?;
            if len == 0 {
                break;
            }
            copy.send(&buffer[..len])
                .await
                .context("导入 Geo 数据失败")?;
            sent += len as u64;
            if last_report.elapsed() >= Duration::from_secs(2) {
                let percent = 35
                    + (sent
                        .saturating_mul(15)
                        .checked_div(total_bytes)
                        .unwrap_or(0) as i32);
                sqlx::query("UPDATE geo_sync_runs SET progress_percent=max(coalesce(progress_percent, 0), $2) WHERE id=$1")
                    .bind(run.id)
                    .bind(percent)
                    .execute(&self.runs)
                    .await
                    .context("记录 Geo 导入进度失败")?;
                last_report = tokio::time::Instant::now();
            }
        }
        let copy_result = copy.finish().await;
        copy_monitor.abort();
        let copied = copy_result.context("完成 Geo COPY 失败")? as i64;
        if copied != exported {
            bail!("Geo 导入行数与 MMDB 导出行数不一致");
        }
        sqlx::query("UPDATE geo_sync_runs SET progress_percent=60 WHERE id=$1")
            .bind(run.id)
            .execute(&self.runs)
            .await?;
        self.set_stage(run.id, "validating").await?;
        let row = sqlx::query(&format!(
            "SELECT count(*)::bigint AS total, \
             count(*) FILTER (WHERE ip_version=4)::bigint AS ipv4, \
             count(*) FILTER (WHERE start_ip_num>end_ip_num)::bigint AS invalid \
             FROM public.{stage}"
        ))
        .fetch_one(&self.pool)
        .await
        .context("验证 Geo 数据失败")?;
        let total: i64 = row.try_get("total")?;
        let ipv4: i64 = row.try_get("ipv4")?;
        let invalid: i64 = row.try_get("invalid")?;
        if total == 0 || invalid != 0 || ipv4 != total {
            bail!("Geo 数据校验失败：缺少 IPv4 或存在非 IPv4/无效区间");
        }
        self.set_progress(run.id, 64).await?;
        self.set_stage(run.id, "indexing_primary").await?;
        let primary_monitor = self.monitor_index_progress(run.id, &stage, 64, 75);
        let primary_result = sqlx::query(&format!(
            "ALTER TABLE public.{stage} ADD PRIMARY KEY (network)"
        ))
        .execute(&self.pool)
        .await;
        primary_monitor.abort();
        primary_result.context("创建 Geo 临时表主键失败")?;
        self.set_progress(run.id, 75).await?;
        self.set_stage(run.id, "indexing_lookup").await?;
        let lookup_monitor = self.monitor_index_progress(run.id, &stage, 75, 96);
        let lookup_result = sqlx::query(&format!(
            "CREATE INDEX {stage}_gist_idx ON public.{stage} USING gist (start_ip_num, end_ip_num) \
             INCLUDE (country_iso_code, country_name, province_name, city_name, continent_name, latitude, longitude)"
        ))
        .execute(&self.pool).await;
        lookup_monitor.abort();
        lookup_result.context("创建 Geo 临时表查询索引失败")?;
        self.set_progress(run.id, 96).await?;
        self.set_stage(run.id, "analyzing").await?;
        sqlx::query(&format!("ANALYZE public.{stage}"))
            .execute(&self.pool)
            .await
            .context("分析 Geo 临时表失败")?;
        self.copy_table_grants(&stage).await?;
        sqlx::query("UPDATE geo_sync_runs SET progress_percent=98 WHERE id=$1")
            .bind(run.id)
            .execute(&self.runs)
            .await?;
        self.set_stage(run.id, "switching").await?;
        self.switch_table(run.id).await?;
        sqlx::query(
            "UPDATE geo_sync_runs SET status='succeeded', stage='completed', \
             row_count=$2, progress_rows=$2, progress_percent=100, file_hash=$3, \
             database_type=$4, build_epoch=$5, finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1",
        )
        .bind(run.id)
        .bind(total)
        .bind(&hash)
        .bind(&database_type)
        .bind(build_epoch)
        .execute(&self.runs)
        .await
        .context("记录 Geo 导入成功摘要失败")?;
        Ok((total, hash, database_type, build_epoch))
    }
}

fn geo_stage_name(id: i64) -> String {
    format!("ip_geo_stage_{id}")
}

fn snapshot_and_export(
    source: &Path,
    snapshot: &Path,
    tsv: &Path,
    progress: &AtomicU64,
    ipv4_progress: &AtomicU64,
    ipv6_progress: &AtomicU64,
    percent: &AtomicU64,
    max_rows: Option<u64>,
) -> Result<(String, String, i64, i64, i64, i64)> {
    let before = fs::metadata(source).context("读取 MMDB 文件属性失败")?;
    let mut input = File::open(source).context("打开 MMDB 文件失败")?;
    let mut output = BufWriter::new(File::create(snapshot).context("创建 MMDB 快照失败")?);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut copied_bytes = 0u64;
    loop {
        let len = input.read(&mut buffer).context("读取 MMDB 文件失败")?;
        if len == 0 {
            break;
        }
        output
            .write_all(&buffer[..len])
            .context("写入 MMDB 快照失败")?;
        hasher.update(&buffer[..len]);
        copied_bytes += len as u64;
        if before.len() > 0 {
            percent.store((copied_bytes * 3 / before.len()).min(3), Ordering::Relaxed);
        }
    }
    output.flush().context("保存 MMDB 快照失败")?;
    let after = fs::metadata(source).context("再次读取 MMDB 文件属性失败")?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        bail!("MMDB 文件正在变化，请完成写入后重试");
    }
    let hash = format!("{:x}", hasher.finalize());
    // 快照在读取期间只由当前任务持有且不会修改，映射可避免再复制整份 MMDB 到堆内存。
    let reader = unsafe { Reader::open_mmap(snapshot) }.context("解析 MMDB 文件失败")?;
    let database_type = reader.metadata.database_type.clone();
    let build_epoch = reader.metadata.build_epoch as i64;
    let mut writer = BufWriter::new(File::create(tsv).context("创建 Geo 导入数据失败")?);
    // 多个网段可能指向同一 MMDB 数据记录；缓存已编码列，避免重复解码和分配。
    let mut record_cache: HashMap<usize, Vec<u8>> = HashMap::new();
    let mut exported = 0i64;
    let mut source_rows = 0i64;
    // 直接定位 IPv4 子树，避免遍历和解码 IPv6 数据。
    let ipv4: IpNetwork = "0.0.0.0/0".parse()?;
    for result in reader.within(ipv4, Default::default())? {
        if max_rows.is_some_and(|limit| exported as u64 >= limit) {
            break;
        }
        let lookup = result.context("读取 MMDB 网段失败")?;
        source_rows += 1;
        if source_rows % 4096 == 0 {
            ipv4_progress.store(source_rows as u64, Ordering::Relaxed);
        }
        if source_rows % 8192 == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        let network = lookup.network().context("读取 MMDB 网络地址失败")?;
        let IpNetwork::V4(network) = network else {
            bail!("MMDB IPv4 子树返回了非 IPv4 网段");
        };
        // MMDB 不提供网段总数；此值表示已扫描的 IPv4 地址空间比例，并非耗时预测。
        let scanned = 3 + u64::from(u32::from(network.network())) * 32 / u64::from(u32::MAX);
        if source_rows % 4096 == 0 {
            percent.fetch_max(scanned.min(34), Ordering::Relaxed);
        }
        let Some(offset) = lookup.offset() else {
            continue;
        };
        if !record_cache.contains_key(&offset) {
            let Some(record) = lookup.decode::<geoip2::City<'_>>()? else {
                continue;
            };
            if record_cache.len() >= GEO_CACHE_LIMIT {
                record_cache.clear();
            }
            record_cache.insert(offset, encode_geo_fields(&record)?);
        }
        write!(
            writer,
            "{}\t4\t{}\t{}\t",
            network,
            u32::from(network.network()),
            u32::from(network.broadcast())
        )?;
        writer.write_all(&record_cache[&offset])?;
        writer.write_all(b"\n").context("写入 Geo 导入数据失败")?;
        exported += 1;
        if exported % 1000 == 0 {
            progress.store(exported as u64, Ordering::Relaxed);
        }
    }
    writer.flush().context("保存 Geo 导入数据失败")?;
    progress.store(exported as u64, Ordering::Relaxed);
    ipv4_progress.store(source_rows as u64, Ordering::Relaxed);
    // IPv6 只统计网段，不解码记录，也不写入目标库。
    let mut ipv6_rows = 0i64;
    if reader.metadata.ip_version == 6 && max_rows.is_none() {
        for result in reader.networks(Default::default())? {
            let lookup = result.context("扫描 MMDB IPv6 网段失败")?;
            if matches!(lookup.network()?, IpNetwork::V6(_)) {
                ipv6_rows += 1;
                if ipv6_rows % 4096 == 0 {
                    ipv6_progress.store(ipv6_rows as u64, Ordering::Relaxed);
                }
            }
        }
    }
    ipv6_progress.store(ipv6_rows as u64, Ordering::Relaxed);
    percent.store(35, Ordering::Relaxed);
    Ok((
        hash,
        database_type,
        build_epoch,
        exported,
        source_rows,
        ipv6_rows,
    ))
}

fn fingerprint_mmdb(path: &Path) -> Result<(String, String, i64)> {
    let before = fs::metadata(path).context("读取 MMDB 文件属性失败")?;
    let mut file = File::open(path).context("打开 MMDB 文件失败")?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let len = file.read(&mut buffer).context("读取 MMDB 文件失败")?;
        if len == 0 {
            break;
        }
        hasher.update(&buffer[..len]);
    }
    let after = fs::metadata(path).context("再次读取 MMDB 文件属性失败")?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        bail!("MMDB 文件正在变化，请完成写入后重试");
    }
    // 校验发生在用户点击后，读取元数据时文件由部署者保持只读。
    let reader = unsafe { Reader::open_mmap(path) }.context("解析 MMDB 文件失败")?;
    Ok((
        format!("{:x}", hasher.finalize()),
        reader.metadata.database_type.clone(),
        reader.metadata.build_epoch as i64,
    ))
}

fn pick_name(names: &geoip2::Names<'_>) -> String {
    names
        .simplified_chinese
        .or(names.english)
        .or(names.japanese)
        .or(names.german)
        .unwrap_or("")
        .to_string()
}

fn encode_geo_fields(record: &geoip2::City<'_>) -> Result<Vec<u8>> {
    let subdivision = record.subdivisions.first();
    let fields = [
        pick_name(&record.continent.names),
        record.country.iso_code.unwrap_or("").to_string(),
        pick_name(&record.country.names),
        subdivision
            .and_then(|item| item.iso_code)
            .unwrap_or("")
            .to_string(),
        subdivision
            .map(|item| pick_name(&item.names))
            .unwrap_or_default(),
        pick_name(&record.city.names),
        record
            .location
            .latitude
            .map(|value| value.to_string())
            .unwrap_or_else(|| r"\N".to_string()),
        record
            .location
            .longitude
            .map(|value| value.to_string())
            .unwrap_or_else(|| r"\N".to_string()),
        record.location.time_zone.unwrap_or("").to_string(),
    ];
    let mut encoded = Vec::with_capacity(160);
    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            encoded.write_all(b"\t")?;
        }
        if (index == 6 || index == 7) && field == r"\N" {
            encoded.write_all(field.as_bytes())?;
        } else {
            write_tsv_field(&mut encoded, field)?;
        }
    }
    Ok(encoded)
}

fn write_tsv_field(writer: &mut impl Write, value: &str) -> Result<()> {
    if !value
        .bytes()
        .any(|byte| matches!(byte, b'\\' | b'\t' | b'\n' | b'\r'))
    {
        writer.write_all(value.as_bytes())?;
        return Ok(());
    }
    for byte in value.bytes() {
        match byte {
            b'\\' => writer.write_all(b"\\\\")?,
            b'\t' => writer.write_all(b"\\t")?,
            b'\n' => writer.write_all(b"\\n")?,
            b'\r' => writer.write_all(b"\\r")?,
            _ => writer.write_all(&[byte])?,
        }
    }
    Ok(())
}

fn parse_run(row: sqlx::sqlite::SqliteRow) -> Result<GeoRun> {
    Ok(GeoRun {
        id: row.try_get("id")?,
        status: row.try_get("status")?,
        stage: row.try_get("stage")?,
        file_name: row.try_get("file_name")?,
        file_hash: row.try_get("file_hash")?,
        row_count: row.try_get("row_count")?,
        source_row_count: row.try_get("source_row_count")?,
        ipv6_source_row_count: row.try_get("ipv6_source_row_count")?,
        row_limit: row.try_get("row_limit")?,
        progress_rows: row.try_get("progress_rows")?,
        progress_percent: row.try_get("progress_percent")?,
        error: row.try_get("error")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}

fn valid_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains('\0')
}

async fn validate_ip_geo_columns(pool: &PgPool) -> Result<()> {
    let required = [
        "network",
        "ip_version",
        "start_ip_num",
        "end_ip_num",
        "continent_name",
        "country_iso_code",
        "country_name",
        "province_iso_code",
        "province_name",
        "city_name",
        "latitude",
        "longitude",
        "time_zone",
        "imported_at",
    ];
    let columns: Vec<String> = sqlx::query_scalar(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema='public' AND table_name='ip_geo'",
    )
    .fetch_all(pool)
    .await
    .context("检查 ip_geo 列失败")?;
    if required
        .iter()
        .any(|column| !columns.iter().any(|item| item == column))
    {
        bail!("现有 ip_geo 表结构与 Geo 导入格式不兼容");
    }
    Ok(())
}
