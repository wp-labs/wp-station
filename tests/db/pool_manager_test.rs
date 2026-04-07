use crate::common::setup_db;
use wp_station::db::{get_pool, is_pool_initialized, try_get_pool};

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
