//! 知识库生命周期管理模块。
//!
//! 负责知识库的加载、卸载、重载以及 SQL 查询等操作，支持配置数据源和本地 authority 两种模式。

use crate::error::AppError;
use crate::server::ProjectLayout;
use lazy_static::lazy_static;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;
use tracing::{debug, error, info, warn};
use wp_knowledge::facade;
use wp_knowledge::loader::{self, ProviderKind};
use wp_knowledge::mem::RowData;
use wp_model_core::model::DataField;

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

lazy_static! {
    /// 当前知识库运行时已加载的数据源类型。
    static ref KNOWLEDGE_LOADED: RwLock<Option<KnowledgeLoadedSource>> = RwLock::new(None);
}

pub fn db_init() -> anyhow::Result<Vec<DataField>> {
    //todo 写一个加载 project_root/models/knowledge下所有数据的方法
    Ok(vec![])
}

pub async fn sql_query(sql: &str) -> anyhow::Result<Vec<DataField>> {
    let rows: Vec<RowData> = sql_query_rows(sql).await?;
    Ok(rows.into_iter().next().unwrap_or_default())
}

/// 直接通过 wp-knowledge provider 执行 SQL，支持 knowdb.toml 配置的真实数据库。
pub async fn sql_query_rows(sql: &str) -> anyhow::Result<Vec<RowData>> {
    let rows: Vec<RowData> = facade::query_async(sql)
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))?;
    debug!("知识库工具执行 SQL 查询完成: rows={}", rows.len());
    Ok(rows)
}

pub async fn sql_knowdb_list() -> anyhow::Result<Vec<String>> {
    let sql = r#"SELECT GROUP_CONCAT(name, ', ') as name FROM sqlite_master WHERE type='table'"#;
    let result: RowData = sql_query(sql).await?;
    debug!("知识库工具查询数据表列表完成");
    match result.first() {
        Some(value) => {
            let list = format!("{}", value.get_value());
            let items: Vec<String> = list
                .split(',')
                .map(|s: &str| s.trim().to_string())
                .collect();
            debug!("知识库工具查询数据表列表成功: count={}", items.len());
            Ok(items)
        }
        None => Ok(vec![]),
    }
}

/// 检查知识库是否已加载
pub fn is_knowledge_loaded() -> bool {
    KNOWLEDGE_LOADED.read().unwrap().is_some()
}

fn is_loaded_source(source: KnowledgeLoadedSource) -> bool {
    *KNOWLEDGE_LOADED.read().unwrap() == Some(source)
}

fn set_loaded_source(source: KnowledgeLoadedSource) {
    *KNOWLEDGE_LOADED.write().unwrap() = Some(source);
}

/// 读取 knowdb.toml 中声明的 provider 名称。
pub fn configured_provider_name(layout: &ProjectLayout) -> Result<Option<String>, AppError> {
    let Some(context) = build_knowledge_context(layout)? else {
        return Ok(None);
    };

    let dict = Default::default();
    let (conf, _, _) = loader::parse_knowdb_conf(&context.root, &context.knowdb_path, &dict)
        .map_err(|e| {
            error!("解析 knowdb 配置失败: {}", e);
            AppError::internal(e)
        })?;

    Ok(conf.provider.map(|provider| match provider.kind {
        ProviderKind::Postgres => "postgres".to_string(),
        ProviderKind::Mysql => "mysql".to_string(),
        ProviderKind::SqliteAuthority => "sqlite".to_string(),
    }))
}

/// 加载 knowdb.toml 当前声明的数据源；若配置了 provider，则走正式数据库。
pub fn load_knowledge(layout: &ProjectLayout) -> anyhow::Result<()> {
    if is_loaded_source(KnowledgeLoadedSource::Configured) {
        info!("知识库已加载，跳过初始化");
        return Ok(());
    }

    let Some(context) = build_knowledge_context(layout)? else {
        return Ok(());
    };

    info!(
        "初始化知识库: root={}, knowdb={}",
        context.root.display(),
        context.knowdb_path.display()
    );

    let dict = Default::default();
    match facade::init_thread_cloned_from_knowdb(
        &context.root,
        &context.knowdb_path,
        &context.auth_uri,
        &dict,
    ) {
        Ok(_) => {
            info!("知识库初始化成功");
        }
        Err(e) => {
            let error_msg = format!("{:?}", e);
            // 如果是 "already initialized" 错误，视为成功（wp_knowledge 是全局单例）
            if error_msg.contains("already initialized") {
                info!("知识库提供者已初始化（全局单例），继续使用");
            } else {
                error!("初始化知识库失败：{:?}", e);
                return Err(AppError::internal(e).into());
            }
        }
    }

    set_loaded_source(KnowledgeLoadedSource::Configured);

    Ok(())
}

/// 强制加载本地 authority sqlite，忽略 knowdb.toml 中的 provider，供调试页查询本地知识库使用。
pub fn load_sqlite_knowledge(layout: &ProjectLayout) -> anyhow::Result<()> {
    if is_loaded_source(KnowledgeLoadedSource::SqliteAuthority) {
        info!("本地知识库已加载，跳过初始化");
        return Ok(());
    }

    let Some(context) = build_knowledge_context(layout)? else {
        return Ok(());
    };

    info!(
        "初始化本地知识库 authority: root={}, knowdb={}",
        context.root.display(),
        context.knowdb_path.display()
    );

    let dict = Default::default();
    let (conf, _, _) = loader::parse_knowdb_conf(&context.root, &context.knowdb_path, &dict)
        .map_err(|e| {
            error!("解析 knowdb 配置失败: {}", e);
            AppError::internal(e)
        })?;

    loader::build_authority_from_knowdb(
        &context.root,
        &context.knowdb_path,
        &context.auth_uri,
        &dict,
    )
    .map_err(|e| {
        error!("构建本地知识库 authority 失败: {}", e);
        AppError::internal(e)
    })?;

    let ro_uri = format!("file:{}?mode=ro&uri=true", context.auth_path.display());
    facade::init_thread_cloned_from_authority(&ro_uri).map_err(|e| {
        error!("初始化本地知识库 authority 失败: {}", e);
        AppError::internal(e)
    })?;

    wp_knowledge::runtime::runtime().configure_result_cache(
        conf.cache.enabled,
        conf.cache.capacity,
        Duration::from_millis(conf.cache.ttl_ms.max(1)),
    );

    info!("本地知识库 authority 初始化成功");
    set_loaded_source(KnowledgeLoadedSource::SqliteAuthority);

    Ok(())
}

/// 重置 Station 侧记录的已加载来源；wp-knowledge runtime 会在下次加载时覆盖 provider。
pub fn unload_knowledge() {
    let mut loaded = KNOWLEDGE_LOADED.write().unwrap();
    if loaded.is_some() {
        *loaded = None;
        info!("知识库已卸载");
    }
}

/// 重新加载知识库
pub fn reload_knowledge(layout: &ProjectLayout) -> anyhow::Result<()> {
    unload_knowledge();
    load_knowledge(layout)
}

/// 重新加载本地 authority sqlite。
pub fn reload_sqlite_knowledge(layout: &ProjectLayout) -> anyhow::Result<()> {
    unload_knowledge();
    load_sqlite_knowledge(layout)
}

fn build_knowledge_context(layout: &ProjectLayout) -> Result<Option<KnowledgeContext>, AppError> {
    let Some(root) = ensure_models_root_exists(&layout.models_root)? else {
        return Ok(None);
    };

    let knowledge_root = root.join("models").join("knowledge");
    if !knowledge_root.exists() {
        warn!(
            "项目中未找到知识库目录，跳过初始化: {}",
            knowledge_root.display()
        );
        return Ok(None);
    }

    let knowdb_path = knowledge_root.join("knowdb.toml");
    if !knowdb_path.exists() {
        warn!(
            "未检测到知识库配置文件，跳过加载: {}",
            knowdb_path.display()
        );
        return Ok(None);
    }

    let run_dir = root.join(".run");
    if !run_dir.exists() {
        fs::create_dir_all(&run_dir).map_err(|e| {
            error!("创建运行目录失败: {}", e);
            AppError::internal(e)
        })?;
    }

    let auth_path = run_dir.join("authority.sqlite");
    if auth_path.exists() {
        let _ = std::fs::remove_file(&auth_path);
    }

    let auth_uri = format!("file:{}?mode=rwc&uri=true", auth_path.display());

    Ok(Some(KnowledgeContext {
        root,
        knowdb_path,
        auth_path,
        auth_uri,
    }))
}

fn ensure_models_root_exists(root: &Path) -> Result<Option<PathBuf>, AppError> {
    if !root.exists() {
        warn!("知识库目录不存在，跳过初始化: {}", root.display());
        return Ok(None);
    }

    let canonical = root.canonicalize().map_err(AppError::internal)?;
    Ok(Some(canonical))
}
