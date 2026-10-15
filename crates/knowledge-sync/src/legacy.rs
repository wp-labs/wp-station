//! 将旧 PostgreSQL 运行历史迁入 Station SQLite，再移除旧 schema。

use anyhow::{Context, Result};
use sqlx::{PgPool, Row, SqlitePool};

/// 幂等迁移旧运行记录；SQLite 提交成功后才删除 PostgreSQL 旧表。
pub async fn migrate_legacy_runs(pg_url: &str, station: &SqlitePool) -> Result<()> {
    let pg = PgPool::connect(pg_url)
        .await
        .context("连接旧知识同步记录库失败")?;
    let (schema_exists, geo_exists, intel_exists): (bool, bool, bool) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM pg_namespace WHERE nspname='knowledge_sync'), \
                to_regclass('knowledge_sync.geo_runs') IS NOT NULL, \
                to_regclass('knowledge_sync.intel_runs') IS NOT NULL",
    )
    .fetch_one(&pg)
    .await?;
    if geo_exists {
        let rows = sqlx::query(
            "SELECT id,status,stage,file_name,file_hash,database_type,build_epoch,row_count, \
             row_limit,progress_rows,progress_percent,error, \
             to_char(started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS started_at, \
             to_char(finished_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS finished_at \
             FROM knowledge_sync.geo_runs ORDER BY id",
        )
        .fetch_all(&pg)
        .await
        .context("读取旧 Geo 运行历史失败")?;
        let mut tx = station.begin().await?;
        for row in rows {
            sqlx::query(
                "INSERT OR IGNORE INTO geo_sync_runs \
                 (legacy_pg_run_id,status,stage,file_name,file_hash,database_type,build_epoch, \
                  row_count,row_limit,progress_rows,progress_percent,error,started_at,finished_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(row.try_get::<i64, _>("id")?)
            .bind(row.try_get::<String, _>("status")?)
            .bind(row.try_get::<String, _>("stage")?)
            .bind(row.try_get::<String, _>("file_name")?)
            .bind(row.try_get::<Option<String>, _>("file_hash")?)
            .bind(row.try_get::<Option<String>, _>("database_type")?)
            .bind(row.try_get::<Option<i64>, _>("build_epoch")?)
            .bind(row.try_get::<Option<i64>, _>("row_count")?)
            .bind(row.try_get::<Option<i64>, _>("row_limit")?)
            .bind(row.try_get::<i64, _>("progress_rows")?)
            .bind(row.try_get::<Option<i32>, _>("progress_percent")?)
            .bind(row.try_get::<Option<String>, _>("error")?)
            .bind(row.try_get::<String, _>("started_at")?)
            .bind(row.try_get::<Option<String>, _>("finished_at")?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.context("保存 Geo 旧历史到 SQLite 失败")?;
        sqlx::query("DROP TABLE knowledge_sync.geo_runs")
            .execute(&pg)
            .await
            .context("清理旧 Geo 运行表失败")?;
    }
    if intel_exists {
        let rows = sqlx::query(
            "SELECT id,status,stage,read_count,invalid_count,expired_count,ip_count,domain_count,error, \
             to_char(started_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS started_at, \
             to_char(finished_at AT TIME ZONE 'UTC','YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"') AS finished_at \
             FROM knowledge_sync.intel_runs ORDER BY id",
        )
        .fetch_all(&pg)
        .await
        .context("读取旧情报运行历史失败")?;
        let mut tx = station.begin().await?;
        for row in rows {
            sqlx::query(
                "INSERT OR IGNORE INTO intel_sync_runs \
                 (legacy_pg_run_id,status,stage,read_count,invalid_count,expired_count, \
                  ip_count,domain_count,error,started_at,finished_at) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?)",
            )
            .bind(row.try_get::<i64, _>("id")?)
            .bind(row.try_get::<String, _>("status")?)
            .bind(row.try_get::<String, _>("stage")?)
            .bind(row.try_get::<i64, _>("read_count")?)
            .bind(row.try_get::<i64, _>("invalid_count")?)
            .bind(row.try_get::<i64, _>("expired_count")?)
            .bind(row.try_get::<i64, _>("ip_count")?)
            .bind(row.try_get::<i64, _>("domain_count")?)
            .bind(row.try_get::<Option<String>, _>("error")?)
            .bind(row.try_get::<String, _>("started_at")?)
            .bind(row.try_get::<Option<String>, _>("finished_at")?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await.context("保存情报旧历史到 SQLite 失败")?;
        sqlx::query("DROP TABLE knowledge_sync.intel_runs")
            .execute(&pg)
            .await
            .context("清理旧情报运行表失败")?;
    }
    if schema_exists {
        sqlx::query("DROP SCHEMA IF EXISTS knowledge_sync")
            .execute(&pg)
            .await
            .context("清理旧知识同步 schema 失败")?;
    }
    Ok(())
}
