//! 数据知识库同步基础能力。
//!
//! 当前阶段实现资产库初始化、定时任务幂等注册与运行状态读取。

pub mod geo;
pub mod intel;
pub mod legacy;

/// Station 主 SQLite 数据库中的运行记录连接池。
pub type RunStore = sqlx::SqlitePool;

/// 连接 Station 已完成迁移的 SQLite 数据库，供 Geo 和情报任务共用。
pub async fn open_run_store(url: &str) -> Result<RunStore> {
    use sqlx::ConnectOptions;
    use std::str::FromStr;
    let options = sqlx::sqlite::SqliteConnectOptions::from_str(url)?
        .log_statements(log::LevelFilter::Off)
        .log_slow_statements(log::LevelFilter::Warn, Duration::from_secs(10));
    sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(3)
        .connect_with(options)
        .await
        .context("连接 Station SQLite 运行记录库失败")
}

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use percent_encoding::percent_decode_str;
use serde::Serialize;
use sqlx::Row;
use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use std::env;
use std::str::FromStr;
use std::time::Duration;
use tracing::{info, warn};
use url::Url;

const ASSET_BOOTSTRAP_SQL: &str = include_str!("../sql/asset_sync.sql");
const ASSET_JOB_NAME: &str = "asset_subject_lookup_refresh";
const ASSET_JOB_COMMAND: &str = "CALL asset_lookup.refresh_asset_subject_lookup();";
const ASSET_SCHEMA_VERSION: i32 = 5;

/// 资产目标 PostgreSQL 与外部资产源库的服务端配置。
#[derive(Clone)]
pub struct AssetSyncConfig {
    pub target_database_url: String,
    pub source_host: String,
    pub source_port: u16,
    pub source_database: String,
    pub source_username: String,
    pub source_password: String,
    pub schedule: String,
    pub reader_role: Option<String>,
}

impl AssetSyncConfig {
    /// 从部署环境读取资产同步配置；目标地址未设置时表示此功能未配置。
    pub fn from_env() -> Result<Option<Self>> {
        let Some(target_database_url) =
            optional_env("WP_KSYNC_TARGET_URL")?.or(optional_env("WP_KSYNC_ASSET_TARGET_URL")?)
        else {
            return Ok(None);
        };

        let target_url =
            Url::parse(&target_database_url).context("解析资产目标 PostgreSQL URL 失败")?;
        let source_host = optional_env("WP_KSYNC_ASSET_SOURCE_HOST")?.unwrap_or(
            target_url
                .host_str()
                .context("资产目标 PostgreSQL URL 缺少主机地址")?
                .to_string(),
        );
        let source_port = optional_env("WP_KSYNC_ASSET_SOURCE_PORT")?
            .unwrap_or_else(|| {
                target_url
                    .port_or_known_default()
                    .unwrap_or(5432)
                    .to_string()
            })
            .parse::<u16>()
            .context("WP_KSYNC_ASSET_SOURCE_PORT 必须是 1-65535 的端口号")?;
        let source_database = required_env("WP_KSYNC_ASSET_SOURCE_DATABASE")?;
        let source_username =
            optional_env("WP_KSYNC_ASSET_SOURCE_USERNAME")?.unwrap_or_else(|| {
                percent_decode_str(target_url.username())
                    .decode_utf8_lossy()
                    .into_owned()
            });
        let source_password =
            optional_env("WP_KSYNC_ASSET_SOURCE_PASSWORD")?.unwrap_or_else(|| {
                target_url
                    .password()
                    .map(|password| {
                        percent_decode_str(password)
                            .decode_utf8_lossy()
                            .into_owned()
                    })
                    .unwrap_or_default()
            });
        let schedule = required_env("WP_KSYNC_ASSET_CRON")?;
        let reader_role = optional_env("WP_KSYNC_ASSET_READER_ROLE")?;

        Ok(Some(Self {
            target_database_url,
            source_host,
            source_port,
            source_database,
            source_username,
            source_password,
            schedule,
            reader_role,
        }))
    }
}

/// 资产同步服务实例。
#[derive(Clone)]
pub struct AssetSyncService {
    pool: PgPool,
    config: AssetSyncConfig,
}

impl AssetSyncService {
    /// 建立目标 PostgreSQL 连接池。
    pub async fn connect(config: AssetSyncConfig) -> Result<Self> {
        let connect_options = PgConnectOptions::from_str(&config.target_database_url)
            .context("解析资产目标 PostgreSQL URL 失败")?;
        let pool = PgPoolOptions::new()
            .max_connections(3)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(connect_options)
            .await
            .context("连接资产目标 PostgreSQL 失败")?;
        Ok(Self { pool, config })
    }

    /// 检查并初始化资产对象，幂等注册 pg_cron 任务。
    pub async fn initialize(&self) -> Result<AssetSyncStatus> {
        let database_name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&self.pool)
            .await
            .context("读取资产目标数据库名称失败")?;

        ensure_data_extensions(&self.pool).await?;
        ensure_asset_server(&self.pool, &self.config).await?;
        bootstrap_asset_schema(&self.pool).await?;
        validate_asset_objects(&self.pool).await?;
        grant_reader_access(&self.pool, self.config.reader_role.as_deref()).await?;
        let (job, cron_error) = match ensure_pg_cron(&self.pool).await {
            Ok(()) => match ensure_asset_job(&self.pool, &self.config.schedule).await {
                Ok(job) => (Some(job), None),
                Err(error) => (None, Some(format!("资产定时任务初始化失败: {error:#}"))),
            },
            Err(error) => (None, Some(format!("pg_cron 不可用: {error:#}"))),
        };

        status_from_pool(
            &self.pool,
            database_name,
            job,
            Some(&self.config.schedule),
            cron_error,
        )
        .await
    }

    /// 读取资产同步状态。
    pub async fn status(&self) -> Result<AssetSyncStatus> {
        let database_name: String = sqlx::query_scalar("SELECT current_database()")
            .fetch_one(&self.pool)
            .await
            .context("读取资产目标数据库名称失败")?;
        status_from_pool(
            &self.pool,
            database_name,
            None,
            Some(&self.config.schedule),
            None,
        )
        .await
    }

    /// 直接读取 pg_cron 的资产任务执行历史。
    pub async fn history(&self, limit: i64, offset: i64) -> Result<RunHistory<AssetRun>> {
        let cron_available: bool = sqlx::query_scalar("SELECT to_regclass('cron.job') IS NOT NULL")
            .fetch_one(&self.pool)
            .await
            .context("检查 pg_cron 历史表失败")?;
        if !cron_available {
            return Ok(RunHistory {
                items: Vec::new(),
                total: 0,
            });
        }
        let job_id: Option<i64> = sqlx::query_scalar(
            "SELECT jobid FROM cron.job WHERE database=current_database() \
             AND jobname=$1 ORDER BY jobid LIMIT 1",
        )
        .bind(ASSET_JOB_NAME)
        .fetch_optional(&self.pool)
        .await
        .context("查询资产定时任务失败")?;
        let Some(job_id) = job_id else {
            return Ok(RunHistory {
                items: Vec::new(),
                total: 0,
            });
        };
        let total: i64 =
            sqlx::query_scalar("SELECT count(*) FROM cron.job_run_details WHERE jobid=$1")
                .bind(job_id)
                .fetch_one(&self.pool)
                .await
                .context("统计资产任务历史失败")?;
        let rows = sqlx::query(
            "SELECT r.runid, r.status, r.start_time, r.end_time, r.return_message, \
                    s.written_count, s.asset_count, s.terminal_count, s.network_count, s.ipv4_count, s.ipv6_count \
             FROM cron.job_run_details r \
             LEFT JOIN LATERAL ( \
                 SELECT written_count, asset_count, terminal_count, network_count, ipv4_count, ipv6_count \
                 FROM asset_lookup.sync_run_summary s \
                 WHERE s.cron_run_id = r.runid OR \
                   (s.cron_run_id IS NULL AND r.status='succeeded' AND \
                    s.started_at >= r.start_time AND s.finished_at <= r.end_time) \
                 ORDER BY (s.cron_run_id = r.runid) DESC, s.finished_at DESC LIMIT 1 \
             ) s ON true \
             WHERE r.jobid=$1 ORDER BY r.runid DESC LIMIT $2 OFFSET $3",
        )
        .bind(job_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .context("查询资产任务历史失败")?;
        let items = rows
            .into_iter()
            .map(|row| {
                Ok(AssetRun {
                    run_id: row.try_get("runid")?,
                    status: row.try_get("status")?,
                    started_at: row.try_get("start_time")?,
                    finished_at: row.try_get("end_time")?,
                    message: row.try_get("return_message")?,
                    written_count: row.try_get("written_count")?,
                    asset_count: row.try_get("asset_count")?,
                    terminal_count: row.try_get("terminal_count")?,
                    network_count: row.try_get("network_count")?,
                    ipv4_count: row.try_get("ipv4_count")?,
                    ipv6_count: row.try_get("ipv6_count")?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(RunHistory { items, total })
    }
}

/// 分页历史结果。
#[derive(Debug, Serialize)]
pub struct RunHistory<T> {
    pub items: Vec<T>,
    pub total: i64,
}

/// pg_cron 记录的一次资产任务执行。
#[derive(Debug, Serialize)]
pub struct AssetRun {
    pub run_id: i64,
    pub status: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub message: Option<String>,
    pub written_count: Option<i64>,
    pub asset_count: Option<i64>,
    pub terminal_count: Option<i64>,
    pub network_count: Option<i64>,
    pub ipv4_count: Option<i64>,
    pub ipv6_count: Option<i64>,
}

/// 资产任务和数据状态响应。
#[derive(Debug, Clone, Serialize)]
pub struct AssetSyncStatus {
    pub configured: bool,
    pub initialized: bool,
    pub database: String,
    pub task_name: Option<String>,
    pub task_active: Option<bool>,
    pub task_schedule: Option<String>,
    pub schedule_matches: Option<bool>,
    pub latest_run_status: Option<String>,
    pub latest_run_started_at: Option<DateTime<Utc>>,
    pub latest_run_finished_at: Option<DateTime<Utc>>,
    pub latest_run_message: Option<String>,
    pub latest_success_at: Option<DateTime<Utc>>,
    pub current_row_count: Option<i64>,
    pub current_ipv4_count: Option<i64>,
    pub current_ipv6_count: Option<i64>,
    pub latest_written_count: Option<i64>,
    pub latest_asset_count: Option<i64>,
    pub latest_terminal_count: Option<i64>,
    pub latest_network_count: Option<i64>,
    pub latest_ipv4_count: Option<i64>,
    pub latest_ipv6_count: Option<i64>,
    pub latest_data_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

impl AssetSyncStatus {
    /// 状态接口中表示未注入资产服务参数的响应。
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            initialized: false,
            database: String::new(),
            task_name: None,
            task_active: None,
            task_schedule: None,
            schedule_matches: None,
            latest_run_status: None,
            latest_run_started_at: None,
            latest_run_finished_at: None,
            latest_run_message: None,
            latest_success_at: None,
            current_row_count: None,
            current_ipv4_count: None,
            current_ipv6_count: None,
            latest_written_count: None,
            latest_asset_count: None,
            latest_terminal_count: None,
            latest_network_count: None,
            latest_ipv4_count: None,
            latest_ipv6_count: None,
            latest_data_at: None,
            error: None,
        }
    }

    /// 状态接口中表示启动初始化失败的响应。
    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            configured: true,
            initialized: false,
            error: Some(error.into()),
            ..Self::unconfigured()
        }
    }
}

async fn ensure_data_extensions(pool: &PgPool) -> Result<()> {
    for extension in ["dblink", "btree_gist"] {
        let sql = format!("CREATE EXTENSION IF NOT EXISTS {extension}");
        sqlx::query(&sql)
            .execute(pool)
            .await
            .with_context(|| format!("检查或创建 PostgreSQL 扩展 {extension} 失败"))?;
    }
    Ok(())
}

async fn ensure_pg_cron(pool: &PgPool) -> Result<()> {
    sqlx::query("CREATE EXTENSION IF NOT EXISTS pg_cron")
        .execute(pool)
        .await
        .context("创建 pg_cron 扩展失败；数据库镜像须包含扩展并预加载 pg_cron")?;
    Ok(())
}

async fn ensure_asset_server(pool: &PgPool, config: &AssetSyncConfig) -> Result<()> {
    let options = format!(
        "host {}, port {}, dbname {}",
        quote_literal(&config.source_host),
        quote_literal(&config.source_port.to_string()),
        quote_literal(&config.source_database)
    );
    sqlx::query(&format!(
        "CREATE SERVER IF NOT EXISTS dysec_asset_source FOREIGN DATA WRAPPER dblink_fdw OPTIONS ({options})"
    ))
    .execute(pool)
    .await
    .context("初始化资产源库连接定义失败")?;

    let mapping_options = format!(
        "user {}, password {}",
        quote_literal(&config.source_username),
        quote_literal(&config.source_password)
    );
    sqlx::query(&format!(
        "CREATE USER MAPPING IF NOT EXISTS FOR CURRENT_USER SERVER dysec_asset_source OPTIONS ({mapping_options})"
    ))
    .execute(pool)
    .await
    .context("初始化资产源库用户映射失败")?;

    let server_options: Vec<String> = sqlx::query_scalar(
        "SELECT srvoptions FROM pg_foreign_server WHERE srvname = 'dysec_asset_source'",
    )
    .fetch_one(pool)
    .await
    .context("读取资产源库连接定义失败")?;
    let options = option_map(&server_options);
    if options.get("host") != Some(&config.source_host)
        || options.get("port") != Some(&config.source_port.to_string())
        || options.get("dbname") != Some(&config.source_database)
    {
        bail!("已存在的资产源库连接定义与部署环境不一致");
    }

    let mapping_options: Option<Vec<String>> = sqlx::query_scalar(
        "SELECT um.umoptions \
         FROM pg_user_mapping um \
         JOIN pg_foreign_server fs ON fs.oid = um.umserver \
         WHERE fs.srvname = 'dysec_asset_source' \
           AND um.umuser = (SELECT oid FROM pg_roles WHERE rolname = current_user)",
    )
    .fetch_optional(pool)
    .await
    .context("读取资产源库用户映射失败")?;
    let mapping_options = option_map(&mapping_options.unwrap_or_default());
    if mapping_options.get("user") != Some(&config.source_username)
        || mapping_options.get("password") != Some(&config.source_password)
    {
        bail!("已存在的资产源库用户映射与部署环境不一致");
    }

    // 在初始化阶段验证连接，避免 cron 首次运行才发现凭据或网络错误。
    let source_database: String = sqlx::query_scalar(
        "SELECT remote_value FROM dblink('dysec_asset_source', 'SELECT current_database() AS remote_value') AS remote(remote_value text)",
    )
    .fetch_one(pool)
    .await
    .context("连接资产源库失败；请检查服务端环境变量和本地源库副本")?;
    if source_database != config.source_database {
        bail!("资产源库连接实际落入的数据库与部署环境不一致");
    }
    Ok(())
}

async fn bootstrap_asset_schema(pool: &PgPool) -> Result<()> {
    let mut transaction = pool
        .begin()
        .await
        .context("开启资产 schema 初始化事务失败")?;
    sqlx::query("CREATE SCHEMA IF NOT EXISTS asset_lookup")
        .execute(&mut *transaction)
        .await
        .context("创建资产 schema 失败")?;
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS asset_lookup.station_schema_version (\
           component text PRIMARY KEY, version integer NOT NULL, installed_at timestamptz NOT NULL DEFAULT now()\
         )",
    )
    .execute(&mut *transaction)
    .await
    .context("创建资产 schema 版本表失败")?;
    let installed_version: Option<i32> = sqlx::query_scalar(
        "SELECT version FROM asset_lookup.station_schema_version WHERE component = 'asset-sync'",
    )
    .fetch_optional(&mut *transaction)
    .await
    .context("读取资产 schema 版本失败")?;

    if installed_version.is_some_and(|version| version > ASSET_SCHEMA_VERSION) {
        bail!("资产 schema 版本高于当前程序支持版本");
    }
    if installed_version != Some(ASSET_SCHEMA_VERSION) {
        sqlx::raw_sql(ASSET_BOOTSTRAP_SQL)
            .execute(&mut *transaction)
            .await
            .context("创建或更新资产同步对象失败")?;
        sqlx::query(
            "INSERT INTO asset_lookup.station_schema_version (component, version) \
             VALUES ('asset-sync', $1) \
             ON CONFLICT (component) DO UPDATE SET version = EXCLUDED.version, installed_at = now()",
        )
        .bind(ASSET_SCHEMA_VERSION)
        .execute(&mut *transaction)
        .await
        .context("记录资产 schema 版本失败")?;
    }
    transaction
        .commit()
        .await
        .context("提交资产 schema 初始化事务失败")?;
    Ok(())
}

async fn validate_asset_objects(pool: &PgPool) -> Result<()> {
    let objects_present: bool = sqlx::query_scalar(
        "SELECT to_regclass('asset_lookup.asset_subject_exclude_biz_sync') IS NOT NULL \
            AND to_regclass('asset_lookup.sync_run_summary') IS NOT NULL \
            AND to_regprocedure('asset_lookup.inet_to_num(inet)') IS NOT NULL \
            AND to_regprocedure('asset_lookup.refresh_asset_subject_lookup()') IS NOT NULL",
    )
    .fetch_one(pool)
    .await
    .context("检查资产同步对象失败")?;
    if !objects_present {
        bail!("资产同步表、摘要表、转换函数或同步过程缺失");
    }

    let view_kind: Option<String> = sqlx::query_scalar(
        "SELECT c.relkind::text FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'public' AND c.relname = 'v_asset_subject_exclude_biz'",
    )
    .fetch_optional(pool)
    .await
    .context("检查资产视图失败")?;
    if view_kind.as_deref() != Some("v") {
        bail!("public.v_asset_subject_exclude_biz 不存在或不是普通视图");
    }

    let has_range_index: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_indexes \
         WHERE schemaname = 'asset_lookup' \
           AND tablename = 'asset_subject_exclude_biz_sync' \
           AND indexdef ILIKE '%USING gist (start_ip_num, end_ip_num)%')",
    )
    .fetch_one(pool)
    .await
    .context("检查资产查询索引失败")?;
    if !has_range_index {
        bail!("资产同步表缺少 IP 区间 GiST 索引");
    }
    Ok(())
}

fn option_map(options: &[String]) -> std::collections::HashMap<String, String> {
    options
        .iter()
        .filter_map(|option| {
            option
                .split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
        })
        .collect()
}

async fn grant_reader_access(pool: &PgPool, reader_role: Option<&str>) -> Result<()> {
    let Some(reader_role) = reader_role else {
        return Ok(());
    };
    if reader_role.trim().is_empty() {
        bail!("资产查询角色不能为空");
    }
    let role = quote_identifier(reader_role);
    sqlx::raw_sql(&format!(
        "GRANT USAGE ON SCHEMA asset_lookup TO {role};\
         GRANT SELECT ON asset_lookup.asset_subject_exclude_biz_sync TO {role};\
         GRANT SELECT ON public.v_asset_subject_exclude_biz TO {role};"
    ))
    .execute(pool)
    .await
    .context("授予资产查询权限失败")?;
    Ok(())
}

async fn ensure_asset_job(pool: &PgPool, schedule: &str) -> Result<AssetJob> {
    let jobs = sqlx::query(
        "SELECT jobid, jobname, schedule, command, active \
         FROM cron.job \
         WHERE database = current_database() \
           AND (jobname = $1 OR command = $2) \
         ORDER BY jobid",
    )
    .bind(ASSET_JOB_NAME)
    .bind(ASSET_JOB_COMMAND)
    .fetch_all(pool)
    .await
    .context("读取 pg_cron 资产任务失败")?;

    if jobs.len() > 1 {
        bail!("发现多个资产同步 cron 任务，停止自动初始化");
    }
    if let Some(job) = jobs.first() {
        let existing = AssetJob {
            job_id: job.try_get("jobid")?,
            name: job
                .try_get::<Option<String>, _>("jobname")?
                .unwrap_or_else(|| ASSET_JOB_NAME.to_string()),
            schedule: job.try_get("schedule")?,
            active: job.try_get("active")?,
        };
        if !existing.active {
            warn!("资产 cron 任务已存在但停用: job_id={}", existing.job_id);
        } else if existing.schedule != schedule {
            warn!(
                "资产 cron 周期与部署参数不同，保留现有任务: job_id={}, existing={}, configured={}",
                existing.job_id, existing.schedule, schedule
            );
        }
        return Ok(existing);
    }

    let job_id: i64 = sqlx::query_scalar("SELECT cron.schedule($1, $2, $3)")
        .bind(ASSET_JOB_NAME)
        .bind(schedule)
        .bind(ASSET_JOB_COMMAND)
        .fetch_one(pool)
        .await
        .context("创建 pg_cron 资产同步任务失败")?;

    info!("创建资产 pg_cron 任务成功: job_id={job_id}, schedule={schedule}");
    Ok(AssetJob {
        job_id,
        name: ASSET_JOB_NAME.to_string(),
        schedule: schedule.to_string(),
        active: true,
    })
}

struct AssetJob {
    job_id: i64,
    name: String,
    schedule: String,
    active: bool,
}

async fn status_from_pool(
    pool: &PgPool,
    database_name: String,
    job_hint: Option<AssetJob>,
    configured_schedule: Option<&str>,
    cron_error: Option<String>,
) -> Result<AssetSyncStatus> {
    let cron_available: bool = sqlx::query_scalar("SELECT to_regclass('cron.job') IS NOT NULL")
        .fetch_one(pool)
        .await
        .context("检查 pg_cron 状态失败")?;
    let job = match job_hint {
        Some(job) => Some(job),
        None if cron_available => {
            let row = sqlx::query(
                "SELECT jobid, jobname, schedule, active \
                 FROM cron.job \
                 WHERE database = current_database() \
                   AND (jobname = $1 OR command = $2) \
                 ORDER BY jobid LIMIT 1",
            )
            .bind(ASSET_JOB_NAME)
            .bind(ASSET_JOB_COMMAND)
            .fetch_optional(pool)
            .await
            .context("读取 pg_cron 资产任务状态失败")?;
            row.map(|row| {
                Ok::<_, sqlx::Error>(AssetJob {
                    job_id: row.try_get("jobid")?,
                    name: row
                        .try_get::<Option<String>, _>("jobname")?
                        .unwrap_or_else(|| ASSET_JOB_NAME.to_string()),
                    schedule: row.try_get("schedule")?,
                    active: row.try_get("active")?,
                })
            })
            .transpose()?
        }
        None => None,
    };

    let count_row = sqlx::query(
        "SELECT count(*)::bigint AS row_count, max(synced_at) AS latest_data_at, \
                count(*) FILTER (WHERE lower(ip_type)='ipv4')::bigint AS ipv4_count, \
                count(*) FILTER (WHERE lower(ip_type)='ipv6')::bigint AS ipv6_count \
         FROM asset_lookup.asset_subject_exclude_biz_sync",
    )
    .fetch_one(pool)
    .await
    .context("读取资产同步数据状态失败")?;

    let latest_success = sqlx::query(
        "SELECT started_at, finished_at, asset_count, terminal_count, network_count, written_count, ipv4_count, ipv6_count \
         FROM asset_lookup.sync_run_summary ORDER BY finished_at DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .context("读取资产最近成功同步摘要失败")?;

    let latest_run = if let Some(job) = &job {
        sqlx::query(
            "SELECT status, start_time, end_time, return_message \
             FROM cron.job_run_details WHERE jobid = $1 \
             ORDER BY runid DESC LIMIT 1",
        )
        .bind(job.job_id)
        .fetch_optional(pool)
        .await
        .context("读取资产最近 cron 执行结果失败")?
    } else {
        None
    };

    let latest_success_at = latest_success
        .as_ref()
        .map(|row| row.try_get("finished_at"))
        .transpose()?;
    let (
        latest_written_count,
        latest_asset_count,
        latest_terminal_count,
        latest_network_count,
        latest_ipv4_count,
        latest_ipv6_count,
    ) = match latest_success.as_ref() {
        Some(row) => (
            Some(row.try_get("written_count")?),
            Some(row.try_get("asset_count")?),
            row.try_get("terminal_count")?,
            Some(row.try_get("network_count")?),
            row.try_get("ipv4_count")?,
            row.try_get("ipv6_count")?,
        ),
        None => (None, None, None, None, None, None),
    };

    let (latest_run_status, latest_run_started_at, latest_run_finished_at, latest_run_message) =
        match latest_run {
            Some(row) => (
                row.try_get("status")?,
                row.try_get("start_time")?,
                row.try_get("end_time")?,
                row.try_get("return_message")?,
            ),
            None => (None, None, None, None),
        };

    Ok(AssetSyncStatus {
        configured: true,
        initialized: job.is_some(),
        database: database_name,
        task_name: job.as_ref().map(|job| job.name.clone()),
        task_active: job.as_ref().map(|job| job.active),
        task_schedule: job.as_ref().map(|job| job.schedule.clone()),
        schedule_matches: job
            .as_ref()
            .map(|job| configured_schedule.is_none_or(|schedule| job.schedule == schedule)),
        latest_run_status,
        latest_run_started_at,
        latest_run_finished_at,
        latest_run_message,
        latest_success_at,
        current_row_count: Some(count_row.try_get("row_count")?),
        current_ipv4_count: Some(count_row.try_get("ipv4_count")?),
        current_ipv6_count: Some(count_row.try_get("ipv6_count")?),
        latest_written_count,
        latest_asset_count,
        latest_terminal_count,
        latest_network_count,
        latest_ipv4_count,
        latest_ipv6_count,
        latest_data_at: count_row.try_get("latest_data_at")?,
        error: cron_error.or_else(|| {
            (!cron_available).then(|| "pg_cron 扩展不可用，资产定时任务尚未创建".to_string())
        }),
    })
}

fn required_env(name: &str) -> Result<String> {
    optional_env(name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("缺少环境变量 {name}"))
}

fn optional_env(name: &str) -> Result<Option<String>> {
    env::var(name)
        .map(|value| Some(value.trim().to_string()))
        .or_else(|error| match error {
            env::VarError::NotPresent => Ok(None),
            env::VarError::NotUnicode(_) => Err(anyhow!("环境变量 {name} 不是有效文本")),
        })
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
