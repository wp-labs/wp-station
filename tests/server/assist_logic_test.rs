use crate::common::{rand_suffix, setup_db};
use tokio::time::{Duration, sleep};
use wp_station::server::assist_task::{
    AssistListQuery, AssistReplyRequest, AssistSubmitRequest, assist_cancel_logic,
    assist_get_logic, assist_list_logic, assist_reply_logic, assist_submit_logic,
};
use wp_station::utils::pagination::PageQuery;

#[tokio::test]
async fn test_assist_task_lifecycle() {
    setup_db().await;

    let submit_resp = assist_submit_logic(AssistSubmitRequest {
        task_type: "manual".to_string(),
        target_rule: "wpl".to_string(),
        log_data: format!("sample-log-{}", rand_suffix()),
        current_rule: Some("rule body".to_string()),
        extra_note: Some("manual note".to_string()),
    })
    .await
    .expect("submit manual assist");

    // Allow background tasks to progress
    sleep(Duration::from_millis(50)).await;

    let list = assist_list_logic(AssistListQuery {
        page: PageQuery {
            page: Some(1),
            page_size: Some(10),
        },
    })
    .await
    .expect("list assist tasks");
    assert!(
        list.items
            .iter()
            .any(|item| item.task_id == submit_resp.task_id)
    );

    let detail = assist_get_logic(submit_resp.task_id.clone())
        .await
        .expect("get assist detail");
    assert_eq!(detail.task_type, "manual");

    assist_reply_logic(AssistReplyRequest {
        task_id: submit_resp.task_id.clone(),
        wpl_suggestion: Some("suggest".to_string()),
        oml_suggestion: None,
        explanation: Some("done".to_string()),
    })
    .await
    .expect("reply assist");

    let updated = assist_get_logic(submit_resp.task_id.clone())
        .await
        .expect("detail after reply");
    assert_eq!(updated.status, "success");
    assert_eq!(updated.wpl_suggestion.as_deref(), Some("suggest"));

    // Create another task and immediately cancel it
    let cancel_resp = assist_submit_logic(AssistSubmitRequest {
        task_type: "manual".to_string(),
        target_rule: "oml".to_string(),
        log_data: "cancel-log".to_string(),
        current_rule: None,
        extra_note: None,
    })
    .await
    .expect("submit assist for cancel");

    assist_cancel_logic(cancel_resp.task_id.clone())
        .await
        .expect("cancel pending assist");
}
