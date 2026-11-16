use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE IF NOT EXISTS agent_sync_runs (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT,\
                 status TEXT NOT NULL,\
                 read_count INTEGER NOT NULL DEFAULT 0,\
                 agent_count INTEGER NOT NULL DEFAULT 0,\
                 error TEXT,\
                 started_at TEXT NOT NULL,\
                 finished_at TEXT)",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE UNIQUE INDEX IF NOT EXISTS idx_agent_sync_one_running \
                 ON agent_sync_runs(status) WHERE status='running'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE IF EXISTS agent_sync_runs")
            .await?;
        Ok(())
    }
}
