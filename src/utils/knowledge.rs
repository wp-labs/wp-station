use crate::error::AppError;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use tracing::{debug, info};
use wp_data_utils::cache::FieldQueryCache;
use wp_knowledge::facade;
use wp_model_core::model::{DataField, DataRecord};
use wp_oml::{DataRecordRef, core::FieldExtractor, language::SqlQuery, types::AnyResult};

lazy_static! {
    /// 全局知识库是否已加载的标志（共享配置，无连接隔离）
    static ref KNOWLEDGE_LOADED: RwLock<bool> = RwLock::new(false);
}

pub fn db_init() -> AnyResult<Vec<DataField>> {
    //todo 写一个加载 project_root/models/knowledge下所有数据的方法
    Ok(vec![])
}

pub fn sql_query(sql: &str) -> AnyResult<Vec<DataField>> {
    let cache = &mut FieldQueryCache::default();
    let query = SqlQuery::new(sql.to_string(), HashMap::default());
    let result = query.extract_more(
        &mut DataRecordRef::from(&DataRecord::default()),
        &DataRecord::default(),
        cache,
    );
    debug!("知识库工具执行 SQL 查询完成");
    Ok(result)
}

pub fn sql_knowdb_list() -> AnyResult<Vec<String>> {
    let sql = r#"SELECT GROUP_CONCAT(name, ', ') as name FROM sqlite_master WHERE type='table'"#;
    let cache = &mut FieldQueryCache::default();
    let query = SqlQuery::new(sql.to_string(), HashMap::default());
    let result = query.extract_more(
        &mut DataRecordRef::from(&DataRecord::default()),
        &DataRecord::default(),
        cache,
    );
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
    *KNOWLEDGE_LOADED.read().unwrap()
}

/// 加载知识库（全局共享，只初始化一次）
pub fn load_knowledge(project_dir: &str) -> AnyResult<()> {
    // 检查是否已加载
    if is_knowledge_loaded() {
        info!("知识库已加载，跳过初始化");
        return Ok(());
    }

    let root = PathBuf::from(&project_dir).canonicalize().map_err(|e| {
        error!("无法解析项目目录路径: {}", e);
        AppError::internal(e)
    })?;

    let knowdb_path = root.join("models/knowledge/knowdb.toml");
    let auth_path = root.join(".run/authority.sqlite");

    // 清理旧的 authority 文件
    if auth_path.exists() {
        let _ = std::fs::remove_file(&auth_path);
    }

    let auth_uri = format!("file:{}?mode=rwc&uri=true", auth_path.display());
    info!(
        "初始化知识库: root={}, knowdb={}",
        root.display(),
        knowdb_path.display()
    );

    let dict = Default::default();
    match facade::init_thread_cloned_from_knowdb(&root, &knowdb_path, &auth_uri, &dict) {
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

    // 标记为已加载
    *KNOWLEDGE_LOADED.write().unwrap() = true;

    Ok(())
}

/// 卸载知识库（释放资源）
pub fn unload_knowledge() {
    let mut loaded = KNOWLEDGE_LOADED.write().unwrap();
    if *loaded {
        *loaded = false;
        info!("知识库已卸载");
    }
}

/// 重新加载知识库
pub fn reload_knowledge(project_dir: &str) -> AnyResult<()> {
    unload_knowledge();
    load_knowledge(project_dir)
}
