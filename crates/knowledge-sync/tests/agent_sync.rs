mod common;

use wp_station_knowledge_sync::agent::parse_agent_mapping_json;

#[test]
fn parses_matrix_and_rejects_conflicting_agent_ips() {
    let body = r#"{"status":"success","data":{"resultType":"matrix","result":[{"metric":{"agent":"agent-a","host_ip":"10.0.0.1"},"values":[[1794758400,"1"],[1794844800,"1"]]},{"metric":{"agent":"agent-b","host_ip":"2001:db8::1"},"values":[[1794758400,"1"]]}]}}"#;
    let (mapping, read) = parse_agent_mapping_json(body).expect("解析 Agent 数据");
    assert_eq!(read, 2);
    assert_eq!(mapping.get("agent-a").map(String::as_str), Some("10.0.0.1"));
    assert_eq!(
        mapping.get("agent-b").map(String::as_str),
        Some("2001:db8::1")
    );

    let conflicting = body.replace("agent-b", "agent-a");
    assert!(parse_agent_mapping_json(&conflicting).is_err());
    let empty = r#"{"status":"success","data":{"resultType":"matrix","result":[]}}"#;
    assert!(parse_agent_mapping_json(empty).is_err());
}

#[tokio::test]
async fn agent_run_table_is_created_by_station_migration() {
    let (_dir, store) = common::knowledge_sync_store().await;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM agent_sync_runs")
        .fetch_one(&store)
        .await
        .expect("读取 Agent 同步运行表");
    assert_eq!(count, 0);
}
