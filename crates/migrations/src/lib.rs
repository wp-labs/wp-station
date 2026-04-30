// WarpStation 数据库迁移和实体定义

pub use sea_orm_migration::prelude::*;

pub mod entity;
mod m20250101_000001_create_tables;
mod m20260428_000002_add_release_group;

pub use entity::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250101_000001_create_tables::Migration),
            Box::new(m20260428_000002_add_release_group::Migration),
        ]
    }
}
