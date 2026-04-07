use crate::common::{setup_db, unique_name};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{
    NewKnowledgeConfig, create_knowledge_config, find_all_knowledge_configs,
    find_knowledge_config_by_file_name, get_knowledge_config_status_list, update_knowledge_config,
    update_knowledge_config_active,
};
use wp_station_migrations::entity::knowledge_config::{
    Column as KnowledgeColumn, Entity as KnowledgeEntity,
};

async fn cleanup_config(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = KnowledgeEntity::delete_many()
        .filter(KnowledgeColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
}

async fn create_config() -> String {
    setup_db().await;
    let file = format!("knowledge-{}.toml", unique_name("cfg"));
    let new_cfg = NewKnowledgeConfig {
        file_name: file.clone(),
        config_content: Some("[tables]".to_string()),
        create_sql: Some("CREATE TABLE t()".to_string()),
        insert_sql: Some("INSERT".to_string()),
        data_content: Some("row".to_string()),
    };
    create_knowledge_config(new_cfg)
        .await
        .expect("create knowledge config");
    file
}

#[tokio::test]
async fn test_create_and_find_knowledge_config() {
    let file = create_config().await;

    let found = find_knowledge_config_by_file_name(&file)
        .await
        .expect("find config")
        .expect("config exists");
    assert_eq!(found.file_name, file);
    assert_eq!(found.config_content.as_deref(), Some("[tables]"));

    let all = find_all_knowledge_configs().await.expect("list configs");
    assert!(all.iter().any(|cfg| cfg.file_name == file));

    cleanup_config(&file).await;
}

#[tokio::test]
async fn test_update_knowledge_config() {
    let file = create_config().await;

    let updated = NewKnowledgeConfig {
        file_name: file.clone(),
        config_content: Some("[tables]\n[[tables.columns]]".to_string()),
        create_sql: Some("CREATE TABLE updated()".to_string()),
        insert_sql: Some("INSERT updated".to_string()),
        data_content: Some("row2".to_string()),
    };

    update_knowledge_config(&file, updated)
        .await
        .expect("update config");

    let fetched = find_knowledge_config_by_file_name(&file)
        .await
        .expect("find updated")
        .expect("exists");
    assert_eq!(
        fetched.config_content.as_deref(),
        Some("[tables]\n[[tables.columns]]")
    );
    assert_eq!(
        fetched.create_sql.as_deref(),
        Some("CREATE TABLE updated()")
    );

    cleanup_config(&file).await;
}

#[tokio::test]
async fn test_find_knowledge_config_not_found() {
    setup_db().await;
    let missing = find_knowledge_config_by_file_name("does-not-exist")
        .await
        .expect("find missing");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_update_active_flag_and_status_list() {
    let file = create_config().await;

    let status_list = get_knowledge_config_status_list()
        .await
        .expect("status list before update");
    assert!(
        status_list
            .iter()
            .any(|(name, active)| name == &file && *active)
    );

    update_knowledge_config_active(&file, false)
        .await
        .expect("deactivate");

    let status_list_after = get_knowledge_config_status_list()
        .await
        .expect("status list after update");
    assert!(status_list_after.iter().all(|(name, _)| name != &file));

    cleanup_config(&file).await;
}
