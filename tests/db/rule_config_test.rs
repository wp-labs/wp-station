use crate::common::{rand_suffix, setup_db, unique_name};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{
    NewRuleConfig, RuleType, create_rule_config, delete_rule_config, find_rule_by_type_and_name,
    find_rules_by_type, get_rule_file_names, is_rule_configs_empty, update_rule_content,
};
use wp_station_migrations::entity::rule_config::{Column as RuleColumn, Entity as RuleEntity};

async fn cleanup_rule(file: &str) {
    let pool = wp_station::db::get_pool();
    let _ = RuleEntity::delete_many()
        .filter(RuleColumn::FileName.eq(file))
        .exec(pool.inner())
        .await;
}

async fn create_sample_rule(rule_type: RuleType) -> String {
    setup_db().await;
    let file = format!("{}-{}.toml", unique_name("rule"), rand_suffix());
    let new_rule = NewRuleConfig {
        rule_type,
        file_name: file.clone(),
        display_name: None,
        content: Some("initial".to_string()),
        sample_content: None,
        file_size: Some(7),
    };
    create_rule_config(new_rule)
        .await
        .expect("create rule config");
    file
}

#[tokio::test]
async fn test_create_and_find_rule_config() {
    let file = create_sample_rule(RuleType::Wpl).await;

    let result = find_rule_by_type_and_name(RuleType::Wpl.as_ref(), &file)
        .await
        .expect("find rule")
        .expect("rule exists");
    assert_eq!(result.file_name, file);
    assert_eq!(result.content.as_deref(), Some("initial"));

    cleanup_rule(&file).await;
}

#[tokio::test]
async fn test_update_rule_content_and_listings() {
    let file = create_sample_rule(RuleType::Oml).await;

    update_rule_content(RuleType::Oml.as_ref(), &file, "updated", 7)
        .await
        .expect("update rule");

    let updated = find_rule_by_type_and_name(RuleType::Oml.as_ref(), &file)
        .await
        .expect("find updated")
        .expect("exists");
    assert_eq!(updated.content.as_deref(), Some("updated"));

    let list = find_rules_by_type(RuleType::Oml.as_ref())
        .await
        .expect("list rules");
    assert!(list.iter().any(|rule| rule.file_name == file));

    let file_names = get_rule_file_names(RuleType::Oml.as_ref())
        .await
        .expect("file names");
    assert!(file_names.contains(&file));

    cleanup_rule(&file).await;
}

#[tokio::test]
async fn test_delete_rule_config() {
    let file = create_sample_rule(RuleType::Sink).await;

    delete_rule_config(RuleType::Sink.as_ref(), &file)
        .await
        .expect("delete rule");

    let missing = find_rule_by_type_and_name(RuleType::Sink.as_ref(), &file)
        .await
        .expect("find after delete");
    assert!(missing.is_none());

    cleanup_rule(&file).await;
}

#[tokio::test]
async fn test_is_rule_configs_empty_flag() {
    let file = create_sample_rule(RuleType::Source).await;
    let has_rules = is_rule_configs_empty().await.expect("check flag");
    assert!(!has_rules, "there should be at least one active rule");

    delete_rule_config(RuleType::Source.as_ref(), &file)
        .await
        .expect("soft delete");
    cleanup_rule(&file).await;
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
