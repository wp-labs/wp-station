use crate::common::{remove_project_path, setup_db, test_project_root, unique_name};
use wp_station::utils::{
    delete_knowledge_from_project, list_knowledge_dirs, read_knowledge_files, write_knowdb_config,
    write_knowledge_files,
};

fn project_root_str() -> String {
    test_project_root().to_string_lossy().to_string()
}

async fn create_config() -> String {
    setup_db().await;
    let file = format!("knowledge-{}", unique_name("cfg"));
    let root = project_root_str();
    write_knowdb_config(&root, "version = 2").expect("write knowdb");
    write_knowledge_files(
        &root,
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
    let root = project_root_str();

    let found = read_knowledge_files(&root, &file)
        .expect("read config")
        .expect("config exists");
    assert_eq!(found.file_name, file);
    assert_eq!(found.config_content.as_deref(), Some("version = 2"));
    assert_eq!(
        found.create_sql.as_deref(),
        Some("CREATE TABLE t(id INTEGER);")
    );

    let all = list_knowledge_dirs(&root).expect("list knowledge dirs");
    assert!(all.contains(&file));

    remove_project_path(format!("models/knowledge/{file}"));
}

#[tokio::test]
async fn test_update_knowledge_files() {
    let file = create_config().await;
    let root = project_root_str();

    write_knowledge_files(
        &root,
        &file,
        Some("CREATE TABLE updated(id INTEGER);".to_string()),
        Some("INSERT INTO updated VALUES (?1);".to_string()),
        Some("id\n2\n".to_string()),
    )
    .expect("update knowledge files");

    let fetched = read_knowledge_files(&root, &file)
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
    let missing =
        read_knowledge_files(&project_root_str(), "does-not-exist").expect("read missing config");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_delete_knowledge_files_updates_listing() {
    let file = create_config().await;
    let root = project_root_str();

    delete_knowledge_from_project(&root, &file).expect("delete knowledge files");

    let status_list_after = list_knowledge_dirs(&root).expect("status list after delete");
    assert!(status_list_after.iter().all(|name| name != &file));
}
