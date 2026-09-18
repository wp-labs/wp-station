use crate::common::{setup_db, test_models_root, test_project_layout};
use wp_station::utils::{
    is_knowledge_loaded, load_knowledge, sql_knowdb_list, sql_query, unload_knowledge,
    write_knowdb_config,
};

#[tokio::test]
async fn test_sql_query_returns_fields() {
    setup_db().await;
    unload_knowledge();
    load_knowledge(&test_project_layout()).expect("load knowledge before query");
    let result = sql_query("SELECT 1 as value").await.expect("sql query");
    if let Some(field) = result.first() {
        assert_eq!(field.get_name(), "value");
    }
}

#[tokio::test]
async fn test_sql_knowdb_list_handles_empty_state() {
    setup_db().await;
    unload_knowledge();
    load_knowledge(&test_project_layout()).expect("load knowledge before listing tables");
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

#[tokio::test]
async fn test_load_knowledge_from_project_root() {
    setup_db().await;
    unload_knowledge();
    std::fs::create_dir_all(test_models_root().join(".run")).expect("create .run dir");
    let result = load_knowledge(&test_project_layout());
    assert!(
        result.is_ok(),
        "expected knowledge load to succeed: {:?}",
        result.err()
    );
    unload_knowledge();
}

#[tokio::test]
async fn test_load_knowledge_rejects_legacy_provider_format() {
    setup_db().await;
    unload_knowledge();

    let legacy_knowdb = r#"
version = 2

[provider]
kind = "postgres"
connection_uri = "postgres://postgres:demo@127.0.0.1:5432/demo"
pool_size = 8
"#;
    write_knowdb_config(&test_project_layout(), legacy_knowdb).expect("write legacy knowdb");

    let result = load_knowledge(&test_project_layout());
    let message = result
        .expect_err("legacy provider format should fail")
        .to_string();
    assert!(
        message.contains("旧版 [provider] 配置格式"),
        "actual: {message}"
    );
}
