use sqlx::Connection;
use wp_station_knowledge_sync::geo::{GeoSyncConfig, GeoSyncService};
#[allow(dead_code)]
mod common;

#[tokio::test]
async fn imports_mmdb_into_local_postgres() {
    let Ok(Some(config)) = GeoSyncConfig::from_env() else {
        return;
    };
    let Ok(file_name) = std::env::var("WP_KSYNC_GEO_TEST_FILE") else {
        return;
    };
    let (_dir, store) = common::knowledge_sync_store().await;
    let target = config.target_database_url.clone();
    let service = GeoSyncService::connect(config, store.clone())
        .await
        .expect("Geo 目标库应可连接");
    let before = service.initialize().await.expect("Geo 初始化应成功");
    assert!(before.table_exists, "启动时应创建正式表");
    assert!(before.index_ready, "启动时应创建查询索引");
    let run = service
        .start_import(&file_name)
        .await
        .expect("应可创建 Geo 导入任务");
    assert_eq!(
        run.file_hash.as_ref().map(String::len),
        Some(64),
        "点击后应记录 SHA-256"
    );
    let import_service = service.clone();
    let import_run = run.clone();
    let task = tokio::spawn(async move { import_service.execute_import(import_run).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let current = service
                .run(run.id)
                .await
                .expect("读取导入阶段")
                .expect("导入记录存在");
            if current.status == "running" && current.stage != "queued" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("导入任务应进入执行阶段");
    let responsive =
        tokio::time::timeout(std::time::Duration::from_secs(3), service.status()).await;
    assert!(responsive.is_ok(), "导入期间 Geo 状态查询不应阻塞其他请求");
    task.await.expect("Geo 后台任务应正常结束");
    let result = service
        .run(run.id)
        .await
        .expect("应可读取导入记录")
        .expect("导入记录应存在");
    assert_eq!(result.status, "succeeded", "{:?}", result.error);
    assert_eq!(result.progress_percent, Some(100));
    assert_eq!(result.file_hash.as_ref().map(String::len), Some(64));
    assert_eq!(result.ipv6_source_row_count, Some(0));
    let pool = sqlx::PgPool::connect(&target)
        .await
        .expect("连接 Geo 测试库");
    let ipv6_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM public.ip_geo WHERE ip_version=6")
            .fetch_one(&pool)
            .await
            .expect("统计 IPv6 行数");
    assert_eq!(ipv6_count, 0, "Geo 应只导入 IPv4");
    let after = service.status().await.expect("应可读取 Geo 状态");
    assert!(after.table_exists);
    assert_eq!(after.current_row_count, result.row_count);
    // 模拟 PostgreSQL 已提交、SQLite 成功摘要未写入的跨库故障。
    sqlx::query("UPDATE geo_sync_runs SET status='failed', stage='failed', error='模拟状态写入失败' WHERE id=$1")
        .bind(run.id)
        .execute(&store)
        .await
        .expect("模拟运行记录异常");
    let recovered = service.initialize().await.expect("启动时应核对已提交数据");
    assert_eq!(
        recovered.latest_run.as_ref().map(|run| run.status.as_str()),
        Some("succeeded")
    );
    assert!(
        service.start_import(&file_name).await.is_err(),
        "同版本或同 SHA-256 文件应拒绝再次导入"
    );
    if let Ok(next_file) = std::env::var("WP_KSYNC_GEO_TEST_NEXT_FILE") {
        sqlx::query("GRANT SELECT ON public.ip_geo TO PUBLIC")
            .execute(&pool)
            .await
            .expect("授权测试只读角色");
        let mut persistent = sqlx::PgConnection::connect(&target)
            .await
            .expect("创建保持连接的业务查询会话");
        let cached_sql = "SELECT count(*) FROM public.ip_geo WHERE ip_version=4";
        let cached_before: i64 = sqlx::query_scalar(cached_sql)
            .fetch_one(&mut persistent)
            .await
            .expect("切换前执行预编译查询");
        let mut reader = pool.begin().await.expect("开启旧版读取事务");
        let old_count: i64 = sqlx::query_scalar("SELECT count(*) FROM public.ip_geo")
            .fetch_one(&mut *reader)
            .await
            .expect("持有旧版读锁");
        let next = service
            .start_import(&next_file)
            .await
            .expect("不同版本应允许创建导入任务");
        assert_eq!(next.file_hash.as_ref().map(String::len), Some(64));
        let import_service = service.clone();
        let next_for_task = next.clone();
        let task = tokio::spawn(async move { import_service.execute_import(next_for_task).await });
        tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                if service.run(next.id).await.unwrap().unwrap().stage == "switching" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("新表应完成准备并等待切换");
        let visible: i64 = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::query_scalar("SELECT count(*) FROM public.ip_geo").fetch_one(&pool),
        )
        .await
        .expect("切换等待时读请求不应阻塞")
        .expect("旧版数据仍可查询");
        assert_eq!(visible, old_count);
        reader.commit().await.expect("释放旧版读锁");
        task.await.expect("导入任务结束");
        let next_result = service.run(next.id).await.unwrap().unwrap();
        assert_eq!(next_result.status, "succeeded", "{:?}", next_result.error);
        assert_ne!(result.file_hash, next_result.file_hash);
        assert_eq!(
            service.status().await.unwrap().current_row_count,
            next_result.row_count
        );
        let cached_after: i64 = sqlx::query_scalar(cached_sql)
            .fetch_one(&mut persistent)
            .await
            .expect("切换后复用业务查询会话");
        assert_eq!(cached_before, old_count);
        assert_eq!(cached_after, next_result.row_count.unwrap());
        let index_ready: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE schemaname='public' AND tablename='ip_geo' AND indexname='ip_geo_num_lookup_gist_idx')",
        ).fetch_one(&pool).await.unwrap();
        assert!(index_ready, "切换后查询索引应保留标准名称");
        let public_can_read: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_class c, LATERAL aclexplode(c.relacl) acl \
             WHERE c.oid='public.ip_geo'::regclass AND acl.grantee=0 AND acl.privilege_type='SELECT')",
        ).fetch_one(&pool).await.unwrap();
        assert!(public_can_read, "切换后应保留只读授权");
        let leftovers: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM pg_class WHERE relname LIKE 'ip_geo_stage_%' OR relname LIKE 'ip_geo_old_%'",
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(leftovers, 0, "完成后不应遗留临时表");
    }
}
