use crate::common::{
    init_default_configs_to_test_layout, remove_project_path, setup_db, test_infra_root,
    test_models_root,
};

#[tokio::test]
async fn test_init_default_configs_is_idempotent_for_project_files() {
    setup_db().await;
    let infra_root = test_infra_root();
    let models_root = test_models_root();

    remove_project_path("conf/wparse.toml");
    remove_project_path("models/knowledge/knowdb.toml");

    init_default_configs_to_test_layout();
    let wparse = infra_root.join("conf/wparse.toml");
    let knowdb = models_root.join("models/knowledge/knowdb.toml");
    assert!(wparse.is_file(), "wparse default should be restored");
    assert!(knowdb.is_file(), "knowdb default should be restored");

    std::fs::write(&wparse, "user-edited").expect("edit default config");
    init_default_configs_to_test_layout();
    let content = std::fs::read_to_string(&wparse).expect("read edited config");
    assert_eq!(
        content, "user-edited",
        "default loader must not overwrite user edits"
    );
}
