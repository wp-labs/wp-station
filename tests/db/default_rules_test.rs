use crate::common::{
    init_default_configs_to_test_layout, remove_project_path, setup_db, test_infra_root,
    test_models_root,
};

#[tokio::test]
async fn test_init_default_configs_preserves_existing_managed_project() {
    setup_db().await;
    let infra_root = test_infra_root();
    let models_root = test_models_root();

    remove_project_path("conf/wparse.toml");
    remove_project_path("models/knowledge/knowdb.toml");

    init_default_configs_to_test_layout();
    let wparse = infra_root.join("conf/wparse.toml");
    let knowdb = models_root.join("models/knowledge/knowdb.toml");
    assert!(
        !wparse.exists(),
        "existing infra repository should not backfill a deliberately removed file"
    );
    assert!(
        !knowdb.exists(),
        "existing models repository should not backfill a deliberately removed file"
    );

    init_default_configs_to_test_layout();
    std::fs::write(&wparse, "user-edited").expect("edit default config");
    init_default_configs_to_test_layout();
    let content = std::fs::read_to_string(&wparse).expect("read edited config");
    assert_eq!(
        content, "user-edited",
        "default loader must not overwrite user edits"
    );
}

#[tokio::test]
async fn test_init_default_wparse_business_sinks_backfills_missing_template() {
    setup_db().await;
    let business_sink = test_infra_root().join("topology/sinks/business.d/sink.toml");
    std::fs::remove_file(&business_sink).expect("remove business sink template");

    init_default_configs_to_test_layout();

    let content = std::fs::read_to_string(&business_sink)
        .expect("missing wparse business sink should be backfilled");
    assert!(
        content.contains("name = \"kafka_sink\""),
        "backfilled business sink should use the default template"
    );
}
