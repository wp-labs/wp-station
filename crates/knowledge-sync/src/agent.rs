//! 从 Prometheus 查询 Agent 与主机 IP，并全量发布到 Redis。

use std::collections::BTreeMap;
use std::env;
use std::net::IpAddr;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tracing::{info, warn};

use crate::RunHistory;

const QUERY: &str = "group by (agent, host_ip) (present_over_time(system_host_info[1d]))";
const REGISTRY_KEY: &str = "agent_ip:registry";
const PUBLISH_SCRIPT: &str = r#"
local stage, registry = KEYS[1], KEYS[2]
if redis.call('TYPE', stage).ok ~= 'hash' then return redis.error_reply('agent stage is not a hash') end
local pairs = redis.call('HGETALL', stage)
if #pairs == 0 then return redis.error_reply('agent stage is empty') end
local old = redis.call('SMEMBERS', registry)
local incoming = {}
for i = 1, #pairs, 2 do incoming[pairs[i]] = true end
for _, agent in ipairs(old) do
  if not incoming[agent] then redis.call('DEL', agent) end
end
for i = 1, #pairs, 2 do redis.call('SET', pairs[i], pairs[i + 1]) end
redis.call('DEL', registry)
for i = 1, #pairs, 2 do redis.call('SADD', registry, pairs[i]) end
redis.call('DEL', stage)
return #pairs / 2
"#;

/// 部署时配置 Prometheus 地址，Redis 复用情报同步的连接。
#[derive(Clone)]
pub struct AgentSyncConfig {
    pub prometheus_url: String,
    pub redis_url: String,
    pub interval: Duration,
}

impl AgentSyncConfig {
    /// 未配置 Prometheus 地址时不启用任务。
    pub fn from_env() -> Result<Option<Self>> {
        let Ok(prometheus_url) = env::var("WP_KSYNC_AGENT_PROMETHEUS_URL") else {
            return Ok(None);
        };
        if prometheus_url.trim().is_empty() {
            return Ok(None);
        }
        let redis_url = env::var("WP_KSYNC_REDIS_URL")
            .context("配置 Agent 同步时必须设置 WP_KSYNC_REDIS_URL")?;
        let seconds: u64 = env::var("WP_KSYNC_AGENT_INTERVAL")
            .unwrap_or_else(|_| "300".into())
            .parse()
            .context("Agent 同步周期无效")?;
        if seconds < 60 {
            bail!("Agent 同步周期不能短于 60 秒");
        }
        Ok(Some(Self {
            prometheus_url,
            redis_url,
            interval: Duration::from_secs(seconds),
        }))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentRun {
    pub id: i64,
    pub status: String,
    pub read_count: i64,
    pub agent_count: i64,
    pub error: Option<String>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentSyncStatus {
    pub configured: bool,
    pub interval_seconds: Option<u64>,
    pub agent_count: Option<i64>,
    pub latest_run: Option<AgentRun>,
    pub latest_success_run: Option<AgentRun>,
    pub error: Option<String>,
}

impl AgentSyncStatus {
    pub fn unconfigured() -> Self {
        Self {
            configured: false,
            interval_seconds: None,
            agent_count: None,
            latest_run: None,
            latest_success_run: None,
            error: None,
        }
    }

    pub fn unavailable(error: impl Into<String>) -> Self {
        Self {
            configured: true,
            error: Some(error.into()),
            ..Self::unconfigured()
        }
    }
}

#[derive(Clone)]
pub struct AgentSyncService {
    http: reqwest::Client,
    url: String,
    redis: redis::Client,
    runs: SqlitePool,
    interval: Duration,
}

impl AgentSyncService {
    pub async fn connect(config: AgentSyncConfig, runs: SqlitePool) -> Result<Self> {
        let url =
            reqwest::Url::parse(&config.prometheus_url).context("解析 Prometheus URL 失败")?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("Prometheus URL 只支持 HTTP 或 HTTPS");
        }
        let redis = redis::Client::open(config.redis_url).context("解析 Redis URL 失败")?;
        let mut connection = redis
            .get_multiplexed_async_connection()
            .await
            .context("连接 Agent 同步 Redis 失败")?;
        let _: String = redis::cmd("PING")
            .query_async(&mut connection)
            .await
            .context("检查 Agent 同步 Redis 失败")?;
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()?,
            url: url.to_string(),
            redis,
            runs,
            interval: config.interval,
        })
    }

    pub async fn initialize(&self) -> Result<AgentSyncStatus> {
        sqlx::query("UPDATE agent_sync_runs SET status='interrupted', error='Station 重启导致任务中断', finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE status='running'")
            .execute(&self.runs).await?;
        self.status().await
    }

    pub async fn status(&self) -> Result<AgentSyncStatus> {
        let mut redis = self.redis.get_multiplexed_async_connection().await?;
        let count: i64 = redis::cmd("SCARD")
            .arg(REGISTRY_KEY)
            .query_async(&mut redis)
            .await?;
        let latest = self
            .read_run("SELECT * FROM agent_sync_runs ORDER BY id DESC LIMIT 1")
            .await?;
        let success = self
            .read_run(
                "SELECT * FROM agent_sync_runs WHERE status='succeeded' ORDER BY id DESC LIMIT 1",
            )
            .await?;
        Ok(AgentSyncStatus {
            configured: true,
            interval_seconds: Some(self.interval.as_secs()),
            agent_count: Some(count),
            latest_run: latest,
            latest_success_run: success,
            error: None,
        })
    }

    async fn read_run(&self, query: &str) -> Result<Option<AgentRun>> {
        sqlx::query(query)
            .fetch_optional(&self.runs)
            .await?
            .map(parse_run)
            .transpose()
    }

    pub async fn history(&self, limit: i64, offset: i64) -> Result<RunHistory<AgentRun>> {
        let total = sqlx::query_scalar("SELECT count(*) FROM agent_sync_runs")
            .fetch_one(&self.runs)
            .await?;
        let rows = sqlx::query("SELECT * FROM agent_sync_runs ORDER BY id DESC LIMIT $1 OFFSET $2")
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.runs)
            .await?;
        Ok(RunHistory {
            total,
            items: rows.into_iter().map(parse_run).collect::<Result<_>>()?,
        })
    }

    pub async fn clear_history(&self) -> Result<u64> {
        let mut tx = self.runs.begin().await?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agent_sync_runs WHERE status='running')",
        )
        .fetch_one(&mut *tx)
        .await?;
        if running {
            bail!("Agent 同步正在执行，完成后才能清空历史");
        }
        let count = sqlx::query("DELETE FROM agent_sync_runs")
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(count)
    }

    /// 启动后等待下一个整周期边界；不在启动时查询 Prometheus。
    pub fn spawn_timer(&self) {
        let service = self.clone();
        tokio::spawn(async move {
            loop {
                let seconds = service.interval.as_secs();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                tokio::time::sleep(Duration::from_secs(seconds - now % seconds)).await;
                if let Err(error) = service.run_once().await {
                    warn!("Agent 定时同步失败: error={:#}", error);
                }
            }
        });
    }

    pub async fn run_once(&self) -> Result<AgentRun> {
        let id: i64 = sqlx::query_scalar("INSERT INTO agent_sync_runs(status,started_at) VALUES('running',strftime('%Y-%m-%dT%H:%M:%fZ','now')) RETURNING id")
            .fetch_one(&self.runs).await.context("创建 Agent 同步记录失败，可能已有任务在执行")?;
        info!("Agent 同步开始: run_id={}", id);
        let result = self.execute(id).await;
        match result {
            Ok((read_count, agent_count)) => {
                sqlx::query("UPDATE agent_sync_runs SET status='succeeded',read_count=$2,agent_count=$3,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1")
                    .bind(id).bind(read_count).bind(agent_count).execute(&self.runs).await?;
                info!(
                    "Agent 同步完成: run_id={}, read={}, agents={}",
                    id, read_count, agent_count
                );
            }
            Err(error) => {
                let message = format!("{error:#}");
                sqlx::query("UPDATE agent_sync_runs SET status='failed',error=$2,finished_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=$1")
                    .bind(id).bind(message.chars().take(500).collect::<String>()).execute(&self.runs).await?;
                bail!("{message}");
            }
        }
        self.read_run(&format!("SELECT * FROM agent_sync_runs WHERE id={id}"))
            .await?
            .context("Agent 运行记录不存在")
    }

    async fn execute(&self, id: i64) -> Result<(i64, i64)> {
        let end = Utc::now();
        let start = end - chrono::Duration::days(2);
        let response = self
            .http
            .get(&self.url)
            .query(&[
                ("query", QUERY),
                ("start", &start.to_rfc3339()),
                ("end", &end.to_rfc3339()),
                ("step", "1d"),
            ])
            .send()
            .await
            .context("请求 Prometheus 失败")?
            .error_for_status()
            .context("Prometheus 返回 HTTP 错误")?
            .json::<PrometheusResponse>()
            .await
            .context("解析 Prometheus 响应失败")?;
        let (mapping, read_count) = parse_mapping(response)?;
        let stage = format!("agent_ip:staging:{id}");
        let mut redis = self.redis.get_multiplexed_async_connection().await?;
        let result = async {
            let mut pipe = redis::pipe();
            pipe.atomic().cmd("DEL").arg(&stage).ignore();
            for (agent, ip) in &mapping {
                pipe.cmd("HSET").arg(&stage).arg(agent).arg(ip).ignore();
            }
            pipe.query_async::<_, ()>(&mut redis)
                .await
                .context("写入 Agent 临时映射失败")?;
            let count: i64 = redis::Script::new(PUBLISH_SCRIPT)
                .key(&stage)
                .key(REGISTRY_KEY)
                .invoke_async(&mut redis)
                .await
                .context("发布 Agent 映射失败")?;
            Ok::<i64, anyhow::Error>(count)
        }
        .await;
        if result.is_err() {
            let _: redis::RedisResult<()> =
                redis::cmd("DEL").arg(&stage).query_async(&mut redis).await;
        }
        Ok((read_count, result?))
    }
}

#[derive(Deserialize)]
struct PrometheusResponse {
    status: String,
    data: Option<PrometheusData>,
    error: Option<String>,
}

#[derive(Deserialize)]
struct PrometheusData {
    #[serde(rename = "resultType")]
    result_type: String,
    result: Vec<PrometheusSeries>,
}

#[derive(Deserialize)]
struct PrometheusSeries {
    metric: BTreeMap<String, String>,
    values: Vec<(f64, String)>,
}

fn parse_mapping(response: PrometheusResponse) -> Result<(BTreeMap<String, String>, i64)> {
    if response.status != "success" {
        bail!(
            "Prometheus 查询失败: {}",
            response.error.unwrap_or_default()
        );
    }
    let data = response.data.context("Prometheus 响应缺少 data")?;
    if data.result_type != "matrix" {
        bail!("Prometheus 响应不是 matrix");
    }
    let read_count = data.result.len() as i64;
    let mut mapping = BTreeMap::new();
    for series in data.result {
        if !series
            .values
            .iter()
            .any(|(_, value)| value.parse::<f64>().is_ok_and(|n| n > 0.0))
        {
            continue;
        }
        let agent = series
            .metric
            .get("agent")
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        let ip = series
            .metric
            .get("host_ip")
            .map(String::as_str)
            .unwrap_or("")
            .trim();
        if agent.is_empty()
            || agent == REGISTRY_KEY
            || agent.starts_with("agent_ip:staging:")
            || ip.parse::<IpAddr>().is_err()
        {
            bail!("Prometheus 返回无效的 agent 或 host_ip");
        }
        match mapping.insert(agent.to_owned(), ip.to_owned()) {
            Some(previous) if previous != ip => bail!("Agent {agent} 对应多个 IP，拒绝覆盖旧映射"),
            _ => {}
        }
    }
    if mapping.is_empty() {
        bail!("Prometheus 查询结果为空，保留已有 Agent 映射");
    }
    Ok((mapping, read_count))
}

/// 校验 Prometheus 响应并提取可发布的 Agent 映射。
pub fn parse_agent_mapping_json(body: &str) -> Result<(BTreeMap<String, String>, i64)> {
    parse_mapping(serde_json::from_str(body).context("解析 Prometheus JSON 失败")?)
}

fn parse_run(row: sqlx::sqlite::SqliteRow) -> Result<AgentRun> {
    let started: String = row.try_get("started_at")?;
    let finished: Option<String> = row.try_get("finished_at")?;
    Ok(AgentRun {
        id: row.try_get("id")?,
        status: row.try_get("status")?,
        read_count: row.try_get("read_count")?,
        agent_count: row.try_get("agent_count")?,
        error: row.try_get("error")?,
        started_at: DateTime::parse_from_rfc3339(&started)?.with_timezone(&Utc),
        finished_at: finished
            .map(|value| DateTime::parse_from_rfc3339(&value).map(|time| time.with_timezone(&Utc)))
            .transpose()?,
    })
}
