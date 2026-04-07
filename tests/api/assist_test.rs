use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};

fn assist_request_body(task_type: &str) -> serde_json::Value {
    serde_json::json!({
        "task_type": task_type,
        "target_rule": "wpl",
        "log_data": format!("assist-log-{}", rand_suffix()),
        "current_rule": "rule body",
        "extra_note": "notes from api test"
    })
}

#[actix_web::test]
async fn test_assist_api_endpoints_flow() {
    setup_db().await;
    let app = test::init_service(
        App::new()
            .service(wp_station::api::assist_submit)
            .service(wp_station::api::assist_list)
            .service(wp_station::api::assist_get)
            .service(wp_station::api::assist_reply)
            .service(wp_station::api::assist_cancel),
    )
    .await;

    // Submit a manual assist task
    let submit_req = test::TestRequest::post()
        .uri("/api/assist")
        .set_json(&assist_request_body("manual"))
        .to_request();
    let submit_resp = test::call_service(&app, submit_req).await;
    assert_eq!(submit_resp.status(), StatusCode::OK);
    let submit_payload: serde_json::Value = test::read_body_json(submit_resp).await;
    let task_id = submit_payload["task_id"]
        .as_str()
        .expect("task id field")
        .to_string();

    // Verify the task appears in the list endpoint
    let list_req = test::TestRequest::get()
        .uri("/api/assist?page=1&page_size=5")
        .to_request();
    let list_resp = test::call_service(&app, list_req).await;
    assert_eq!(list_resp.status(), StatusCode::OK);
    let list_payload: serde_json::Value = test::read_body_json(list_resp).await;
    assert!(
        list_payload["items"]
            .as_array()
            .expect("items array")
            .iter()
            .any(|item| item["task_id"] == task_id),
        "task should be returned from list endpoint"
    );

    // Fetch task detail
    let detail_req = test::TestRequest::get()
        .uri(&format!("/api/assist/{}", task_id))
        .to_request();
    let detail_resp = test::call_service(&app, detail_req).await;
    assert_eq!(detail_resp.status(), StatusCode::OK);
    let detail_payload: serde_json::Value = test::read_body_json(detail_resp).await;
    assert_eq!(detail_payload["status"], "pending");

    // Reply to the task with suggestions
    let reply_req = test::TestRequest::post()
        .uri("/api/assist/reply")
        .set_json(&serde_json::json!({
            "task_id": task_id.clone(),
            "wpl_suggestion": "formatted rule",
            "explanation": "analysis done"
        }))
        .to_request();
    let reply_resp = test::call_service(&app, reply_req).await;
    assert_eq!(reply_resp.status(), StatusCode::OK);

    let post_reply_detail = test::TestRequest::get()
        .uri(&format!("/api/assist/{}", task_id))
        .to_request();
    let post_reply_resp = test::call_service(&app, post_reply_detail).await;
    assert_eq!(post_reply_resp.status(), StatusCode::OK);
    let post_reply_payload: serde_json::Value = test::read_body_json(post_reply_resp).await;
    assert_eq!(post_reply_payload["status"], "success");
    assert_eq!(post_reply_payload["wpl_suggestion"], "formatted rule");

    // Create another task and cancel it while still pending
    let cancel_submit = test::TestRequest::post()
        .uri("/api/assist")
        .set_json(&assist_request_body("manual"))
        .to_request();
    let cancel_resp = test::call_service(&app, cancel_submit).await;
    assert_eq!(cancel_resp.status(), StatusCode::OK);
    let cancel_payload: serde_json::Value = test::read_body_json(cancel_resp).await;
    let cancel_task_id = cancel_payload["task_id"].as_str().unwrap();

    let cancel_req = test::TestRequest::post()
        .uri(&format!("/api/assist/{}/cancel", cancel_task_id))
        .to_request();
    let cancel_call = test::call_service(&app, cancel_req).await;
    assert_eq!(cancel_call.status(), StatusCode::OK);
}
