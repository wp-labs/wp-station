use crate::common::{rand_suffix, setup_db};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use wp_station::db::{
    AssistTargetRule, AssistTaskStatus, AssistTaskType, NewAssistTask, create_assist_task,
    find_assist_task_by_id, list_assist_tasks, update_assist_task_reply, update_assist_task_status,
};
use wp_station_migrations::entity::assist_task::{Column as AssistColumn, Entity as AssistEntity};

async fn cleanup_task(task_id: &str) {
    let pool = wp_station::db::get_pool();
    let _ = AssistEntity::delete_many()
        .filter(AssistColumn::TaskId.eq(task_id))
        .exec(pool.inner())
        .await;
}

#[tokio::test]
async fn test_assist_task_lifecycle() {
    setup_db().await;
    let task_id = format!("assist-{}", rand_suffix());
    let new_task = NewAssistTask {
        task_id: task_id.clone(),
        task_type: AssistTaskType::Ai,
        target_rule: AssistTargetRule::Wpl,
        log_data: "[]".to_string(),
        current_rule: Some("initial".to_string()),
        extra_note: Some("note".to_string()),
    };

    let id = create_assist_task(new_task)
        .await
        .expect("create assist task");
    assert!(id > 0);

    let fetched = find_assist_task_by_id(&task_id)
        .await
        .expect("find task")
        .expect("task exists");
    assert_eq!(fetched.task_type, AssistTaskType::Ai.as_ref());

    let (tasks, total) = list_assist_tasks(1, 20).await.expect("list tasks");
    assert!(total >= 1);
    assert!(tasks.iter().any(|task| task.task_id == task_id));

    update_assist_task_status(
        &task_id,
        AssistTaskStatus::Processing,
        Some("working".to_string()),
    )
    .await
    .expect("update status");

    update_assist_task_reply(
        &task_id,
        Some("wpl result".to_string()),
        Some("oml result".to_string()),
        Some("done".to_string()),
    )
    .await
    .expect("update reply");

    let updated = find_assist_task_by_id(&task_id)
        .await
        .expect("find updated")
        .expect("task exists");
    assert_eq!(updated.status, AssistTaskStatus::Success.as_ref());
    assert_eq!(updated.wpl_suggestion.as_deref(), Some("wpl result"));
    assert_eq!(updated.oml_suggestion.as_deref(), Some("oml result"));

    cleanup_task(&task_id).await;
}
