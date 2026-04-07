use crate::common::setup_db;
use sea_orm::{ConnectionTrait, DatabaseBackend, EntityTrait, PaginatorTrait, Statement};
use wp_station::db::{get_pool, init_default_configs_from_embedded};
use wp_station_migrations::entity::knowledge_config::Entity as KnowledgeEntity;
use wp_station_migrations::entity::rule_config::Entity as RuleEntity;

#[tokio::test]
async fn test_init_default_configs_is_idempotent_for_rule_configs() {
    setup_db().await;
    let pool = get_pool();
    let conn = pool.inner();
    conn.execute(Statement::from_string(DatabaseBackend::Postgres, "BEGIN"))
        .await
        .expect("begin transaction for rule configs");
    conn.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        "DELETE FROM rule_configs",
    ))
    .await
    .expect("reset rule configs");

    init_default_configs_from_embedded(conn)
        .await
        .expect("run default config loader first time");
    let first_count = RuleEntity::find()
        .count(conn)
        .await
        .expect("count rule configs after first run");

    init_default_configs_from_embedded(conn)
        .await
        .expect("run default config loader second time");
    let second_count = RuleEntity::find()
        .count(conn)
        .await
        .expect("count rule configs after second run");

    assert!(first_count > 0, "default rules should be inserted");
    assert_eq!(
        first_count, second_count,
        "default rules should not duplicate on repeated init"
    );

    conn.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        "ROLLBACK",
    ))
    .await
    .expect("rollback rule config inserts");
}

#[tokio::test]
async fn test_init_default_configs_is_idempotent_for_knowledge_configs() {
    setup_db().await;
    let pool = get_pool();
    let conn = pool.inner();
    conn.execute(Statement::from_string(DatabaseBackend::Postgres, "BEGIN"))
        .await
        .expect("begin transaction for knowledge configs");
    conn.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        "DELETE FROM knowledge_configs",
    ))
    .await
    .expect("reset knowledge configs");

    init_default_configs_from_embedded(conn)
        .await
        .expect("run default config loader first time");
    let first_count = KnowledgeEntity::find()
        .count(conn)
        .await
        .expect("count knowledge configs after first run");

    init_default_configs_from_embedded(conn)
        .await
        .expect("run default config loader second time");
    let second_count = KnowledgeEntity::find()
        .count(conn)
        .await
        .expect("count knowledge configs after second run");

    assert!(
        first_count > 0,
        "default knowledge configs should be inserted"
    );
    assert_eq!(
        first_count, second_count,
        "default knowledge configs should not duplicate on repeated init"
    );

    conn.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        "ROLLBACK",
    ))
    .await
    .expect("rollback knowledge config inserts");
}
