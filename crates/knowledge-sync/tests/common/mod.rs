use wp_station_knowledge_sync::RunStore;
use wp_station_migrations::{Migrator, MigratorTrait};

/// 使用 Station 的真实迁移创建独立运行记录库，避免测试共享开发数据。
pub async fn knowledge_sync_store() -> (tempfile::TempDir, RunStore) {
    let dir = tempfile::tempdir().expect("创建知识同步测试目录");
    let url = format!(
        "sqlite://{}?mode=rwc",
        dir.path().join("station.db").display()
    );
    let db = sea_orm::Database::connect(&url)
        .await
        .expect("连接测试 SQLite");
    Migrator::up(&db, None).await.expect("执行 Station 迁移");
    drop(db);
    let store = wp_station_knowledge_sync::open_run_store(&url)
        .await
        .expect("连接知识同步运行记录库");
    (dir, store)
}
