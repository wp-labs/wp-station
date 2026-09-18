use sea_orm::Schema;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(manager.get_database_backend());
        if !manager.has_table("release_restore_jobs").await? {
            manager
                .create_table(schema.create_table_from_entity(
                    crate::entity::release_restore_job::Entity,
                ))
                .await?;
        }

        if !manager
            .has_index("release_restore_jobs", "idx_restore_jobs_source_created")
            .await?
        {
            manager
                .create_index(
                Index::create()
                    .name("idx_restore_jobs_source_created")
                    .table(crate::entity::release_restore_job::Entity)
                    .col(crate::entity::release_restore_job::Column::SourceReleaseId)
                    .col(crate::entity::release_restore_job::Column::CreatedAt)
                    .to_owned(),
                )
                .await?;
        }
        if !manager
            .has_index("release_restore_jobs", "idx_restore_jobs_status_phase")
            .await?
        {
            manager
                .create_index(
                Index::create()
                    .name("idx_restore_jobs_status_phase")
                    .table(crate::entity::release_restore_job::Entity)
                    .col(crate::entity::release_restore_job::Column::Status)
                    .col(crate::entity::release_restore_job::Column::Phase)
                    .col(crate::entity::release_restore_job::Column::UpdatedAt)
                    .to_owned(),
                )
                .await?;
        }
        if !manager
            .has_index("release_restore_jobs", "idx_restore_jobs_system_version")
            .await?
        {
            manager
                .create_index(
                Index::create()
                    .name("idx_restore_jobs_system_version")
                    .table(crate::entity::release_restore_job::Entity)
                    .col(crate::entity::release_restore_job::Column::System)
                    .col(crate::entity::release_restore_job::Column::TargetVersion)
                    .unique()
                    .to_owned(),
                )
                .await?;
        }

        // 初始迁移使用当前 entity 建表；全新数据库可能已经包含这些列。
        // 仅在升级旧数据库时执行 ALTER，避免重复列错误。
        if !manager.has_column("release_targets", "attempt_no").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(crate::entity::release_target::Entity)
                        .add_column(
                            ColumnDef::new(crate::entity::release_target::Column::AttemptNo)
                                .integer()
                                .not_null()
                                .default(1),
                        )
                        .to_owned(),
                )
                .await?;
        }
        if !manager.has_column("release_targets", "operation").await? {
            manager
                .alter_table(
                    Table::alter()
                        .table(crate::entity::release_target::Entity)
                        .add_column(
                            ColumnDef::new(crate::entity::release_target::Column::Operation)
                                .string()
                                .not_null()
                                .default("publish"),
                        )
                        .to_owned(),
                )
                .await?;
        }
        if !manager
            .has_column("release_targets", "previous_group_version")
            .await?
        {
            manager
                .alter_table(
                    Table::alter()
                        .table(crate::entity::release_target::Entity)
                        .add_column(
                            ColumnDef::new(
                                crate::entity::release_target::Column::PreviousGroupVersion,
                            )
                            .string()
                            .null(),
                        )
                        .to_owned(),
                )
                .await?;
        }
        for (column_name, column) in [
            (
                "request_summary",
                crate::entity::release_target::Column::RequestSummary,
            ),
            (
                "response_status",
                crate::entity::release_target::Column::ResponseStatus,
            ),
            (
                "response_summary",
                crate::entity::release_target::Column::ResponseSummary,
            ),
        ] {
            if !manager.has_column("release_targets", column_name).await? {
                manager
                    .alter_table(
                        Table::alter()
                            .table(crate::entity::release_target::Entity)
                            .add_column(ColumnDef::new(column).text().null())
                            .to_owned(),
                    )
                    .await?;
            }
        }

        if !manager
            .has_index("release_targets", "idx_release_targets_attempt")
            .await?
        {
            manager
                .create_index(
                Index::create()
                    .name("idx_release_targets_attempt")
                    .table(crate::entity::release_target::Entity)
                    .col(crate::entity::release_target::Column::ReleaseId)
                    .col(crate::entity::release_target::Column::DeviceId)
                    .col(crate::entity::release_target::Column::ReleaseGroup)
                    .col(crate::entity::release_target::Column::Operation)
                    .col(crate::entity::release_target::Column::AttemptNo)
                    .unique()
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
                    .name("idx_release_targets_attempt")
                    .table(crate::entity::release_target::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::release_restore_job::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
