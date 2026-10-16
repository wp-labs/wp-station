use wp_station::{DatabaseConf, DatabaseKind, Setting};

#[test]
fn test_database_conf_connection_string_helpers() {
    let conf = DatabaseConf::default();
    let base = conf.connection_string();
    assert!(base.contains(&conf.name));

    let with_options = conf.connection_string_with_options();
    assert!(with_options.ends_with(&format!("?sslmode={}", conf.ssl_mode)));
}

#[test]
fn test_database_conf_prefers_sqlite_url_when_present() {
    let conf = DatabaseConf {
        url: "sqlite:///tmp/wp-station/station.db".to_string(),
        ..DatabaseConf::default()
    };

    assert_eq!(conf.database_kind(), DatabaseKind::Sqlite);
    assert_eq!(conf.connection_string(), conf.url);
    assert_eq!(conf.connection_string_with_options(), conf.url);
    assert!(conf.safe_summary().contains("/tmp/wp-station/station.db"));
}

#[test]
fn test_database_conf_resolves_relative_sqlite_url_to_workspace() {
    let conf = DatabaseConf {
        url: "sqlite://db/station.db".to_string(),
        ..DatabaseConf::default()
    };

    let expected = format!(
        "sqlite://{}/db/station.db",
        Setting::workspace_root().display()
    );
    assert_eq!(conf.connection_string(), expected);
    assert_eq!(conf.connection_string_with_options(), expected);
}

#[test]
fn test_database_conf_deserializes_with_sqlite_url_only() {
    let conf: DatabaseConf = toml::from_str(
        r#"
url = "sqlite:///tmp/wp-station/station.db"
"#,
    )
    .expect("sqlite-only database config should deserialize");

    assert_eq!(conf.database_kind(), DatabaseKind::Sqlite);
    assert_eq!(
        conf.connection_string(),
        "sqlite:///tmp/wp-station/station.db"
    );
    assert_eq!(conf.host, "localhost");
    assert_eq!(conf.port, 5432);
}

#[test]
fn test_database_conf_keeps_postgres_url_mode() {
    let conf = DatabaseConf {
        url: "postgresql://station:secret@db.example.com:5432/wp-station".to_string(),
        ..DatabaseConf::default()
    };

    assert_eq!(conf.database_kind(), DatabaseKind::Postgres);
    assert_eq!(conf.connection_string(), conf.url);
    assert_eq!(conf.connection_string_with_options(), conf.url);
    assert_eq!(conf.safe_summary(), "postgres:url(provided)");
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
