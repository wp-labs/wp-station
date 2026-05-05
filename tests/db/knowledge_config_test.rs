use crate::common::{remove_project_path, setup_db, test_project_layout, unique_name};
use wp_station::utils::{
    delete_knowledge_from_project, list_knowledge_dirs, read_knowledge_files, write_knowdb_config,
    write_knowledge_files,
};

async fn create_config() -> String {
    setup_db().await;
    let file = format!("knowledge-{}", unique_name("cfg"));
    let layout = test_project_layout();
    write_knowdb_config(&layout, "version = 2").expect("write knowdb");
    write_knowledge_files(
        &layout,
        &file,
        Some("CREATE TABLE t(id INTEGER);".to_string()),
        Some("INSERT INTO t VALUES (?1);".to_string()),
        Some("id\n1\n".to_string()),
    )
    .expect("write knowledge files");
    file
}

#[tokio::test]
async fn test_create_and_find_knowledge_files() {
    let file = create_config().await;
    let layout = test_project_layout();

    let found = read_knowledge_files(&layout, &file)
        .expect("read config")
        .expect("config exists");
    assert_eq!(found.file_name, file);
    assert_eq!(found.config_content.as_deref(), Some("version = 2"));
    assert_eq!(
        found.create_sql.as_deref(),
        Some("CREATE TABLE t(id INTEGER);")
    );

    let all = list_knowledge_dirs(&layout).expect("list knowledge dirs");
    assert!(all.contains(&file));

    remove_project_path(format!("models/knowledge/{file}"));
}

#[tokio::test]
async fn test_update_knowledge_files() {
    let file = create_config().await;
    let layout = test_project_layout();

    write_knowledge_files(
        &layout,
        &file,
        Some("CREATE TABLE updated(id INTEGER);".to_string()),
        Some("INSERT INTO updated VALUES (?1);".to_string()),
        Some("id\n2\n".to_string()),
    )
    .expect("update knowledge files");

    let fetched = read_knowledge_files(&layout, &file)
        .expect("read updated")
        .expect("exists");
    assert_eq!(
        fetched.create_sql.as_deref(),
        Some("CREATE TABLE updated(id INTEGER);")
    );

    remove_project_path(format!("models/knowledge/{file}"));
}

#[tokio::test]
async fn test_find_knowledge_files_not_found() {
    setup_db().await;
    let layout = test_project_layout();
    let missing = read_knowledge_files(&layout, "does-not-exist").expect("read missing config");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_delete_knowledge_files_updates_listing() {
    let file = create_config().await;
    let layout = test_project_layout();

    delete_knowledge_from_project(&layout, &file).expect("delete knowledge files");

    let status_list_after = list_knowledge_dirs(&layout).expect("status list after delete");
    assert!(status_list_after.iter().all(|name| name != &file));
}
