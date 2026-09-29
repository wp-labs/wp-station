use sea_orm::sea_query::Expr;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        if !manager
            .has_index("release_restore_jobs", "idx_restore_jobs_active_system")
            .await?
        {
            manager
                .create_index(
                    Index::create()
                        .name("idx_restore_jobs_active_system")
                        .table(crate::entity::release_restore_job::Entity)
                        .col(crate::entity::release_restore_job::Column::System)
                        .unique()
                        .and_where(
                            Expr::col((
                                crate::entity::release_restore_job::Entity,
                                crate::entity::release_restore_job::Column::Status,
                            ))
                            .is_in(["QUEUED", "RUNNING"]),
                        )
                        .to_owned(),
                )
                .await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_restore_jobs_active_system")
                    .table(crate::entity::release_restore_job::Entity)
                    .to_owned(),
            )
            .await
    }
}
