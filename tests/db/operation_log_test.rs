use crate::common::setup_db;
use chrono::{Duration, Utc};
use sea_orm::EntityTrait;
use wp_station::db::{NewOperationLog, create_operation_log, find_logs_page};
use wp_station_migrations::entity::operation_log::Entity as OperationLogEntity;

#[tokio::test]
async fn test_operation_log_create_and_query() {
    setup_db().await;
    let log = create_operation_log(NewOperationLog {
        operator: "tester".to_string(),
        operation: "create".to_string(),
        target: Some("device".to_string()),
        description: Some("desc".to_string()),
        content: Some("payload".to_string()),
        status: "success".to_string(),
    })
    .await
    .expect("create operation log");

    let start = Some(Utc::now() - Duration::minutes(1));
    let end = Some(Utc::now() + Duration::minutes(1));
    let (logs, total) = find_logs_page(Some("tester"), Some("create"), start, end, 1, 10)
        .await
        .expect("list logs");

    assert!(total >= 1);
    assert!(logs.iter().any(|entry| entry.id == log.id));

    OperationLogEntity::delete_by_id(log.id)
        .exec(wp_station::db::get_pool().inner())
        .await
        .ok();
}
