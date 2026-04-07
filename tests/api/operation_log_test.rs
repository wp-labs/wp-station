use crate::common::{rand_suffix, setup_db};
use actix_web::{App, http::StatusCode, test};
use chrono::Utc;
use wp_station::db::{NewOperationLog, create_operation_log};

#[actix_web::test]
async fn test_list_operation_logs_via_api() {
    setup_db().await;
    let operator = format!("api-logger-{}", rand_suffix());

    create_operation_log(NewOperationLog {
        operator: operator.clone(),
        operation: "create".to_string(),
        target: Some("operation-target".to_string()),
        description: Some("log created from api test".to_string()),
        content: Some("details for api test".to_string()),
        status: "success".to_string(),
    })
    .await
    .expect("create operation log");

    let today = Utc::now().format("%Y-%m-%d").to_string();

    let app = test::init_service(App::new().service(wp_station::api::list_operation_logs)).await;

    let uri = format!(
        "/api/operation-logs?operator={}&operation=create&start_date={}&end_date={}&page=1&page_size=5",
        operator, today, today
    );
    let req = test::TestRequest::get().uri(&uri).to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let payload: serde_json::Value = test::read_body_json(resp).await;

    let operators: Vec<String> = payload["items"]
        .as_array()
        .expect("items array")
        .iter()
        .filter_map(|item| {
            item.get("operator")
                .and_then(|op| op.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    assert!(
        operators.contains(&operator),
        "operation log list should contain the inserted record"
    );
}
