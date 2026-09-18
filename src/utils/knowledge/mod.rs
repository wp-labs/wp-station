//! 知识库生命周期与查询模块。
//!
//! 包含两类职责：
//! - 知识库 provider / authority 的加载、卸载和重载；
//! - 基于 `wp-knowledge` 的 SQL 查询辅助。

mod lifecycle;
mod query;

use lazy_static::lazy_static;
use std::path::PathBuf;
use std::sync::RwLock;

/// 当前进程中已加载的知识库来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KnowledgeLoadedSource {
    Configured,
    SqliteAuthority,
}

struct KnowledgeContext {
    root: PathBuf,
    knowdb_path: PathBuf,
    auth_path: PathBuf,
    auth_uri: String,
}

const LEGACY_PROVIDER_FORMAT_MESSAGE: &str = "knowdb.toml 使用了旧版 [provider] 配置格式；升级到 wp-knowledge 0.14+ 后，请改为 [provider.sqldb] 或 [provider.redis]。当前旧格式会回退到本地 authority，无法查询远程数据库。";

lazy_static! {
    /// 当前知识库运行时已加载的数据源类型。
    static ref KNOWLEDGE_LOADED: RwLock<Option<KnowledgeLoadedSource>> = RwLock::new(None);
}

pub use lifecycle::{
    configured_provider_names, is_knowledge_loaded, load_knowledge, load_sqlite_knowledge,
    reload_knowledge, reload_sqlite_knowledge, should_reload_knowledge_source, unload_knowledge,
};
pub use query::{db_init, sql_knowdb_list, sql_query, sql_query_rows, sql_query_rows_for};
