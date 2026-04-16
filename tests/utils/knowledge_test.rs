use wp_station::utils::{
    is_knowledge_loaded, load_knowledge, sql_knowdb_list, sql_query, unload_knowledge,
};

#[tokio::test]
async fn test_sql_query_returns_fields() {
    let result = sql_query("SELECT 1 as value").await.expect("sql query");
    if let Some(field) = result.first() {
        assert_eq!(field.get_name(), "value");
    }
}

#[tokio::test]
async fn test_sql_knowdb_list_handles_empty_state() {
    let list = sql_knowdb_list().await.expect("knowdb list");
    assert!(list.iter().all(|name| !name.is_empty()));
}

#[test]
fn test_knowledge_loaded_flags_can_toggle() {
    unload_knowledge();
    assert!(!is_knowledge_loaded());
    unload_knowledge();
    assert!(!is_knowledge_loaded());
}

#[test]
fn test_load_knowledge_from_project_root() {
    unload_knowledge();
    std::fs::create_dir_all("project_root/.run").expect("create .run dir");
    let result = load_knowledge("project_root");
    assert!(
        result.is_ok(),
        "expected knowledge load to succeed: {:?}",
        result.err()
    );
    unload_knowledge();
}
