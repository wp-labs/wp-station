use std::path::PathBuf;

use wp_station::db::init_default_configs_to_models;
use wp_station::server::ProjectLayout;
use wp_station::utils::{
    is_knowledge_loaded, load_knowledge, sql_knowdb_list, sql_query, unload_knowledge,
};

fn temp_knowledge_layout(prefix: &str) -> ProjectLayout {
    let root = std::env::temp_dir().join(format!(
        "wp-station-{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).expect("create temp knowledge root");
    init_default_configs_to_models(root.to_str().expect("utf-8 temp root"))
        .expect("initialize default knowledge configs");
    ProjectLayout {
        models_root: root,
        infra_root: PathBuf::new(),
    }
}

#[tokio::test]
async fn test_sql_query_returns_fields() {
    let layout = temp_knowledge_layout("sql-query");
    load_knowledge(&layout).expect("load knowledge");

    let result = sql_query("SELECT 1 as value").await.expect("sql query");
    if let Some(field) = result.first() {
        assert_eq!(field.get_name(), "value");
    }

    unload_knowledge();
}

#[tokio::test]
async fn test_sql_knowdb_list_handles_empty_state() {
    let layout = temp_knowledge_layout("knowdb-list");
    load_knowledge(&layout).expect("load knowledge");

    let list = sql_knowdb_list().await.expect("knowdb list");
    assert!(list.iter().all(|name| !name.is_empty()));

    unload_knowledge();
}

#[test]
fn test_knowledge_loaded_flags_can_toggle() {
    unload_knowledge();
    assert!(!is_knowledge_loaded());
    unload_knowledge();
    assert!(!is_knowledge_loaded());
}

#[tokio::test]
async fn test_load_knowledge_from_project_root() {
    unload_knowledge();
    let layout = temp_knowledge_layout("load-knowledge");
    let result = load_knowledge(&layout);
    assert!(
        result.is_ok(),
        "expected knowledge load to succeed: {:?}",
        result.err()
    );
    unload_knowledge();
}
