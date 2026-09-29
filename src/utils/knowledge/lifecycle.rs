//! 知识库加载、卸载与来源切换逻辑。

use super::{
    KNOWLEDGE_LOADED, KnowledgeContext, KnowledgeLoadedSource, LEGACY_PROVIDER_FORMAT_MESSAGE,
};
use crate::constants::project::{DIR_KNOWLEDGE, DIR_MODELS, FILE_KNOWDB};
use crate::error::AppError;
use crate::server::RepoLayout;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{error, info, warn};
use wp_knowledge::facade;
use wp_knowledge::loader;

/// 检查 Station 当前进程是否已经加载了任意知识库来源。
pub fn is_knowledge_loaded() -> bool {
    KNOWLEDGE_LOADED.read().unwrap().is_some()
}

/// 读取 `knowdb.toml` 中声明的 provider 生效名称。
///
/// SQL provider 未配置 `name` 时使用 `wp-knowledge` 的默认名称 `default`。
/// 多数据库模式下调用方必须基于此列表显式选择查询目标，避免误落到默认连接。
pub fn configured_provider_names(layout: &RepoLayout) -> Result<Vec<String>, AppError> {
    let Some(context) = build_knowledge_context(layout)? else {
        return Ok(Vec::new());
    };

    ensure_supported_provider_format(&context)?;

    let dict = Default::default();
    let (conf, _, _) = loader::parse_knowdb_conf(&context.root, &context.knowdb_path, &dict)
        .map_err(|e| {
            error!("解析 knowdb 配置失败: {}", e);
            AppError::internal(e)
        })?;

    Ok(conf.provider().map_or_else(Vec::new, |provider| {
        if let Some(sqldb) = provider.sqldb {
            return sqldb
                .iter()
                .map(|spec| spec.effective_name().to_string())
                .collect();
        }

        if provider.redis.is_some() {
            return vec!["redis".to_string()];
        }

        Vec::new()
    }))
}

/// 按 `knowdb.toml` 当前配置加载知识库 provider。
pub fn load_knowledge(layout: &RepoLayout) -> anyhow::Result<()> {
    if is_loaded_source(KnowledgeLoadedSource::Configured) {
        info!("知识库已加载，跳过初始化");
        return Ok(());
    }

    let Some(context) = build_knowledge_context(layout)? else {
        return Ok(());
    };

    ensure_supported_provider_format(&context).map_err(|err| anyhow::anyhow!(err.to_string()))?;

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

/// 强制加载本地 authority sqlite，供调试查询场景使用。
pub fn load_sqlite_knowledge(layout: &RepoLayout) -> anyhow::Result<()> {
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

    if context.auth_path.exists() {
        fs::remove_file(&context.auth_path).map_err(|e| {
            error!("删除旧的本地知识库 authority 失败: {}", e);
            AppError::internal(e)
        })?;
    }

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
    match facade::init_thread_cloned_from_authority(&ro_uri) {
        Ok(_) => {}
        Err(e) => {
            let error_msg = format!("{:?}", e);
            if error_msg.contains("already initialized") {
                info!("本地知识库 authority 已初始化（全局单例），继续使用");
            } else {
                error!("初始化本地知识库 authority 失败: {}", e);
                return Err(AppError::internal(e).into());
            }
        }
    }

    wp_knowledge::runtime::runtime().configure_result_cache(
        conf.cache.enabled,
        conf.cache.capacity,
        Duration::from_millis(conf.cache.ttl_ms.max(1)),
    );

    info!("本地知识库 authority 初始化成功");
    set_loaded_source(KnowledgeLoadedSource::SqliteAuthority);
    Ok(())
}

/// 仅重置 Station 侧已加载来源标记，不主动销毁全局 runtime。
pub fn unload_knowledge() {
    let mut loaded = KNOWLEDGE_LOADED.write().unwrap();
    if loaded.is_some() {
        *loaded = None;
        info!("知识库已卸载");
    }
}

/// 重新加载配置 provider。
pub fn reload_knowledge(layout: &RepoLayout) -> anyhow::Result<()> {
    unload_knowledge();
    load_knowledge(layout)
}

/// 重新加载本地 authority sqlite。
pub fn reload_sqlite_knowledge(layout: &RepoLayout) -> anyhow::Result<()> {
    unload_knowledge();
    load_sqlite_knowledge(layout)
}

/// 判断当前查询源切换时是否需要重新加载。
pub fn should_reload_knowledge_source(source: &str) -> bool {
    match (source, current_loaded_source()) {
        ("configured", Some(KnowledgeLoadedSource::Configured)) => false,
        ("sqlite", Some(KnowledgeLoadedSource::SqliteAuthority)) => false,
        (_, Some(_)) => true,
        _ => false,
    }
}

/// 组装知识库加载所需的目录、配置和 authority 路径。
fn build_knowledge_context(layout: &RepoLayout) -> Result<Option<KnowledgeContext>, AppError> {
    let Some(root) = ensure_models_root_exists(&layout.models_root)? else {
        return Ok(None);
    };

    let knowledge_root = root.join(DIR_MODELS).join(DIR_KNOWLEDGE);
    if !knowledge_root.exists() {
        warn!(
            "项目中未找到知识库目录，跳过初始化: {}",
            knowledge_root.display()
        );
        return Ok(None);
    }

    let knowdb_path = knowledge_root.join(FILE_KNOWDB);
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
    let auth_uri = format!("file:{}?mode=rwc&uri=true", auth_path.display());

    Ok(Some(KnowledgeContext {
        root,
        knowdb_path,
        auth_path,
        auth_uri,
    }))
}

/// 确认 models 根目录存在，并转换为规范化绝对路径。
fn ensure_models_root_exists(root: &Path) -> Result<Option<PathBuf>, AppError> {
    if !root.exists() {
        warn!("知识库目录不存在，跳过初始化: {}", root.display());
        return Ok(None);
    }

    let canonical = root.canonicalize().map_err(AppError::internal)?;
    Ok(Some(canonical))
}

/// 拒绝旧版 provider 配置格式，避免 runtime 退回本地 authority 却让调用方误判。
fn ensure_supported_provider_format(context: &KnowledgeContext) -> Result<(), AppError> {
    let knowdb_content = fs::read_to_string(&context.knowdb_path).map_err(AppError::internal)?;
    if has_legacy_provider_format(&knowdb_content) {
        error!(
            "检测到旧版知识库 provider 配置格式: path={}",
            context.knowdb_path.display()
        );
        return Err(AppError::validation(LEGACY_PROVIDER_FORMAT_MESSAGE));
    }
    Ok(())
}

/// 判断 `knowdb.toml` 是否仍使用旧版平铺的 `[provider]` 配置。
fn has_legacy_provider_format(knowdb_content: &str) -> bool {
    let normalized = knowdb_content.replace("\r\n", "\n");
    let has_nested_provider =
        normalized.contains("[provider.sqldb]") || normalized.contains("[provider.redis]");
    if has_nested_provider || !normalized.contains("[provider]") {
        return false;
    }

    normalized.contains("\nkind =")
        || normalized.contains("\nconnection_uri =")
        || normalized.contains("\npool_size =")
        || normalized.contains("\nmin_connections =")
        || normalized.contains("\nacquire_timeout_ms =")
        || normalized.contains("\nidle_timeout_ms =")
        || normalized.contains("\nmax_lifetime_ms =")
}

/// 检查指定来源是否已经处于已加载状态。
fn is_loaded_source(source: KnowledgeLoadedSource) -> bool {
    *KNOWLEDGE_LOADED.read().unwrap() == Some(source)
}

/// 读取当前已加载来源，供切换查询源时判断是否需要 reload。
fn current_loaded_source() -> Option<KnowledgeLoadedSource> {
    *KNOWLEDGE_LOADED.read().unwrap()
}

/// 更新当前进程记录的知识库来源。
fn set_loaded_source(source: KnowledgeLoadedSource) {
    *KNOWLEDGE_LOADED.write().unwrap() = Some(source);
}
