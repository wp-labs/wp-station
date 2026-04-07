use wp_station::{DatabaseConf, Setting};

#[test]
fn test_database_conf_connection_string_helpers() {
    let conf = DatabaseConf::default();
    let base = conf.connection_string();
    assert!(base.contains(&conf.name));

    let with_options = conf.connection_string_with_options();
    assert!(with_options.ends_with(&format!("?sslmode={}", conf.ssl_mode)));
}

#[test]
fn test_workspace_root_points_to_existing_dir() {
    let root = Setting::workspace_root();
    assert!(root.exists(), "workspace root should exist: {:?}", root);
}

#[test]
fn test_setting_default_values() {
    let setting = wp_station::server::setting::Setting::default();
    assert_eq!(setting.web.port, 8081);
    assert_eq!(setting.database.port, 5432);
    assert_eq!(setting.assist.base_url, String::new());
}
