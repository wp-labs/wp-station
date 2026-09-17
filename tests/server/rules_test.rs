use crate::common::{rand_suffix, remove_project_path, setup_db, test_project_layout};
use wp_station::db::RuleType;
use wp_station::server::rules::{
    RuleFilesQuery, create_rule_file_logic, delete_rule_file_logic, get_knowdb_config_logic,
    get_rule_content_logic, get_rule_files_logic, save_rule_logic, validate_rule_logic,
};
use wp_station::utils::SystemKind;
use wp_station::utils::pagination::PageQuery;
use wp_station::utils::{
    read_knowledge_files, read_rule_content, unload_knowledge, write_knowdb_config,
    write_knowledge_files, write_rule_content, write_wpl_sample_content,
};
fn cleanup_knowledge(file: &str) {
    remove_project_path(format!("models/knowledge/{file}"));
}

fn cleanup_rule(rule_type: RuleType, file: &str) {
    match rule_type {
        RuleType::Wpl => remove_project_path(format!("models/wpl/{file}")),
        RuleType::Oml => remove_project_path(format!("models/oml/{file}")),
        RuleType::Windows => remove_project_path("models/windows.toml"),
        RuleType::Schema => remove_project_path(format!("models/schemas/{file}")),
        RuleType::Rule => remove_project_path(format!("models/rules/{file}")),
        RuleType::Scenarios => remove_project_path(format!("models/scenarios/{file}")),
        RuleType::Sink => remove_project_path(format!("topology/sinks/{file}")),
        RuleType::Source => remove_project_path(format!("topology/sources/{file}")),
        RuleType::Parse => remove_project_path("conf/wparse.toml"),
        RuleType::Wpgen => remove_project_path("conf/wpgen.toml"),
        RuleType::SourceConnect => remove_project_path(format!("connectors/source.d/{file}")),
        RuleType::SinkConnect => remove_project_path(format!("connectors/sink.d/{file}")),
        RuleType::Knowledge | RuleType::All => {}
    }
}

#[tokio::test]
async fn test_wfusion_scenarios_rule_round_trip_via_logic() {
    setup_db().await;
    let name = format!("scenario-{}", rand_suffix());
    let file = format!("{name}/{name}.wfg");

    create_rule_file_logic(SystemKind::Wfusion, RuleType::Scenarios, file.clone())
        .await
        .expect("create scenario rule");

    save_rule_logic(
        SystemKind::Wfusion,
        RuleType::Scenarios,
        file.clone(),
        Some("scenario sandbox {}".to_string()),
    )
    .await
    .expect("save scenario rule");

    let files = get_rule_files_logic(RuleFilesQuery {
        system: SystemKind::Wfusion,
        rule_type: RuleType::Scenarios,
        keyword: None,
        page: PageQuery {
            page: Some(1),
            page_size: Some(50),
        },
    })
    .await
    .expect("list scenario rules");
    assert!(files.items.iter().any(|item| item.file == file));

    let content =
        get_rule_content_logic(SystemKind::Wfusion, RuleType::Scenarios, Some(file.clone()))
            .await
            .expect("get scenario content");
    assert_eq!(content["content"], "scenario sandbox {}");

    cleanup_rule(RuleType::Scenarios, &file);
}

#[tokio::test]
async fn test_wfusion_scenarios_supports_legacy_nested_virtual_name() {
    setup_db().await;
    let name = format!("legacy-{}", rand_suffix());
    let file = format!("{name}/{name}.wfg");
    let legacy_file = format!("{name}.wfg");

    create_rule_file_logic(SystemKind::Wfusion, RuleType::Scenarios, file.clone())
        .await
        .expect("create scenario rule");

    save_rule_logic(
        SystemKind::Wfusion,
        RuleType::Scenarios,
        file.clone(),
        Some("scenario legacy {}".to_string()),
    )
    .await
    .expect("save scenario rule");

    let content =
        get_rule_content_logic(SystemKind::Wfusion, RuleType::Scenarios, Some(legacy_file))
            .await
            .expect("get legacy scenario content");
    assert_eq!(content["content"], "scenario legacy {}");

    cleanup_rule(RuleType::Scenarios, &file);
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
        system: SystemKind::Wparse,
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

    let content =
        get_rule_content_logic(SystemKind::Wparse, RuleType::Knowledge, Some(file.clone()))
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
async fn test_get_knowdb_config_logic_returns_config_content() {
    setup_db().await;
    unload_knowledge();

    let file = format!("autoload-{}", rand_suffix());
    let layout = test_project_layout();
    let updated_knowdb = format!(
        r#"version = 2

[[tables]]
enabled = true
name = "{file}"
[tables.columns]
by_index = [0]
[tables.csv]
has_header = false
[tables.expected_rows]
min = 0
max = 10
"#
    );
    write_knowdb_config(&layout, &updated_knowdb).expect("write knowdb");
    write_knowledge_files(
        &layout,
        &file,
        Some("CREATE TABLE IF NOT EXISTS {table} (id INTEGER);".to_string()),
        Some("INSERT INTO {table} (id) VALUES (?1);".to_string()),
        Some("1\n".to_string()),
    )
    .expect("write knowledge files");

    let response = get_knowdb_config_logic(SystemKind::Wparse)
        .await
        .expect("get knowdb config");
    assert_eq!(response.file, "knowdb.toml");
    assert!(
        response
            .content
            .as_deref()
            .is_some_and(|content| content.contains(&format!("name = \"{file}\""))),
        "expected knowdb content to be returned after runtime autoload attempt"
    );

    unload_knowledge();
    cleanup_knowledge(&file);
}

#[tokio::test]
async fn test_create_and_delete_knowledge_rule_via_logic() {
    setup_db().await;
    let file = format!("logic-{}.toml", rand_suffix());

    create_rule_file_logic(SystemKind::Wparse, RuleType::Knowledge, file.clone())
        .await
        .expect("create knowledge rule");

    assert!(
        read_knowledge_files(&test_project_layout(), &file)
            .expect("read created knowledge")
            .is_some()
    );

    delete_rule_file_logic(SystemKind::Wparse, RuleType::Knowledge, file.clone())
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

    save_rule_logic(
        SystemKind::Wparse,
        RuleType::Wpl,
        file.clone(),
        Some("package demo { rule a { digit:id } }".to_string()),
    )
    .await
    .expect("save new rule");

    save_rule_logic(
        SystemKind::Wparse,
        RuleType::Wpl,
        file.clone(),
        Some("package demo { rule a { chars:name } }".to_string()),
    )
    .await
    .expect("update existing rule");

    let (content, _) = read_rule_content(&test_project_layout(), RuleType::Wpl, &file)
        .expect("query rule")
        .expect("rule exists");
    assert!(content.contains("chars:name"));

    cleanup_rule(RuleType::Wpl, &file);
}

#[tokio::test]
async fn test_get_rule_content_logic_returns_list() {
    setup_db().await;
    let file = format!("bulk-{}", rand_suffix());
    write_rule_content(&test_project_layout(), RuleType::Oml, &file, "content")
        .expect("create sample oml");

    let result = get_rule_content_logic(SystemKind::Wparse, RuleType::Oml, None)
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
        system: SystemKind::Wparse,
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

    delete_rule_file_logic(SystemKind::Wparse, RuleType::Sink, file.clone())
        .await
        .expect("delete sink rule");
    let record =
        read_rule_content(&test_project_layout(), RuleType::Sink, &file).expect("query rule");
    assert!(record.is_none());
}

#[tokio::test]
async fn test_wfusion_global_rule_cannot_be_deleted() {
    setup_db().await;
    let error = delete_rule_file_logic(
        SystemKind::Wfusion,
        RuleType::Rule,
        "_global.wfl".to_string(),
    )
    .await
    .expect_err("global rule must be protected");

    assert!(error.to_string().contains("全局规则文件不允许删除"));
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

    let content = get_rule_content_logic(
        SystemKind::Wparse,
        RuleType::Wpl,
        Some(format!("{file}/sample.dat")),
    )
    .await
    .expect("get sample content");
    assert_eq!(content["content"], "sample-data");

    cleanup_rule(RuleType::Wpl, &file);
}

#[tokio::test]
async fn test_get_rule_content_logic_missing_file_errors() {
    setup_db().await;
    let missing = format!("missing-{}", rand_suffix());
    let result = get_rule_content_logic(SystemKind::Wparse, RuleType::Sink, Some(missing));
    assert!(result.await.is_err());
}

#[tokio::test]
async fn test_validate_rule_logic_uses_unsaved_wpl_content_in_temp_dir() {
    setup_db().await;
    let file = format!("validate-{}", rand_suffix());

    create_rule_file_logic(SystemKind::Wparse, RuleType::Wpl, file.clone())
        .await
        .expect("create empty wpl rule");

    let response = validate_rule_logic(
        SystemKind::Wparse,
        RuleType::Wpl,
        format!("{file}/parse.wpl"),
        Some("package demo { rule a { ( chars:message ) } }".to_string()),
    )
    .await
    .expect("validate unsaved content");

    assert!(response.valid);

    let (saved_content, _) = read_rule_content(&test_project_layout(), RuleType::Wpl, &file)
        .expect("read persisted wpl parse")
        .expect("wpl parse should exist");
    assert!(saved_content.is_empty());

    cleanup_rule(RuleType::Wpl, &file);
}

#[tokio::test]
async fn test_validate_rule_logic_rejects_empty_wpl_parse_content() {
    setup_db().await;
    let file = format!("validate-empty-{}", rand_suffix());

    create_rule_file_logic(SystemKind::Wparse, RuleType::Wpl, file.clone())
        .await
        .expect("create empty wpl rule");

    let response = validate_rule_logic(
        SystemKind::Wparse,
        RuleType::Wpl,
        format!("{file}/parse.wpl"),
        Some(String::new()),
    )
    .await
    .expect("empty parse validation should return business response");

    assert!(!response.valid);
    assert_eq!(
        response.message.as_deref(),
        Some("参数验证失败: WPL 规则内容为空，请先填写 parse.wpl 后再校验")
    );

    cleanup_rule(RuleType::Wpl, &file);
}
