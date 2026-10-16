use crate::common::{
    rand_suffix, remove_project_path, resolve_project_path, setup_db, test_project_layout,
    unique_name,
};
use std::fs;
use wp_station::db::RuleType;
use wp_station::utils::{
    delete_rule_from_project, list_rule_files, read_rule_content, touch_rule_in_project,
    write_rule_content,
};

#[tokio::test]
async fn test_create_and_read_rule_project_file() {
    setup_db().await;
    let layout = test_project_layout();
    let file = format!("{}-{}.toml", unique_name("rule"), rand_suffix());

    write_rule_content(&layout, RuleType::Source, &file, "initial").expect("write source rule");
    let (content, _) = read_rule_content(&layout, RuleType::Source, &file)
        .expect("read source rule")
        .unwrap();
    assert_eq!(content, "initial");

    remove_project_path(format!("topology/sources/{file}"));
}

#[tokio::test]
async fn test_update_rule_content_and_listings() {
    setup_db().await;
    let layout = test_project_layout();
    let file = format!("{}-{}.toml", unique_name("oml"), rand_suffix());

    write_rule_content(&layout, RuleType::Oml, &file, "initial").expect("write oml");
    write_rule_content(&layout, RuleType::Oml, &file, "updated").expect("update oml");

    let (updated, _) = read_rule_content(&layout, RuleType::Oml, &file)
        .expect("read updated")
        .unwrap();
    assert_eq!(updated, "updated");

    let file_names = list_rule_files(&layout, RuleType::Oml).expect("list oml files");
    assert!(file_names.contains(&file));

    remove_project_path(format!("models/oml/{file}"));
}

#[tokio::test]
async fn test_delete_rule_project_file() {
    setup_db().await;
    let layout = test_project_layout();
    let file = format!("{}-{}.toml", unique_name("sink"), rand_suffix());

    touch_rule_in_project(&layout, RuleType::Sink, &file).expect("touch sink");
    delete_rule_from_project(&layout, RuleType::Sink, &file).expect("delete sink");

    let missing = read_rule_content(&layout, RuleType::Sink, &file).expect("read deleted");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_wfusion_schema_and_rule_files_cleanup_parent_dirs() {
    setup_db().await;
    let layout = test_project_layout();

    let schema_name = unique_name("schema");
    let schema_file = format!("{schema_name}/{schema_name}.wfs");
    write_rule_content(&layout, RuleType::Schema, &schema_file, "schema demo")
        .expect("write schema");
    let (schema_content, _) = read_rule_content(&layout, RuleType::Schema, &schema_file)
        .expect("read schema")
        .unwrap();
    assert_eq!(schema_content, "schema demo");
    let schema_files = list_rule_files(&layout, RuleType::Schema).expect("list schema files");
    assert!(schema_files.contains(&schema_file));
    delete_rule_from_project(&layout, RuleType::Schema, &schema_file).expect("delete schema");
    let schema_dir = resolve_project_path(format!("models/schemas/{schema_name}"));
    assert!(
        !schema_dir.exists(),
        "schema parent dir should be removed when empty"
    );

    let rule_name = unique_name("rule");
    let rule_file = format!("{rule_name}/{rule_name}.wfl");
    write_rule_content(&layout, RuleType::Rule, &rule_file, "rule demo").expect("write rule");
    let (rule_content, _) = read_rule_content(&layout, RuleType::Rule, &rule_file)
        .expect("read rule")
        .unwrap();
    assert_eq!(rule_content, "rule demo");
    let rule_files = list_rule_files(&layout, RuleType::Rule).expect("list rule files");
    assert!(rule_files.contains(&rule_file));
    delete_rule_from_project(&layout, RuleType::Rule, &rule_file).expect("delete rule");
    let rule_dir = resolve_project_path(format!("models/rules/{rule_name}"));
    assert!(
        !rule_dir.exists(),
        "rule parent dir should be removed when empty"
    );

    let scenario_name = unique_name("scenario");
    let scenario_file = format!("{scenario_name}/{scenario_name}.wfg");
    write_rule_content(
        &layout,
        RuleType::Scenarios,
        &scenario_file,
        "scenario demo",
    )
    .expect("write scenario");
    let (scenario_content, _) = read_rule_content(&layout, RuleType::Scenarios, &scenario_file)
        .expect("read scenario")
        .unwrap();
    assert_eq!(scenario_content, "scenario demo");
    let scenario_files =
        list_rule_files(&layout, RuleType::Scenarios).expect("list scenario files");
    assert!(scenario_files.contains(&scenario_file));
    delete_rule_from_project(&layout, RuleType::Scenarios, &scenario_file)
        .expect("delete scenario");
    let scenario_path = resolve_project_path(format!(
        "models/scenarios/{scenario_name}/{scenario_name}.wfg"
    ));
    assert!(
        !scenario_path.exists(),
        "scenario file should be removed after delete"
    );

    let _ = fs::remove_dir_all(resolve_project_path("models/schemas"));
    let _ = fs::remove_dir_all(resolve_project_path("models/rules"));
    let _ = fs::remove_dir_all(resolve_project_path("models/scenarios"));
}

#[tokio::test]
async fn test_wfusion_flat_rule_files_can_be_read_by_listed_name() {
    setup_db().await;
    let layout = test_project_layout();

    let schema_file = format!("{}.wfs", unique_name("schema_flat"));
    write_rule_content(&layout, RuleType::Schema, &schema_file, "schema flat")
        .expect("write flat schema");
    let schema_files = list_rule_files(&layout, RuleType::Schema).expect("list flat schemas");
    assert!(schema_files.contains(&schema_file));
    let (schema_content, _) = read_rule_content(&layout, RuleType::Schema, &schema_file)
        .expect("read flat schema")
        .unwrap();
    assert_eq!(schema_content, "schema flat");

    let rule_file = format!("{}.wfl", unique_name("rule_flat"));
    write_rule_content(&layout, RuleType::Rule, &rule_file, "rule flat").expect("write flat rule");
    let rule_files = list_rule_files(&layout, RuleType::Rule).expect("list flat rules");
    assert!(rule_files.contains(&rule_file));
    let (rule_content, _) = read_rule_content(&layout, RuleType::Rule, &rule_file)
        .expect("read flat rule")
        .unwrap();
    assert_eq!(rule_content, "rule flat");

    let scenario_file = format!("{}.wfg", unique_name("scenario_flat"));
    write_rule_content(
        &layout,
        RuleType::Scenarios,
        &scenario_file,
        "scenario flat",
    )
    .expect("write flat scenario");
    let scenario_files =
        list_rule_files(&layout, RuleType::Scenarios).expect("list flat scenarios");
    assert!(scenario_files.contains(&scenario_file));
    let (scenario_content, _) = read_rule_content(&layout, RuleType::Scenarios, &scenario_file)
        .expect("read flat scenario")
        .unwrap();
    assert_eq!(scenario_content, "scenario flat");
}

#[tokio::test]
async fn test_rule_type_helpers_cover_all_variants() {
    let variants = vec![
        RuleType::All,
        RuleType::Wpl,
        RuleType::Oml,
        RuleType::Schema,
        RuleType::Rule,
        RuleType::Scenarios,
        RuleType::Knowledge,
        RuleType::Source,
        RuleType::Sink,
        RuleType::Parse,
        RuleType::Wpgen,
        RuleType::SourceConnect,
        RuleType::SinkConnect,
    ];

    for variant in variants {
        let as_ref = variant.as_ref();
        assert!(!as_ref.is_empty());
        let components = variant.to_wparse_check_components();
        assert!(
            !components.is_empty(),
            "each rule type should map to at least one component"
        );
    }
}
