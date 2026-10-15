use wp_station_knowledge_sync::{legacy::migrate_legacy_runs, open_run_store};
use wp_station_migrations::{Migrator, MigratorTrait};
#[allow(dead_code)]
mod common;

#[tokio::test]
async fn migrates_both_legacy_run_tables_once() {
    let Ok(pg_url) = std::env::var("WP_KSYNC_LEGACY_FIXTURE_URL") else {
        return;
    };
    let pg = sqlx::PgPool::connect(&pg_url)
        .await
        .expect("连接本地临时 PostgreSQL");
    sqlx::raw_sql(
        "CREATE SCHEMA knowledge_sync; \
         CREATE TABLE knowledge_sync.geo_runs ( \
           id bigint PRIMARY KEY, status text NOT NULL, stage text NOT NULL, file_name text NOT NULL, \
           file_hash text, database_type text, build_epoch bigint, row_count bigint, row_limit bigint, \
           progress_rows bigint NOT NULL, progress_percent integer, error text, \
           started_at timestamptz NOT NULL, finished_at timestamptz); \
         CREATE TABLE knowledge_sync.intel_runs ( \
           id bigint PRIMARY KEY, status text NOT NULL, stage text NOT NULL, \
           read_count bigint NOT NULL, invalid_count bigint NOT NULL, expired_count bigint NOT NULL, \
           ip_count bigint NOT NULL, domain_count bigint NOT NULL, error text, \
           started_at timestamptz NOT NULL, finished_at timestamptz); \
         INSERT INTO knowledge_sync.geo_runs \
           (id,status,stage,file_name,row_count,row_limit,progress_rows,started_at,finished_at) \
           VALUES (7,'succeeded','completed','test.mmdb',5000,5000,5000,now(),now()); \
         INSERT INTO knowledge_sync.intel_runs \
           (id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count,started_at,finished_at) \
           VALUES (9,'succeeded','completed',4,1,0,2,1,now(),now())",
    )
    .execute(&pg)
    .await
    .expect("创建旧版测试记录");
    let (_dir, store) = common::knowledge_sync_store().await;
    migrate_legacy_runs(&pg_url, &store)
        .await
        .expect("迁移旧记录");
    migrate_legacy_runs(&pg_url, &store)
        .await
        .expect("重复迁移应幂等");
    let geo: i64 =
        sqlx::query_scalar("SELECT count(*) FROM geo_sync_runs WHERE legacy_pg_run_id=7")
            .fetch_one(&store)
            .await
            .expect("读取 Geo 迁移结果");
    let intel: i64 =
        sqlx::query_scalar("SELECT count(*) FROM intel_sync_runs WHERE legacy_pg_run_id=9")
            .fetch_one(&store)
            .await
            .expect("读取情报迁移结果");
    assert_eq!((geo, intel), (1, 1));
    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='knowledge_sync')",
    )
    .fetch_one(&pg)
    .await
    .expect("检查旧 schema");
    assert!(!schema_exists);
}

#[tokio::test]
async fn migrates_local_legacy_run_history_to_station_sqlite() {
    if std::env::var("WP_KSYNC_LEGACY_MIGRATE_LOCAL")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }
    let pg_url = std::env::var("WP_KSYNC_TARGET_URL")
        .or_else(|_| std::env::var("WP_KSYNC_ASSET_TARGET_URL"))
        .expect("本地目标 PostgreSQL 地址");
    let sqlite_url = std::env::var("WP_KSYNC_STATION_SQLITE_URL").expect("Station SQLite 地址");
    let db = sea_orm::Database::connect(&sqlite_url)
        .await
        .expect("连接 Station SQLite");
    Migrator::up(&db, None).await.expect("执行 Station 迁移");
    drop(db);
    let store = open_run_store(&sqlite_url).await.expect("连接运行记录库");
    migrate_legacy_runs(&pg_url, &store)
        .await
        .expect("迁移旧运行记录");
    let pg = sqlx::PgPool::connect(&pg_url)
        .await
        .expect("连接本地 PostgreSQL");
    let schema_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='knowledge_sync')",
    )
    .fetch_one(&pg)
    .await
    .expect("检查旧 schema");
    assert!(!schema_exists);
    let geo_count: i64 = sqlx::query_scalar("SELECT count(*) FROM geo_sync_runs")
        .fetch_one(&store)
        .await
        .expect("查询 Geo 历史");
    let intel_count: i64 = sqlx::query_scalar("SELECT count(*) FROM intel_sync_runs")
        .fetch_one(&store)
        .await
        .expect("查询情报历史");
    assert!(geo_count > 0 || intel_count > 0);
}
