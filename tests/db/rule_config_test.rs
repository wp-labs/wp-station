use crate::common::{rand_suffix, remove_project_path, setup_db, test_project_root, unique_name};
use wp_station::db::RuleType;
use wp_station::utils::{
    delete_rule_from_project, list_rule_files, read_rule_content, touch_rule_in_project,
    write_rule_content,
};

fn project_root_str() -> String {
    test_project_root().to_string_lossy().to_string()
}

#[tokio::test]
async fn test_create_and_read_rule_project_file() {
    setup_db().await;
    let root = project_root_str();
    let file = format!("{}-{}.toml", unique_name("rule"), rand_suffix());

    write_rule_content(&root, RuleType::Source, &file, "initial").expect("write source rule");
    let (content, _) = read_rule_content(&root, RuleType::Source, &file)
        .expect("read source rule")
        .unwrap();
    assert_eq!(content, "initial");

    remove_project_path(format!("topology/sources/{file}"));
}

#[tokio::test]
async fn test_update_rule_content_and_listings() {
    setup_db().await;
    let root = project_root_str();
    let file = format!("{}-{}.toml", unique_name("oml"), rand_suffix());

    write_rule_content(&root, RuleType::Oml, &file, "initial").expect("write oml");
    write_rule_content(&root, RuleType::Oml, &file, "updated").expect("update oml");

    let (updated, _) = read_rule_content(&root, RuleType::Oml, &file)
        .expect("read updated")
        .unwrap();
    assert_eq!(updated, "updated");

    let file_names = list_rule_files(&root, RuleType::Oml).expect("list oml files");
    assert!(file_names.contains(&file));

    remove_project_path(format!("models/oml/{file}"));
}

#[tokio::test]
async fn test_delete_rule_project_file() {
    setup_db().await;
    let root = project_root_str();
    let file = format!("{}-{}.toml", unique_name("sink"), rand_suffix());

    touch_rule_in_project(&root, RuleType::Sink, &file).expect("touch sink");
    delete_rule_from_project(&root, RuleType::Sink, &file).expect("delete sink");

    let missing = read_rule_content(&root, RuleType::Sink, &file).expect("read deleted");
    assert!(missing.is_none());
}

#[tokio::test]
async fn test_rule_type_helpers_cover_all_variants() {
    let variants = vec![
        RuleType::All,
        RuleType::Wpl,
        RuleType::Oml,
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
        let components = variant.to_check_component();
        assert!(
            !components.is_empty(),
            "each rule type should map to at least one component"
        );
    }
}
