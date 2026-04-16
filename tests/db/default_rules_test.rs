use crate::common::{remove_project_path, setup_db, test_project_root};
use wp_station::db::init_default_configs_to_project;

#[tokio::test]
async fn test_init_default_configs_is_idempotent_for_project_files() {
    setup_db().await;
    let project_root = test_project_root();
    let project_root_str = project_root.to_str().expect("utf-8 test project root");

    remove_project_path("conf/wparse.toml");
    remove_project_path("models/knowledge/knowdb.toml");

    init_default_configs_to_project(project_root_str).expect("first default config load");
    let wparse = project_root.join("conf/wparse.toml");
    let knowdb = project_root.join("models/knowledge/knowdb.toml");
    assert!(wparse.is_file(), "wparse default should be restored");
    assert!(knowdb.is_file(), "knowdb default should be restored");

    std::fs::write(&wparse, "user-edited").expect("edit default config");
    init_default_configs_to_project(project_root_str).expect("second default config load");
    let content = std::fs::read_to_string(&wparse).expect("read edited config");
    assert_eq!(
        content, "user-edited",
        "default loader must not overwrite user edits"
    );
}
