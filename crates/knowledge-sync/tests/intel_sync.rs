use wp_station_knowledge_sync::intel::{IntelSyncConfig, IntelSyncService};
#[allow(dead_code)]
mod common;

#[tokio::test]
async fn syncs_local_doris_into_redis() {
    if std::env::var("WP_KSYNC_INTEL_TEST").ok().as_deref() != Some("1") {
        return;
    }
    let config = IntelSyncConfig::from_env()
        .expect("情报同步环境参数应有效")
        .expect("应配置本地 Doris 和 Redis");
    let (_dir, store) = common::knowledge_sync_store().await;
    let service = IntelSyncService::connect(config, store)
        .await
        .expect("本地服务应可连接");
    service.initialize().await.expect("运行记录应可初始化");
    let run = service.run_once().await.expect("全量同步应成功");
    assert_eq!(run.status, "succeeded");
    assert!(run.read_count > 0);
    assert!(run.ip_count + run.domain_count > 0);
    let status = service.status().await.expect("状态应可查询");
    assert_eq!(status.ip_count, Some(run.ip_count));
    assert_eq!(status.domain_count, Some(run.domain_count));
}
