//! 从集群内 Doris 读取 IP/域名情报并全量切换 Redis 集合。

use std::env;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use redis::AsyncCommands;
use serde::Serialize;
use sqlx::Row;
use sqlx::SqlitePool;
use sqlx::mysql::{MySqlConnectOptions, MySqlPool, MySqlPoolOptions};
use tracing::{info, warn};

use crate::RunHistory;

const PAGE_SIZE: i64 = 5000;
const IP_KEY: &str = "threat_intelligence:ip";
const DOMAIN_KEY: &str = "threat_intelligence:domain";

/// 部署环境注入的情报同步连接与周期。
#[derive(Clone)]
pub struct IntelSyncConfig {
    pub doris_host: String,
    pub doris_port: u16,
    pub doris_user: String,
    pub doris_password: String,
    pub doris_database: String,
    pub redis_url: String,
    pub interval: Duration,
}

impl IntelSyncConfig {
    /// 缺少必需地址时不启用情报同步；空 Doris 密码合法。
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(doris_host) = env::var("WP_KSYNC_DORIS_HOST") else {
            return Ok(None);
        };
        let Ok(redis_url) = env::var("WP_KSYNC_REDIS_URL") else {
            return Ok(None);
        };
        if doris_host.trim().is_empty() || redis_url.trim().is_empty() {
            bail!("情报同步连接地址不能为空");
        }
        let doris_port = env::var("WP_KSYNC_DORIS_PORT")
            .unwrap_or_else(|_| "9030".into())
            .parse()
            .context("Doris 端口无效")?;
        let seconds: u64 = env::var("WP_KSYNC_INTEL_INTERVAL")
            .unwrap_or_else(|_| "300".into())
            .parse()
            .context("情报同步周期无效")?;
        if seconds < 60 {
            bail!("情报同步周期不能短于 60 秒");
        }
        Ok(Some(Self {
            doris_host,
            doris_port,
            doris_user: env::var("WP_KSYNC_DORIS_USER").unwrap_or_else(|_| "root".into()),
            doris_password: env::var("WP_KSYNC_DORIS_PASSWORD").unwrap_or_default(),
            doris_database: "intel".into(),
            redis_url,
            interval: Duration::from_secs(seconds),
        }))
    }
}

/// 单次定时任务的数据库记录。
#[derive(Clone, Debug, Serialize)]
pub struct IntelRun {
    pub id: i64,
    pub status: String,
    pub stage: String,
    pub read_count: i64,
    pub invalid_count: i64,
    pub expired_count: i64,
    pub ip_count: i64,
    pub domain_count: i64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// 当前 Redis 集合数量及最近定时执行状态。
#[derive(Clone, Debug, Serialize)]
pub struct IntelSyncStatus {
    pub configured: bool,
    pub interval_seconds: Option<u64>,
    pub ip_count: Option<i64>,
    pub domain_count: Option<i64>,
    pub latest_run: Option<IntelRun>,
    pub latest_success_run: Option<IntelRun>,
    pub error: Option<String>,
}

impl IntelSyncStatus {
    /// 尚未注入情报同步连接参数。
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            interval_seconds: None,
            ip_count: None,
            domain_count: None,
            latest_run: None,
            latest_success_run: None,
            error: None,
        }
    }

    /// 连接或初始化失败时供状态页面展示。
    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            configured: true,
            error: Some(error.into()),
            ..Self::unconfigured()
        }
    }
}

/// 情报同步服务，启动时只初始化运行表，定时器到点才读取 Doris。
#[derive(Clone)]
pub struct IntelSyncService {
    doris: MySqlPool,
    target: SqlitePool,
    redis: redis::Client,
    interval: Duration,
}

impl IntelSyncService {
    /// 建立 Doris、Redis 与运行记录库连接。
    pub async fn connect(config: IntelSyncConfig, target: SqlitePool) -> Result<Self> {
        let mut doris_options = MySqlConnectOptions::new()
            .host(&config.doris_host)
            .port(config.doris_port)
            .username(&config.doris_user)
            .database(&config.doris_database);
        if !config.doris_password.is_empty() {
            doris_options = doris_options.password(&config.doris_password);
        }
        let doris = MySqlPoolOptions::new()
            .max_connections(2)
            .connect_with(doris_options)
            .await
            .context("连接 Doris 失败")?;
        let redis = redis::Client::open(config.redis_url).context("解析 Redis 地址失败")?;
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .context("连接 Redis 失败")?;
        let _: String = redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .context("检查 Redis 失败")?;
        Ok(Self {
            doris,
            target,
            redis,
            interval: config.interval,
        })
    }

    /// 幂等创建运行记录，并恢复上次进程留下的执行中记录。
    pub async fn initialize(&self) -> Result<IntelSyncStatus> {
        sqlx::query(
            "UPDATE intel_sync_runs SET status='interrupted', stage='interrupted', \
             error='Station 重启导致任务中断', finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status='running'",
        )
        .execute(&self.target)
        .await
        .context("恢复中断的情报任务失败")?;
        self.status().await
    }

    /// 返回 Redis 实际集合数量和最近一次执行记录。
    pub async fn status(&self) -> Result<IntelSyncStatus> {
        let mut connection = self
            .redis
            .get_multiplexed_async_connection()
            .await
            .context("读取 Redis 情报状态失败")?;
        let ip_count: i64 = connection
            .scard(IP_KEY)
            .await
            .context("读取 IP 集合数量失败")?;
        let domain_count: i64 = connection
            .scard(DOMAIN_KEY)
            .await
            .context("读取域名集合数量失败")?;
        let row = sqlx::query(
            "SELECT id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count, \
             error,started_at,finished_at FROM intel_sync_runs ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.target)
        .await
        .context("读取最近情报任务失败")?;
        let success_row = sqlx::query(
            "SELECT id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count, \
             error,started_at,finished_at FROM intel_sync_runs WHERE status='succeeded' ORDER BY id DESC LIMIT 1",
        )
        .fetch_optional(&self.target)
        .await
        .context("读取最近成功情报任务失败")?;
        Ok(IntelSyncStatus {
            configured: true,
            interval_seconds: Some(self.interval.as_secs()),
            ip_count: Some(ip_count),
            domain_count: Some(domain_count),
            latest_run: row.map(parse_run).transpose()?,
            latest_success_run: success_row.map(parse_run).transpose()?,
            error: None,
        })
    }

    /// 分页读取持久化执行历史。
    pub async fn history(&self, limit: i64, offset: i64) -> Result<RunHistory<IntelRun>> {
        let total: i64 = sqlx::query_scalar("SELECT count(*) FROM intel_sync_runs")
            .fetch_one(&self.target)
            .await?;
        let rows = sqlx::query(
            "SELECT id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count, \
             error,started_at,finished_at FROM intel_sync_runs \
             ORDER BY id DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.target)
        .await?;
        Ok(RunHistory {
            items: rows
                .into_iter()
                .map(parse_run)
                .collect::<Result<Vec<_>>>()?,
            total,
        })
    }

    /// 仅清空情报任务执行记录，Redis 正式情报集合保持不变。
    pub async fn clear_history(&self) -> Result<u64> {
        let mut transaction = self
            .target
            .begin()
            .await
            .context("开启情报历史清理事务失败")?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM intel_sync_runs WHERE status='running')",
        )
        .fetch_one(&mut *transaction)
        .await?;
        if running {
            bail!("情报同步正在执行，完成后才能清空历史");
        }
        let deleted = sqlx::query("DELETE FROM intel_sync_runs")
            .execute(&mut *transaction)
            .await
            .context("清空情报同步历史失败")?
            .rows_affected();
        transaction.commit().await.context("提交情报历史清理失败")?;
        Ok(deleted)
    }

    /// 按 Unix 时间边界对齐周期；启动时等待下一个边界，同一进程仅执行一个任务。
    pub fn spawn_timer(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(duration_until_next_tick(service.interval)).await;
                if let Err(error) = service.run_once().await {
                    warn!("情报定时同步失败: error={:#}", error);
                }
            }
        });
    }

    /// 一次全量刷新；公开供本地集成验证调用。
    pub async fn run_once(&self) -> Result<IntelRun> {
        let row = sqlx::query(
            "INSERT INTO intel_sync_runs(status,stage,started_at) VALUES ('running','reading',strftime('%Y-%m-%dT%H:%M:%fZ','now')) \
             RETURNING id",
        )
        .fetch_one(&self.target)
        .await
        .context("创建情报任务失败，可能已有任务正在执行")?;
        let id: i64 = row.try_get("id")?;
        info!("情报同步开始: run_id={}", id);
        let result = self.execute(id).await;
        match result {
            Ok((read, invalid, expired, ip, domain)) => {
                sqlx::query(
                    "UPDATE intel_sync_runs SET status='succeeded',stage='completed', \
                     read_count=$2,invalid_count=$3,expired_count=$4,ip_count=$5,domain_count=$6, \
                     finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1",
                )
                .bind(id)
                .bind(read)
                .bind(invalid)
                .bind(expired)
                .bind(ip)
                .bind(domain)
                .execute(&self.target)
                .await
                .context("记录情报同步成功失败")?;
                info!(
                    "情报同步完成: run_id={}, read={}, ip={}, domain={}",
                    id, read, ip, domain
                );
            }
            Err(error) => {
                let message = format!("{error:#}");
                sqlx::query(
                    "UPDATE intel_sync_runs SET status='failed',stage='failed', \
                     error=$2,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1",
                )
                .bind(id)
                .bind(message.chars().take(500).collect::<String>())
                .execute(&self.target)
                .await
                .context("记录情报同步失败原因失败")?;
                bail!("{message}");
            }
        }
        self.run(id).await?.context("情报运行记录不存在")
    }

    async fn run(&self, id: i64) -> Result<Option<IntelRun>> {
        let row = sqlx::query(
            "SELECT id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count, \
             error,started_at,finished_at FROM intel_sync_runs WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&self.target)
        .await?;
        row.map(parse_run).transpose()
    }

    async fn execute(&self, id: i64) -> Result<(i64, i64, i64, i64, i64)> {
        let ip_stage = format!("{IP_KEY}:staging:{id}");
        let domain_stage = format!("{DOMAIN_KEY}:staging:{id}");
        let mut redis = self.redis.get_multiplexed_async_connection().await?;
        let result = self
            .build_snapshot(&mut redis, id, &ip_stage, &domain_stage)
            .await;
        if result.is_err() {
            let _: redis::RedisResult<()> = redis::cmd("DEL")
                .arg(&ip_stage)
                .arg(&domain_stage)
                .query_async(&mut redis)
                .await;
        }
        result
    }

    async fn build_snapshot(
        &self,
        redis: &mut redis::aio::MultiplexedConnection,
        id: i64,
        ip_stage: &str,
        domain_stage: &str,
    ) -> Result<(i64, i64, i64, i64, i64)> {
        let _: i64 = redis::cmd("DEL")
            .arg(ip_stage)
            .arg(domain_stage)
            .query_async(redis)
            .await?;
        let mut after_id = String::new();
        let mut read = 0i64;
        let mut invalid = 0i64;
        let mut expired = 0i64;
        loop {
            let rows = sqlx::query(
                "SELECT id,x_ioc_type,x_ioc_value, \
                 CASE WHEN x_valid_until IS NULL OR x_valid_until > NOW() THEN 'Y' ELSE 'N' END AS active \
                 FROM intel.stix_object WHERE type='indicator' \
                 AND x_ioc_type IN ('ipv4-addr','ipv6-addr','domain-name') \
                 AND id > ? ORDER BY id LIMIT 5000",
            ).bind(&after_id).fetch_all(&self.doris).await
                .context("分页读取 Doris 情报失败")?;
            if rows.is_empty() {
                break;
            }
            let page_len = rows.len();
            let mut ips = Vec::new();
            let mut domains = Vec::new();
            for row in rows {
                after_id = row.try_get("id")?;
                read += 1;
                let ioc_type: String = row.try_get("x_ioc_type")?;
                let value: Option<String> = row.try_get("x_ioc_value")?;
                let active: String = row.try_get("active")?;
                if active != "Y" {
                    expired += 1;
                    continue;
                }
                let Some(value) = value else {
                    invalid += 1;
                    continue;
                };
                if ioc_type == "domain-name" {
                    if let Some(value) = normalize_domain(&value) {
                        domains.push(value);
                    } else {
                        invalid += 1;
                    }
                } else if let Ok(ip) = IpAddr::from_str(value.trim()) {
                    if (ioc_type == "ipv4-addr" && ip.is_ipv4())
                        || (ioc_type == "ipv6-addr" && ip.is_ipv6())
                    {
                        ips.push(ip.to_string());
                    } else {
                        invalid += 1;
                    }
                } else {
                    invalid += 1;
                }
            }
            if !ips.is_empty() {
                let _: i64 = redis::cmd("SADD")
                    .arg(ip_stage)
                    .arg(ips)
                    .query_async(redis)
                    .await
                    .context("写入 IP 临时集合失败")?;
            }
            if !domains.is_empty() {
                let _: i64 = redis::cmd("SADD")
                    .arg(domain_stage)
                    .arg(domains)
                    .query_async(redis)
                    .await
                    .context("写入域名临时集合失败")?;
            }
            let _: bool = redis::cmd("EXPIRE")
                .arg(ip_stage)
                .arg(7200)
                .query_async(redis)
                .await
                .context("设置 IP 临时集合过期时间失败")?;
            let _: bool = redis::cmd("EXPIRE")
                .arg(domain_stage)
                .arg(7200)
                .query_async(redis)
                .await
                .context("设置域名临时集合过期时间失败")?;
            sqlx::query(
                "UPDATE intel_sync_runs SET read_count=$2,invalid_count=$3,expired_count=$4 \
                 WHERE id=$1",
            )
            .bind(id)
            .bind(read)
            .bind(invalid)
            .bind(expired)
            .execute(&self.target)
            .await?;
            if page_len < PAGE_SIZE as usize {
                break;
            }
        }
        if read == 0 {
            bail!("Doris 情报结果为空，保留上一版 Redis 集合");
        }
        let ip_count: i64 = redis.scard(ip_stage).await?;
        let domain_count: i64 = redis.scard(domain_stage).await?;
        if ip_count + domain_count == 0 {
            bail!("有效情报结果为空，保留上一版 Redis 集合");
        }
        sqlx::query("UPDATE intel_sync_runs SET stage='switching' WHERE id=$1")
            .bind(id)
            .execute(&self.target)
            .await?;
        // 发布前完整校验，避免 Lua 执行期间发生类型或数量错误而只切换一部分键。
        // Lua 作为单个 Redis 命令执行，业务读取不会看见两个正式键的中间状态。
        let script = r#"
            local keys = {KEYS[1], KEYS[2]}
            local staging = {KEYS[3], KEYS[4]}
            for i=1,2 do
                local source_type = redis.call('TYPE', staging[i]).ok
                local target_type = redis.call('TYPE', keys[i]).ok
                if source_type ~= 'none' and source_type ~= 'set' then
                    return redis.error_reply('invalid staging key type')
                end
                if target_type ~= 'none' and target_type ~= 'set' then
                    return redis.error_reply('invalid target key type')
                end
                if redis.call('SCARD', staging[i]) ~= tonumber(ARGV[i]) then
                    return redis.error_reply('staging key count changed')
                end
            end
            for i=1,2 do
                if redis.call('EXISTS', staging[i]) == 1 then
                    redis.call('RENAME', staging[i], keys[i])
                    redis.call('PERSIST', keys[i])
                else
                    redis.call('DEL', keys[i])
                end
            end
            redis.call('DEL', KEYS[5], KEYS[6])
            return 1
        "#;
        let _: i64 = redis::cmd("EVAL")
            .arg(script)
            .arg(6)
            .arg(IP_KEY)
            .arg(DOMAIN_KEY)
            .arg(ip_stage)
            .arg(domain_stage)
            .arg("{intel}:ip")
            .arg("{intel}:domain")
            .arg(ip_count)
            .arg(domain_count)
            .query_async(redis)
            .await
            .context("切换 Redis 情报集合失败")?;
        Ok((read, invalid, expired, ip_count, domain_count))
    }
}

/// 计算距离下一个周期边界的时间，避免任务周期随 Station 启动时刻漂移。
fn duration_until_next_tick(period: Duration) -> Duration {
    let period_nanos = period.as_nanos();
    if period_nanos == 0 {
        return Duration::ZERO;
    }
    let elapsed_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let remainder = elapsed_nanos % period_nanos;
    let wait_nanos = if remainder == 0 {
        period_nanos
    } else {
        period_nanos - remainder
    };
    Duration::new(
        (wait_nanos / 1_000_000_000) as u64,
        (wait_nanos % 1_000_000_000) as u32,
    )
}

fn normalize_domain(value: &str) -> Option<String> {
    let domain = value.trim().trim_end_matches('.').to_ascii_lowercase();
    if domain.len() > 253 || domain.is_empty() || !domain.is_ascii() {
        return None;
    }
    if domain.split('.').any(|label| {
        label.is_empty()
            || label.len() > 63
            || label.starts_with('-')
            || label.ends_with('-')
            || !label
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
    }) {
        return None;
    }
    Some(domain)
}

fn parse_run(row: sqlx::sqlite::SqliteRow) -> Result<IntelRun> {
    Ok(IntelRun {
        id: row.try_get("id")?,
        status: row.try_get("status")?,
        stage: row.try_get("stage")?,
        read_count: row.try_get("read_count")?,
        invalid_count: row.try_get("invalid_count")?,
        expired_count: row.try_get("expired_count")?,
        ip_count: row.try_get("ip_count")?,
        domain_count: row.try_get("domain_count")?,
        error: row.try_get("error")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
    })
}
