use crate::common::setup_db;
use wp_station::DatabaseConf;
use wp_station::db::{get_pool, init_pool, is_pool_initialized, try_get_pool};

#[tokio::test]
async fn test_pool_manager_helpers() {
    setup_db().await;
    assert!(is_pool_initialized());
    let pool = get_pool();
    pool.test_connection()
        .await
        .expect("db pool should be healthy");
    assert!(try_get_pool().is_some());
}

#[tokio::test]
async fn test_init_pool_creates_missing_sqlite_file() {
    let base = std::env::temp_dir().join(format!(
        "wp-station-sqlite-init-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    let db_path = base.join("data/station.db");
    let conf = DatabaseConf {
        url: format!("sqlite://{}", db_path.display()),
        ..DatabaseConf::default()
    };

    init_pool(&conf)
        .await
        .expect("sqlite init pool should create missing db file");

    assert!(db_path.exists(), "sqlite db file should be created");

    let _ = std::fs::remove_dir_all(base);
}
