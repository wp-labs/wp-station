use chrono::Utc;
use sea_orm::{ActiveModelTrait, Schema, Set};
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        let schema = Schema::new(backend);

        // 创建 devices 表（机器管理）
        let stmt = schema.create_table_from_entity(crate::entity::device::Entity);
        manager.create_table(stmt).await?;

        // 创建 releases 表
        let stmt = schema.create_table_from_entity(crate::entity::release::Entity);
        manager.create_table(stmt).await?;

        // 创建 release_targets 表
        let stmt = schema.create_table_from_entity(crate::entity::release_target::Entity);
        manager.create_table(stmt).await?;

        // 创建发布分组查询索引，支撑按分组查找最新发布记录。
        manager
            .create_index(
                Index::create()
                    .name("idx_releases_release_group")
                    .table(crate::entity::release::Entity)
                    .col(crate::entity::release::Column::ReleaseGroup)
                    .col(crate::entity::release::Column::Status)
                    .col(crate::entity::release::Column::PublishedAt)
                    .to_owned(),
            )
            .await?;

        // 创建设备上一成功发布查询索引，支撑回滚和版本比对。
        manager
            .create_index(
                Index::create()
                    .name("idx_release_targets_prev_success")
                    .table(crate::entity::release_target::Entity)
                    .col(crate::entity::release_target::Column::DeviceId)
                    .col(crate::entity::release_target::Column::ReleaseGroup)
                    .col(crate::entity::release_target::Column::Status)
                    .col(crate::entity::release_target::Column::CompletedAt)
                    .to_owned(),
            )
            .await?;

        // 创建 user 表
        let stmt = schema.create_table_from_entity(crate::entity::user::Entity);
        manager.create_table(stmt).await?;

        // 创建 assist_tasks 表
        let stmt = schema.create_table_from_entity(crate::entity::assist_task::Entity);
        manager.create_table(stmt).await?;

        // 创建 sandbox_runs 表
        let stmt = schema.create_table_from_entity(crate::entity::sandbox_run::Entity);
        manager.create_table(stmt).await?;

        // 插入初始 admin 用户
        let now = Utc::now();
        crate::entity::user::ActiveModel {
            username: Set("admin".to_string()),
            password: Set(
                "$2b$12$es3GK5p3xP0dRV6k2AIB8.1JDH/TLzZtzE6iI9Hep1DQsJgI04f22".to_string(),
            ),
            display_name: Set(Some("管理员".to_string())),
            email: Set(Some("admin@xx.com".to_string())),
            role: Set("admin".to_string()),
            status: Set("active".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(manager.get_connection())
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
                    .table(crate::entity::user::Entity)
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
