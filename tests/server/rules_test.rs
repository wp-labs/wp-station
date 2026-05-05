use crate::common::{rand_suffix, remove_project_path, setup_db, test_project_layout};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{NewRelease, ReleaseStatus, RuleType, create_release};
use wp_station::server::rules::{
    RuleFilesQuery, create_rule_file_logic, delete_rule_file_logic, get_rule_content_logic,
    get_rule_files_logic, save_rule_logic,
};
use wp_station::utils::pagination::PageQuery;
use wp_station::utils::{
    read_knowledge_files, read_rule_content, write_knowdb_config, write_knowledge_files,
    write_rule_content, write_wpl_sample_content,
};
use wp_station_migrations::entity::release::{Column as ReleaseColumn, Entity as ReleaseEntity};

fn cleanup_knowledge(file: &str) {
    remove_project_path(format!("models/knowledge/{file}"));
}

fn cleanup_rule(rule_type: RuleType, file: &str) {
    match rule_type {
        RuleType::Wpl => remove_project_path(format!("models/wpl/{file}")),
        RuleType::Oml => remove_project_path(format!("models/oml/{file}")),
        RuleType::Sink => remove_project_path(format!("topology/sinks/{file}")),
        RuleType::Source => remove_project_path(format!("topology/sources/{file}")),
        RuleType::Parse => remove_project_path("conf/wparse.toml"),
        RuleType::Wpgen => remove_project_path("conf/wpgen.toml"),
        RuleType::SourceConnect => remove_project_path(format!("connectors/source.d/{file}")),
        RuleType::SinkConnect => remove_project_path(format!("connectors/sink.d/{file}")),
        RuleType::Knowledge | RuleType::All => {}
    }
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

    cleanup_knowledge(&file);
}

#[tokio::test]
async fn test_create_and_delete_knowledge_rule_via_logic() {
    setup_db().await;
    let file = format!("logic-{}.toml", rand_suffix());

    create_rule_file_logic(RuleType::Knowledge, file.clone())
        .await
        .expect("create knowledge rule");

    assert!(
        read_knowledge_files(&test_project_layout(), &file)
            .expect("read created knowledge")
            .is_some()
    );

    delete_rule_file_logic(RuleType::Knowledge, file.clone(), None)
        .await
        .expect("delete knowledge rule");

    assert!(
        read_knowledge_files(&test_project_layout(), &file)
            .expect("read deleted knowledge")
            .is_none()
    );
}

#[tokio::test]
async fn test_save_rule_logic_creates_and_updates_rule() {
    setup_db().await;
    let file = format!("wpl-{}", rand_suffix());
    let draft_version = format!("draft-{}", rand_suffix());

    // 确保 handle_draft_release 能找到草稿记录
    create_release(NewRelease {
        version: draft_version.clone(),
        release_group: "draft".to_string(),
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

    let (content, _) = read_rule_content(&test_project_layout(), RuleType::Wpl, &file)
        .expect("query rule")
        .expect("rule exists");
    assert!(content.contains("chars:name"));

    cleanup_rule(RuleType::Wpl, &file);
    cleanup_release(&draft_version).await;
}

#[tokio::test]
async fn test_get_rule_content_logic_returns_list() {
    setup_db().await;
    let file = format!("bulk-{}", rand_suffix());
    write_rule_content(&test_project_layout(), RuleType::Oml, &file, "content")
        .expect("create sample oml");

    let result = get_rule_content_logic(RuleType::Oml, None)
        .await
        .expect("list rule content");
    assert!(result.is_array());
    cleanup_rule(RuleType::Oml, &file);
}

#[tokio::test]
async fn test_get_rule_files_logic_filters_keyword() {
    setup_db().await;
    let target = "wparse.toml".to_string();
    write_rule_content(&test_project_layout(), RuleType::Parse, &target, "content")
        .expect("write parse rule");

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
}

#[tokio::test]
async fn test_delete_rule_file_logic_for_standard_rule() {
    setup_db().await;
    let file = format!("delete-{}", rand_suffix());
    write_rule_content(&test_project_layout(), RuleType::Sink, &file, "content")
        .expect("insert sink rule");

    delete_rule_file_logic(RuleType::Sink, file.clone(), None)
        .await
        .expect("delete sink rule");
    let record =
        read_rule_content(&test_project_layout(), RuleType::Sink, &file).expect("query rule");
    assert!(record.is_none());
}

#[tokio::test]
async fn test_wpl_virtual_sample_round_trip() {
    setup_db().await;
    let file = format!("sample-{}", rand_suffix());
    write_rule_content(
        &test_project_layout(),
        RuleType::Wpl,
        &file,
        "package demo {}",
    )
    .expect("write wpl parse");
    write_wpl_sample_content(&test_project_layout(), &file, "sample-data").expect("write sample");

    let content = get_rule_content_logic(RuleType::Wpl, Some(format!("{file}/sample.dat")))
        .await
        .expect("get sample content");
    assert_eq!(content["content"], "sample-data");

    cleanup_rule(RuleType::Wpl, &file);
}

#[tokio::test]
async fn test_get_rule_content_logic_missing_file_errors() {
    setup_db().await;
    let missing = format!("missing-{}", rand_suffix());
    let result = get_rule_content_logic(RuleType::Sink, Some(missing));
    assert!(result.await.is_err());
}
