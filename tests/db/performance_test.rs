use crate::common::{rand_suffix, setup_db};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::performance::{add_performance_result, update_performance_task_status};
use wp_station::db::{
    NewPerformanceTask, create_performance_task, find_performance_task_by_id,
    get_performance_results,
};
use wp_station_migrations::entity::performance::result::{
    Column as ResultColumn, Entity as ResultEntity,
};
use wp_station_migrations::entity::performance::{
    Column as PerformanceColumn, Entity as PerformanceEntity,
};

async fn cleanup_performance(task_id_value: i32, task_key: &str) {
    let pool = wp_station::db::get_pool();
    let db = pool.inner();
    let _ = ResultEntity::delete_many()
        .filter(ResultColumn::TaskId.eq(task_id_value))
        .exec(db)
        .await;
    let _ = PerformanceEntity::delete_many()
        .filter(PerformanceColumn::TaskId.eq(task_key))
        .exec(db)
        .await;
}

#[tokio::test]
async fn test_performance_task_flow() {
    setup_db().await;
    let task_key = format!("task-{}", rand_suffix());
    let task = NewPerformanceTask {
        task_id: task_key.clone(),
        sample_data: Some("data".to_string()),
        config_content: Some("conf".to_string()),
        created_by: Some("tester".to_string()),
    };

    let record_id = create_performance_task(task)
        .await
        .expect("create performance task");

    let fetched = find_performance_task_by_id(&task_key)
        .await
        .expect("find task")
        .expect("task exists");
    assert_eq!(fetched.status, "running");

    update_performance_task_status(&task_key, "completed")
        .await
        .expect("update status");

    add_performance_result(record_id, "sink-a", 120, 2000, "ok")
        .await
        .expect("add result");
    add_performance_result(record_id, "sink-b", 240, 1000, "ok")
        .await
        .expect("add second result");

    let results = get_performance_results(record_id)
        .await
        .expect("get results");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].sink_name, "sink-a");

    cleanup_performance(record_id, &task_key).await;
}
