use wp_station_knowledge_sync::{AssetSyncConfig, AssetSyncService};

const ASSET_JOB_NAME: &str = "asset_subject_lookup_refresh";

/// 仅在显式启用时连接本地资产源库和目标库，避免常规测试修改数据库任务。
#[tokio::test]
async fn initializes_asset_job_once_without_running_sync() {
    if std::env::var("WP_KSYNC_ASSET_TEST").ok().as_deref() != Some("1") {
        return;
    }

    let config = AssetSyncConfig::from_env()
        .expect("资产同步环境参数应有效")
        .expect("应配置本地资产源库和目标库");
    let pool = sqlx::PgPool::connect(&config.target_database_url)
        .await
        .expect("本地资产目标库应可连接");
    let service = AssetSyncService::connect(config.clone())
        .await
        .expect("资产同步服务应可连接");

    let before_runs = sync_run_count(&pool).await;
    let first = service.initialize().await.expect("首次初始化应成功");
    let second = service.initialize().await.expect("重复初始化应成功");

    for status in [&first, &second] {
        assert!(status.initialized, "资产对象应初始化完成: {status:?}");
        assert_eq!(status.error, None, "pg_cron 初始化失败应使测试失败");
        assert_eq!(status.task_name.as_deref(), Some(ASSET_JOB_NAME));
        assert_eq!(status.task_active, Some(true));
        assert_eq!(
            status.task_schedule.as_deref(),
            Some(config.schedule.as_str())
        );
        assert_eq!(status.schedule_matches, Some(true));
        assert!(status.current_row_count.is_some());
    }

    let job_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可查询已注册的资产任务");
    assert_eq!(job_count, 1, "重复初始化不应创建第二个定时任务");

    // 初始化只准备数据库对象；同步摘要只能由后续 pg_cron 执行写入。
    assert_eq!(sync_run_count(&pool).await, before_runs);
    assert_eq!(first.current_row_count, second.current_row_count);
}

async fn sync_run_count(pool: &sqlx::PgPool) -> i64 {
    let exists: bool =
        sqlx::query_scalar("SELECT to_regclass('asset_lookup.sync_run_summary') IS NOT NULL")
            .fetch_one(pool)
            .await
            .expect("应可检查资产同步摘要表");
    if !exists {
        return 0;
    }

    sqlx::query_scalar("SELECT count(*) FROM asset_lookup.sync_run_summary")
        .fetch_one(pool)
        .await
        .expect("应可统计资产同步摘要")
}

/// 在目标库事务内验证三类资产转换，回滚后不改变现有查询数据。
#[tokio::test]
async fn converts_host_terminal_and_network_from_local_source() {
    if std::env::var("WP_KSYNC_ASSET_SYNC_TEST").ok().as_deref() != Some("1") {
        return;
    }

    let config = AssetSyncConfig::from_env()
        .expect("资产同步环境参数应有效")
        .expect("应配置本地资产源库和目标库");
    let service = AssetSyncService::connect(config.clone())
        .await
        .expect("连接资产目标库");
    service.initialize().await.expect("准备资产同步对象");

    let pool = sqlx::PgPool::connect(&config.target_database_url)
        .await
        .expect("连接目标库事务");
    let mut tx = pool.begin().await.expect("开启验证事务");
    sqlx::query("CALL asset_lookup.refresh_asset_subject_lookup()")
        .execute(&mut *tx)
        .await
        .expect("转换三类本地资产");

    let (host, terminal, network): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE asset_type='HOST'), \
                count(*) FILTER (WHERE asset_type='TERMINAL'), \
                count(*) FILTER (WHERE asset_type='NET_WORK') \
         FROM asset_lookup.asset_subject_exclude_biz_sync",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("统计三类转换结果");
    assert!(host > 0 && terminal > 0 && network > 0);

    let (written, summary_host, summary_terminal, summary_network): (i64, i64, i64, i64) =
        sqlx::query_as(
            "SELECT written_count,asset_count,terminal_count,network_count \
             FROM asset_lookup.sync_run_summary ORDER BY run_id DESC LIMIT 1",
        )
        .fetch_one(&mut *tx)
        .await
        .expect("读取同步摘要");
    assert_eq!(
        (summary_host, summary_terminal, summary_network),
        (host, terminal, network)
    );
    assert_eq!(written, host + terminal + network);

    let missing_source_fields: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_lookup.asset_subject_exclude_biz_sync \
         WHERE (asset_type IN ('HOST','TERMINAL') AND (ip IS NULL OR network IS NOT NULL)) \
            OR (asset_type='NET_WORK' AND (network IS NULL OR ip IS NOT NULL))",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("检查 IP 与 CIDR 字段归属");
    assert_eq!(missing_source_fields, 0);

    tx.rollback().await.expect("回滚验证事务");
}
