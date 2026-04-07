// 数据库连接池管理 - 全局单例模式

use crate::error::DbResult;
use crate::server::DatabaseConf;
use lazy_static::lazy_static;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use std::sync::RwLock;
use std::time::Duration;

lazy_static! {
    static ref GLOBAL_POOL: RwLock<Option<DbPool>> = RwLock::new(None);
}

#[derive(Clone)]
pub struct DbPool {
    conn: DatabaseConnection,
}

impl DbPool {
    /// 创建新的数据库连接池
    pub async fn new(
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

    let conn_str = config.connection_string_with_options();
    let pool = DbPool::new(
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
