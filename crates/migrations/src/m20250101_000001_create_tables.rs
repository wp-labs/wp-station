use sea_orm::{ConnectionTrait, DbBackend, Schema, Statement};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let schema = Schema::new(DbBackend::Postgres);

        // 创建 devices 表（机器管理）
        let stmt = schema.create_table_from_entity(crate::entity::device::Entity);
        manager.create_table(stmt).await?;

        // 创建 releases 表
        let stmt = schema.create_table_from_entity(crate::entity::release::Entity);
        manager.create_table(stmt).await?;

        // 创建 release_targets 表
        let stmt = schema.create_table_from_entity(crate::entity::release_target::Entity);
        manager.create_table(stmt).await?;

        // 创建 performance_tasks 表
        let stmt = schema.create_table_from_entity(crate::entity::performance::Entity);
        manager.create_table(stmt).await?;

        // 创建 performance_results 表
        let stmt = schema.create_table_from_entity(crate::entity::performance::result::Entity);
        manager.create_table(stmt).await?;

        // 创建 user 表
        let stmt = schema.create_table_from_entity(crate::entity::user::Entity);
        manager.create_table(stmt).await?;

        // 创建 operation_log 表
        let stmt = schema.create_table_from_entity(crate::entity::operation_log::Entity);
        manager.create_table(stmt).await?;

        // 创建 assist_tasks 表
        let stmt = schema.create_table_from_entity(crate::entity::assist_task::Entity);
        manager.create_table(stmt).await?;

        // 创建 sandbox_runs 表
        let stmt = schema.create_table_from_entity(crate::entity::sandbox_run::Entity);
        manager.create_table(stmt).await?;

        // 插入初始 admin 用户
        manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                DbBackend::Postgres,
                r#"INSERT INTO "user" (username, password, display_name, email, role, status, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())"#,
                [
                    "admin".into(),
                    "$2b$12$es3GK5p3xP0dRV6k2AIB8.1JDH/TLzZtzE6iI9Hep1DQsJgI04f22".into(),
                    "管理员".into(),
                    "admin@xx.com".into(),
                    "admin".into(),
                    "active".into(),
                ],
            ))
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 按相反顺序删表，先删依赖表
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::assist_task::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::sandbox_run::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::operation_log::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::user::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::performance::result::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::performance::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::release_target::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::release::Entity)
                    .to_owned(),
            )
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(crate::entity::device::Entity)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
