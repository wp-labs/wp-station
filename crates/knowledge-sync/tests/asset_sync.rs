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

    // 使用可连接的回环地址验证原位修改，随后恢复，避免触及其他测试环境。
    if config.source_host == "127.0.0.1" || config.source_host == "localhost" {
        let mut alternate_source = config.clone();
        alternate_source.source_host = if config.source_host == "localhost" {
            "127.0.0.1".to_string()
        } else {
            "localhost".to_string()
        };
        AssetSyncService::connect(alternate_source.clone())
            .await
            .expect("应可连接资产目标库")
            .initialize()
            .await
            .expect("应按环境变量修改资产源主机");
        let options: Vec<String> = sqlx::query_scalar(
            "SELECT srvoptions FROM pg_foreign_server WHERE srvname='dysec_asset_source'",
        )
        .fetch_one(&pool)
        .await
        .expect("应可读取资产源库连接定义");
        assert!(options.contains(&format!("host={}", alternate_source.source_host)));
        service.initialize().await.expect("应恢复资产源库主机");
    }

    let original_mapping: Vec<String> = sqlx::query_scalar(
        "SELECT umoptions FROM pg_user_mapping WHERE umserver=(SELECT oid FROM pg_foreign_server WHERE srvname='dysec_asset_source') AND umuser=(SELECT oid FROM pg_roles WHERE rolname=current_user)",
    )
    .fetch_one(&pool)
    .await
    .expect("应可读取当前用户映射");
    let mut wrong_password = config.clone();
    wrong_password.source_password = "test-secret-not-for-display".to_string();
    let password_error = AssetSyncService::connect(wrong_password)
        .await
        .expect("连接资产目标库时尚不读取资产源库")
        .initialize()
        .await
        .expect_err("错误密码应导致源库验证失败");
    let password_error = format!("{password_error:#}");
    assert!(password_error.contains("连接资产源库失败"));
    assert!(!password_error.contains("test-secret-not-for-display"));
    let restored_mapping: Vec<String> = sqlx::query_scalar(
        "SELECT umoptions FROM pg_user_mapping WHERE umserver=(SELECT oid FROM pg_foreign_server WHERE srvname='dysec_asset_source') AND umuser=(SELECT oid FROM pg_roles WHERE rolname=current_user)",
    )
    .fetch_one(&pool)
    .await
    .expect("应可读取回滚后的用户映射");
    assert!(
        restored_mapping == original_mapping,
        "失败时不应修改已有凭据"
    );

    let job_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可查询已注册的资产任务");
    assert_eq!(job_count, 1, "重复初始化不应创建第二个定时任务");

    let socket_directories: String = sqlx::query_scalar("SHOW unix_socket_directories")
        .fetch_one(&pool)
        .await
        .expect("应可读取目标库 socket 目录");
    let expected_host = config.cron_host.as_deref().unwrap_or_else(|| {
        socket_directories
            .split(',')
            .next()
            .expect("应存在 socket 目录")
            .trim()
    });
    let actual_host: String = sqlx::query_scalar(
        "SELECT nodename FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可读取任务连接地址");
    assert_eq!(actual_host, expected_host);

    let job_id: i64 = sqlx::query_scalar(
        "SELECT jobid FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可读取资产任务 ID");
    let alternate_schedule = if config.schedule == "0 0 1 1 *" {
        "0 0 2 1 *"
    } else {
        "0 0 1 1 *"
    };
    let mut alternate_config = config.clone();
    alternate_config.schedule = alternate_schedule.to_string();
    alternate_config.cron_host = Some("localhost".to_string());
    let alternate_service = AssetSyncService::connect(alternate_config)
        .await
        .expect("应可连接本地资产目标库");
    let changed = alternate_service
        .initialize()
        .await
        .expect("应原位更新任务周期");
    let changed_host: String = sqlx::query_scalar(
        "SELECT nodename FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可核对任务连接地址");
    assert_eq!(changed_host, "localhost");
    let restored = service.initialize().await.expect("应恢复原部署周期");
    assert_eq!(changed.task_schedule.as_deref(), Some(alternate_schedule));
    assert_eq!(
        restored.task_schedule.as_deref(),
        Some(config.schedule.as_str())
    );
    assert_eq!(restored.schedule_matches, Some(true));
    let restored_host: String = sqlx::query_scalar(
        "SELECT nodename FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可核对恢复后的任务连接地址");
    assert_eq!(restored_host, expected_host);
    let current_job_id: i64 = sqlx::query_scalar(
        "SELECT jobid FROM cron.job WHERE database=current_database() AND jobname=$1",
    )
    .bind(ASSET_JOB_NAME)
    .fetch_one(&pool)
    .await
    .expect("应可核对任务 ID");
    assert_eq!(current_job_id, job_id, "修改周期不应重新创建任务");

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

    let source_with_public_ip: (String, String, String) = sqlx::query_as(
        "SELECT asset_id, source_ip, public_ip FROM dblink( \
           'dysec_asset_source', \
           'SELECT asset_id::text, btrim(ip)::text, btrim(public_ip)::text \
            FROM public.asset_subject \
            WHERE manage_asset_type IN (''HOST'', ''TERMINAL'') \
              AND NULLIF(btrim(ip), '''') IS NOT NULL \
              AND NULLIF(btrim(public_ip), '''') IS NOT NULL LIMIT 1' \
         ) AS source(asset_id text, source_ip text, public_ip text)",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("本地资产副本应包含同时有 ip 和 public_ip 的记录");
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT source_key, asset_id, ip \
         FROM asset_lookup.asset_subject_exclude_biz_sync \
         WHERE asset_id = $1 ORDER BY source_key",
    )
    .bind(&source_with_public_ip.0)
    .fetch_all(&mut *tx)
    .await
    .expect("读取同一资产的两个地址行");
    assert_eq!(rows.len(), 2);
    assert!(rows.contains(&(
        source_with_public_ip.0.clone(),
        source_with_public_ip.0.clone(),
        source_with_public_ip.1.clone(),
    )));
    assert!(rows.contains(&(
        format!("{}:public_ip", source_with_public_ip.0),
        source_with_public_ip.0.clone(),
        source_with_public_ip.2.clone(),
    )));
    let differing_metadata: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_lookup.asset_subject_exclude_biz_sync private_row \
         JOIN asset_lookup.asset_subject_exclude_biz_sync public_row \
           ON public_row.asset_id = private_row.asset_id \
          AND public_row.source_key = private_row.asset_id || ':public_ip' \
         WHERE private_row.source_key = private_row.asset_id \
           AND private_row.asset_id = $1 \
           AND (private_row.tenant_id, private_row.asset_type, private_row.asset_type_name, \
                private_row.asset_name, private_row.company_id, private_row.company_name, \
                private_row.system_id, private_row.system_name) IS DISTINCT FROM \
               (public_row.tenant_id, public_row.asset_type, public_row.asset_type_name, \
                public_row.asset_name, public_row.company_id, public_row.company_name, \
                public_row.system_id, public_row.system_name)",
    )
    .bind(&source_with_public_ip.0)
    .fetch_one(&mut *tx)
    .await
    .expect("比较两条地址行的资产属性");
    assert_eq!(differing_metadata, 0);

    let source_public_count: i64 = sqlx::query_scalar(
        "SELECT total FROM dblink( \
           'dysec_asset_source', \
           'SELECT count(*)::bigint FROM public.asset_subject \
            WHERE manage_asset_type IN (''HOST'', ''TERMINAL'') \
              AND NULLIF(btrim(public_ip), '''') IS NOT NULL' \
         ) AS source(total bigint)",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("统计本地源库公网 IP 行数");
    let target_public_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM asset_lookup.asset_subject_exclude_biz_sync \
         WHERE source_key = asset_id || ':public_ip'",
    )
    .fetch_one(&mut *tx)
    .await
    .expect("统计同步后的公网 IP 行数");
    assert_eq!(target_public_count, source_public_count);

    tx.rollback().await.expect("回滚验证事务");
}
