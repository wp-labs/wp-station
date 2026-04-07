use crate::common::{rand_suffix, setup_db};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{
    NewKnowledgeConfig, NewRelease, NewRuleConfig, ReleaseStatus, RuleType,
    create_knowledge_config, create_release, create_rule_config, find_rule_by_type_and_name,
};
use wp_station::server::rules::{
    RuleFilesQuery, create_rule_file_logic, delete_rule_file_logic, get_rule_content_logic,
    get_rule_files_logic, save_rule_logic,
};
use wp_station::utils::pagination::PageQuery;
use wp_station_migrations::entity::knowledge_config::{
    Column as KnowledgeColumn, Entity as KnowledgeEntity,
};
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};
use wp_station_migrations::entity::rule_config::{Column as RuleColumn, Entity as RuleEntity};

async fn cleanup_knowledge(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = KnowledgeEntity::delete_many()
        .filter(KnowledgeColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
}

async fn cleanup_rule(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = RuleEntity::delete_many()
        .filter(RuleColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
}

async fn cleanup_release(version: &str) {
    let pool = wp_station::db::get_pool();
    let _ = ReleaseEntity::delete_many()
        .filter(ReleaseColumn::Version.eq(version))
        .exec(pool.inner())
        .await;
}

#[tokio::test]
async fn test_get_rule_files_and_content_for_knowledge() {
    setup_db().await;
    let file = format!("knowledge-{}", rand_suffix());
    create_knowledge_config(NewKnowledgeConfig {
        file_name: file.clone(),
        config_content: Some("version = 1".to_string()),
        create_sql: Some("CREATE TABLE t(id INTEGER);".to_string()),
        insert_sql: Some("INSERT INTO t VALUES (1);".to_string()),
        data_content: Some("row".to_string()),
    })
    .await
    .expect("create knowledge config");

    let files = get_rule_files_logic(RuleFilesQuery {
        rule_type: RuleType::Knowledge,
        keyword: None,
        page: PageQuery {
            page: Some(1),
            page_size: Some(50),
        },
    })
    .await
    .expect("list knowledge files");
    assert!(files.items.iter().any(|item| item.file == file));

    let content = get_rule_content_logic(RuleType::Knowledge, Some(file.clone()))
        .await
        .expect("get knowledge content");
    let cfg: serde_json::Value = content;
    assert_eq!(
        cfg.get("file").and_then(|v| v.as_str()),
        Some(file.as_str())
    );

    cleanup_knowledge(&file).await;
}

#[tokio::test]
async fn test_create_and_delete_knowledge_rule_via_logic() {
    setup_db().await;
    let file = format!("logic-{}.toml", rand_suffix());

    create_rule_file_logic(RuleType::Knowledge, file.clone())
        .await
        .expect("create knowledge rule");

    delete_rule_file_logic(RuleType::Knowledge, file.clone(), None)
        .await
        .expect("delete knowledge rule");

    let record = KnowledgeEntity::find()
        .filter(KnowledgeColumn::FileName.eq(file.clone()))
        .one(wp_station::db::get_pool().inner())
        .await
        .expect("query knowledge entry");
    assert!(record.is_some());
    assert!(!record.unwrap().is_active);

    cleanup_knowledge(&file).await;
}

#[tokio::test]
async fn test_save_rule_logic_creates_and_updates_rule() {
    setup_db().await;
    let file = format!("wpl-{}", rand_suffix());
    let draft_version = format!("draft-{}", rand_suffix());

    // 确保 handle_draft_release 能找到草稿记录
    create_release(NewRelease {
        version: draft_version.clone(),
        pipeline: Some("draft".to_string()),
        created_by: Some("tester".to_string()),
        stages: None,
        status: Some(ReleaseStatus::WAIT),
    })
    .await
    .expect("create draft release");

    save_rule_logic(
        RuleType::Wpl,
        file.clone(),
        Some("package demo { rule a { digit:id } }".to_string()),
        None,
    )
    .await
    .expect("save new rule");

    save_rule_logic(
        RuleType::Wpl,
        file.clone(),
        Some("package demo { rule a { chars:name } }".to_string()),
        None,
    )
    .await
    .expect("update existing rule");

    let rule = find_rule_by_type_and_name(RuleType::Wpl.as_ref(), &file)
        .await
        .expect("query rule")
        .expect("rule exists");
    assert!(rule.content.unwrap().contains("chars:name"));

    cleanup_rule(&file).await;
    cleanup_release(&draft_version).await;
}

#[tokio::test]
async fn test_get_rule_content_logic_returns_list() {
    setup_db().await;
    let file = format!("bulk-{}", rand_suffix());
    create_rule_config(NewRuleConfig {
        rule_type: RuleType::Oml,
        file_name: file.clone(),
        display_name: None,
        content: Some("content".to_string()),
        sample_content: None,
        file_size: Some(7),
    })
    .await
    .expect("create sample rule");

    let result = get_rule_content_logic(RuleType::Oml, None)
        .await
        .expect("list rule content");
    assert!(result.is_array());
    cleanup_rule(&file).await;
}

#[tokio::test]
async fn test_get_rule_files_logic_filters_keyword() {
    setup_db().await;
    let target = format!("filter-{}", rand_suffix());
    create_rule_config(NewRuleConfig {
        rule_type: RuleType::Parse,
        file_name: target.clone(),
        display_name: None,
        content: Some("content".to_string()),
        sample_content: None,
        file_size: Some(10),
    })
    .await
    .expect("insert parse rule");

    let files = get_rule_files_logic(RuleFilesQuery {
        rule_type: RuleType::Parse,
        keyword: Some(target.clone()),
        page: PageQuery {
            page: Some(1),
            page_size: Some(50),
        },
    })
    .await
    .expect("filter files");
    assert!(files.items.iter().any(|item| item.file == target));
    cleanup_rule(&target).await;
}

#[tokio::test]
async fn test_delete_rule_file_logic_for_standard_rule() {
    setup_db().await;
    let file = format!("delete-{}", rand_suffix());
    create_rule_config(NewRuleConfig {
        rule_type: RuleType::Sink,
        file_name: file.clone(),
        display_name: None,
        content: Some("content".to_string()),
        sample_content: None,
        file_size: Some(10),
    })
    .await
    .expect("insert sink rule");

    delete_rule_file_logic(RuleType::Sink, file.clone(), None)
        .await
        .expect("delete sink rule");
    let record = RuleEntity::find()
        .filter(RuleColumn::FileName.eq(file.clone()))
        .one(wp_station::db::get_pool().inner())
        .await
        .expect("query rule");
    assert!(record.is_some());
    cleanup_rule(&file).await;
}

#[tokio::test]
async fn test_get_rule_content_logic_missing_file_errors() {
    setup_db().await;
    let missing = format!("missing-{}", rand_suffix());
    let result = get_rule_content_logic(RuleType::Sink, Some(missing));
    assert!(result.await.is_err());
}
