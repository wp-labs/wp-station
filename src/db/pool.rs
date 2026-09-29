//! 数据库连接池管理模块。

use crate::error::{DbError, DbResult};
use crate::server::{DatabaseConf, DatabaseKind};
use lazy_static::lazy_static;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
use std::fs;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

lazy_static! {
    static ref GLOBAL_POOL: RwLock<Option<DbPool>> = RwLock::new(None);
}

/// 全局数据库连接池包装。
#[derive(Clone)]
pub struct DbPool {
    conn: DatabaseConnection,
}

impl DbPool {
    /// 创建新的数据库连接池
    pub async fn new(
        kind: DatabaseKind,
        database_url: &str,
        max_connections: u32,
        min_connections: u32,
        connect_timeout_secs: u64,
        idle_timeout_secs: u64,
    ) -> DbResult<Self> {
        info!(
            "创建数据库连接池: max_connections={}, min_connections={}, connect_timeout={}s, idle_timeout={}s",
            max_connections, min_connections, connect_timeout_secs, idle_timeout_secs
        );

        let mut opt = ConnectOptions::new(database_url.to_string());
        opt.max_connections(max_connections)
            .min_connections(min_connections)
            .connect_timeout(Duration::from_secs(connect_timeout_secs))
            .idle_timeout(Duration::from_secs(idle_timeout_secs))
            .sqlx_logging(false);

        let conn = Database::connect(opt).await?;
        if matches!(kind, DatabaseKind::Sqlite) {
            apply_sqlite_pragmas(&conn).await?;
        }
        info!("数据库连接池创建成功");
        Ok(Self { conn })
    }

    /// 获取内部数据库连接
    pub fn inner(&self) -> &DatabaseConnection {
        &self.conn
    }

    /// 测试数据库连接
    pub async fn test_connection(&self) -> DbResult<()> {
        self.conn.ping().await?;
        Ok(())
    }
}

/// 初始化全局数据库连接池（应用启动时调用一次）
pub async fn init_pool(config: &DatabaseConf) -> DbResult<()> {
    info!("初始化全局数据库连接池: {}", config.safe_summary());

    let database_kind = config.database_kind();
    let conn_str = config.connection_string_with_options();
    if matches!(database_kind, DatabaseKind::Sqlite) {
        ensure_sqlite_parent_dir(&conn_str)?;
    }
    let pool = DbPool::new(
        database_kind,
        &conn_str,
        config.max_connections,
        config.min_connections,
        config.connect_timeout,
        config.idle_timeout,
    )
    .await?;

    let mut global = match GLOBAL_POOL.write() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("数据库连接池写锁已被污染，继续使用内部状态");
            poisoned.into_inner()
        }
    };
    *global = Some(pool);

    info!("全局数据库连接池初始化成功");
    Ok(())
}

async fn apply_sqlite_pragmas(conn: &DatabaseConnection) -> DbResult<()> {
    for pragma in [
        "PRAGMA journal_mode = WAL;",
        "PRAGMA synchronous = NORMAL;",
        "PRAGMA busy_timeout = 5000;",
        "PRAGMA foreign_keys = ON;",
    ] {
        conn.execute(Statement::from_string(
            DbBackend::Sqlite,
            pragma.to_string(),
        ))
        .await?;
    }
    Ok(())
}

fn ensure_sqlite_parent_dir(database_url: &str) -> DbResult<()> {
    let sqlite_path = database_url
        .trim()
        .trim_start_matches("sqlite://")
        .trim_start_matches("sqlite:")
        .split(['?', '#'])
        .next()
        .unwrap_or("");
    if sqlite_path.is_empty() || sqlite_path == ":memory:" {
        return Ok(());
    }

    let path = PathBuf::from(sqlite_path);
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|err| DbError::Db(sea_orm::DbErr::Custom(err.to_string())))?;
    }
    if !path.exists() {
        fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .map_err(|err| DbError::Db(sea_orm::DbErr::Custom(err.to_string())))?;
    }
    Ok(())
}

/// 获取全局数据库连接池
///
/// # Panics
/// 如果连接池未初始化，将返回 panic。请确保在应用启动时调用 `init_pool()`。
pub fn get_pool() -> DbPool {
    try_get_pool()
        .unwrap_or_else(|| panic!("数据库连接池未初始化！请确保在应用启动时调用了 init_pool()"))
}

/// 尝试获取全局数据库连接池（不会 panic）
///
/// # Returns
/// - `Some(DbPool)` - 连接池已初始化
/// - `None` - 连接池未初始化
pub fn try_get_pool() -> Option<DbPool> {
    let global = match GLOBAL_POOL.read() {
        Ok(guard) => guard,
        Err(poisoned) => {
            warn!("数据库连接池读锁已被污染，继续使用内部状态");
            poisoned.into_inner()
        }
    };

    global.clone()
}

/// 检查连接池是否已初始化
pub fn is_pool_initialized() -> bool {
    match GLOBAL_POOL.read() {
        Ok(guard) => guard.is_some(),
        Err(poisoned) => {
            warn!("数据库连接池读锁已被污染，继续使用内部状态");
            poisoned.into_inner().is_some()
        }
    }
}
