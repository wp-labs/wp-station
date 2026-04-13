# 背景

文件名：2026-04-13_1
创建于：2026-04-13
创建者：wensiwei
主分支：main
任务分支：task/rule-knowledge-file-only_2026-04-13_1
Yolo 模式：Off

---

# 任务描述

将 `rule_config` 和 `knowledge_config` 两张数据库表的职责替换为直接操作 `project_root` 目录下的文件。

约束条件：
- 单用户操作，无并发写冲突问题
- 不需要版本追踪（去掉 `version` 字段）
- 不需要软删除（去掉 `is_active`，删除即物理删除）
- 保证 `project_root` 里的数据始终是用户最新编辑结果
- 每次保存仍然推送到 Gitea

---

# 现状分析

## 当前数据流

```
写操作：API → server/rules.rs → db/rule_config.rs → SQLite
                                                        ↓
                                        export_project_from_db（全量导出所有规则）
                                                        ↓
                                                  sync_to_gitea

读操作：API → server/rules.rs → db/rule_config.rs → SQLite
```

数据库是 source of truth，文件是每次写后生成的产物。

## 目标数据流

```
写操作：API → server/rules.rs → 直接写文件到 project_root
                                        ↓
                                  sync_to_gitea

读操作：API → server/rules.rs → 直接读 project_root 文件
```

文件是 source of truth，消除数据库这一中间层。

---

# 文件结构映射

## rule_config

| RuleType | DB file_name | project_root 路径 |
|----------|-------------|-----------------|
| Parse | `wparse.toml` | `conf/wparse.toml` |
| Wpgen | `wpgen.toml` | `conf/wpgen.toml` |
| SourceConnect | `xxx.toml` | `connectors/source.d/xxx.toml` |
| SinkConnect | `xxx.toml` | `connectors/sink.d/xxx.toml` |
| Source | `subpath/file` | `topology/sources/subpath/file` |
| Sink | `subpath/file` | `topology/sinks/subpath/file` |
| Wpl | `name` | `models/wpl/<name>/parse.wpl`（content）<br>`models/wpl/<name>/sample.dat`（sample_content） |
| Oml | `name` | `models/oml/<name>/adm.oml` |

映射逻辑已在 `src/utils/project.rs` 中的 `rule_target_path` / `wpl_rule_paths` 函数实现，直接复用。

## knowledge_config

| DB 字段 | project_root 路径 |
|---------|-----------------|
| `file_name`（表名） | 目录名 `models/knowledge/<file_name>/` |
| `create_sql` | `models/knowledge/<file_name>/create.sql` |
| `insert_sql` | `models/knowledge/<file_name>/insert.sql` |
| `data_content` | `models/knowledge/<file_name>/data.csv` |
| `config_content`（所有记录同值，是全局配置） | `models/knowledge/knowdb.toml`（独立文件） |

**重要设计变化**：当前 DB 将 `config_content`（knowdb.toml 内容）**复制到每一条** knowledge 记录中，`update_knowdb_config` 会更新所有行。文件方案将其改为独立的 `knowdb.toml` 文件，不再与每个表挂钩，消除这个冗余。

---

# 特殊情况处理

## WPL 虚拟文件路径

接口使用 `<name>/parse.wpl` 和 `<name>/sample.dat` 作为虚拟路径标识子文件。
`split_wpl_virtual_file` / `format_wpl_virtual_file` 这两个函数逻辑不变，只是底层从 DB 查询改为读写文件。

## display_name（Sink）

DB 的 `display_name` 字段仅对 Sink 类型有意义，现已通过 `fallback_sink_display` 函数（`src/utils/constants.rs`）按文件名硬编码映射。文件方案直接保持此逻辑，不再需要 DB 字段。

## last_modified

当前从 DB `updated_at` 字段返回。文件方案改为读文件 mtime（`std::fs::metadata().modified()`），不可靠场景（文件被 rsync/复制覆盖时 mtime 变动）在当前单用户本地操作中不构成问题。

## 初始化（默认配置加载）

当前 `init_default_configs_from_embedded` 将内嵌 `default_configs/` 目录写入 DB，再由 `export_project_from_db` 导出到 `project_root`。

文件方案直接将内嵌 `default_configs/` 内容复制到 `project_root`（仅当对应文件不存在时，保持幂等），跳过 DB 环节。

---

# 变更范围

## 需修改的文件

### `src/server/rules.rs`（主要业务逻辑层，改动最大）

| 函数 | 当前 | 改后 |
|------|------|------|
| `get_rule_files_logic` | `get_rule_file_names` → DB 查询 | 目录扫描（复用 `load_project_snapshot` 逻辑） |
| `get_rule_content_logic` | `find_rule_by_type_and_name` → DB | 读文件 + 文件 mtime 作为 `last_modified` |
| `create_rule_file_logic` | `create_rule_config` → DB insert | `touch_rule_in_project` 创建空文件 |
| `delete_rule_file_logic` | DB 软删除 → `sync_delete_to_gitea` | 物理删除文件 → `sync_delete_to_gitea` |
| `save_rule_logic` | DB upsert → `export_project_from_db`（全量）→ sync | 直接写单文件 → sync（去掉全量导出） |
| `save_knowledge_rule_logic` | DB upsert → `export_project_from_db` → sync | 直接写文件到 `models/knowledge/<name>/` → sync |
| `get_knowdb_config_logic` | `get_knowdb_config_entry` → DB | 读 `models/knowledge/knowdb.toml` |
| `save_knowdb_config_logic` | `update_knowdb_config`（更新所有行）→ export → sync | 直接写 `models/knowledge/knowdb.toml` → sync |

### `src/db/default_rules_loader.rs`

将 `init_default_configs_from_embedded` 改为：
- 遍历内嵌 `default_configs/` 目录
- 按照文件路径规则，将文件写入 `project_root` 对应位置
- 幂等：仅当目标文件不存在时才写入（不覆盖用户已编辑的内容）

### `src/server/app.rs`（初始化调用入口）

将原 `init_default_configs_from_embedded(db)` 的调用改为新的 `init_default_configs_to_project(project_root)` 函数。

### `src/db/rule_config.rs` 和 `src/db/knowledge_config.rs`

这两个文件及 `src/db/mod.rs` 中对应的导出可以在改完 `server/rules.rs` 后逐步删除。
保留其他 DB 模块（`device`、`release`、`user`、`operation_log`、`sandbox` 等）不变。

### `src/utils/project.rs`

- `export_project_from_db` 可废弃（不再需要全量导出）
- `touch_rule_in_project`、`delete_rule_from_project`、`delete_knowledge_from_project` 保留复用
- 新增辅助函数：
  - `read_rule_content(project_root, rule_type, file_name) -> Option<(String, SystemTime)>`：读文件内容 + mtime
  - `list_rule_files(project_root, rule_type) -> Vec<String>`：目录扫描返回文件名列表
  - `list_knowledge_dirs(project_root) -> Vec<String>`：扫描 `models/knowledge/` 子目录列表
  - `write_knowledge_files(project_root, file_name, create_sql, insert_sql, data_content)`：写 knowledge 文件
  - `read_knowledge_files(project_root, file_name) -> KnowledgeFiles`：读 knowledge 文件

## 不变的文件

| 文件 | 原因 |
|------|------|
| `src/api/rules.rs` | HTTP 层无变化 |
| `src/server/sync.rs` | Gitea 同步逻辑不变 |
| `src/server/setting.rs` | 设置读取不变 |
| `src/server/operation_log.rs` | 操作日志仍写 DB |
| `src/utils/knowledge.rs` | 加载知识库读文件，已是文件操作 |
| `src/utils/check.rs` | 规则校验读 project_root 文件 |
| 其他 `src/db/*` | 其他业务表不受影响 |

---

# 保存链路变化对比

## 当前链路（以 `save_rule_logic` 为例）

```
1. find_rule_by_type_and_name → DB 查询是否存在
2. update_rule_content / create_rule_config → DB upsert
3. export_project_from_db → 全量读取 DB 所有规则 → 写所有文件
4. sync_to_gitea → git commit + push
5. handle_draft_release → 更新草稿发布
6. write_operation_log_for_result → 写操作日志
```

## 目标链路

```
1. 直接写文件到 project_root（单文件，无全量导出）
2. sync_to_gitea → git commit + push
3. handle_draft_release → 更新草稿发布
4. write_operation_log_for_result → 写操作日志
```

消除了步骤 1（DB 查询）、步骤 2（DB 写入）、步骤 3（全量文件导出）。

---

# knowledge 特殊链路变化

## `save_knowdb_config_logic`（全局 knowdb.toml）

当前：`update_knowdb_config` 将相同内容写入所有知识库记录 → export → sync

目标：直接写 `models/knowledge/knowdb.toml` → sync

## `save_knowledge_rule_logic`（单个表的 SQL/数据文件）

当前：DB upsert → `export_project_from_db` 全量导出 → sync

目标：写 `models/knowledge/<name>/create.sql` 等文件 → sync

## `get_knowdb_config_logic`

当前：查 DB 找任意一条有 `config_content` 的记录

目标：读 `models/knowledge/knowdb.toml` 文件，不存在则返回 `content: None`

## `get_rule_content_logic`（knowledge 分支）

当前：`find_knowledge_config_by_file_name` → DB

目标：从 `models/knowledge/<name>/` 读取 create.sql / insert.sql / data.csv

---

# 数据库删表方案

## 迁移背景

项目目前只有一个迁移文件：`crates/migrations/src/m20250101_000001_create_tables.rs`，所有表在同一个 `up()` 中创建。删除 `rule_configs` 和 `knowledge_configs` 需要新增一个迁移。

## 新增迁移文件

新建 `crates/migrations/src/m20260413_000002_drop_rule_knowledge_tables.rs`：

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("rule_configs")).if_exists().to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Alias::new("knowledge_configs")).if_exists().to_owned())
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // down 不需要重建这两张表，回滚只需保证迁移框架不报错
        // 若需真正回滚，需手动执行建表语句
        Ok(())
    }
}
```

## 注册迁移

`crates/migrations/src/lib.rs` 添加新迁移：

```rust
mod m20260413_000002_drop_rule_knowledge_tables;

impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20250101_000001_create_tables::Migration),
            Box::new(m20260413_000002_drop_rule_knowledge_tables::Migration),
        ]
    }
}
```

## 删除 entity 文件

迁移执行后，对应 entity 文件也一并删除：

- `crates/migrations/src/entity/rule_config.rs` → 删除
- `crates/migrations/src/entity/knowledge_config.rs` → 删除
- `crates/migrations/src/entity/mod.rs` → 移除这两个 `pub mod` 声明及 pub use

## 执行时机

迁移在应用**启动时自动执行**（`src/server/app.rs` 中的 `Migrator::up` 调用），无需手动操作。先完成业务代码改造并验证通过，再合并迁移文件，确保新代码不再依赖这两张表后才执行删表。

---

# 实施清单

1. `src/utils/project.rs`：新增 `read_rule_content`、`list_rule_files`、`list_knowledge_dirs`、`write_knowledge_files`、`read_knowledge_files` 辅助函数
2. `src/server/rules.rs`：改写 `get_rule_files_logic`，改用目录扫描
3. `src/server/rules.rs`：改写 `get_rule_content_logic`，改用文件读取（包含 knowledge 分支）
4. `src/server/rules.rs`：改写 `create_rule_file_logic`，改用 `touch_rule_in_project`
5. `src/server/rules.rs`：改写 `delete_rule_file_logic`，改用物理删除文件
6. `src/server/rules.rs`：改写 `save_rule_logic`，去掉 DB 操作和全量 export，直接写单文件
7. `src/server/rules.rs`：改写 `save_knowledge_rule_logic`，直接写 knowledge 文件
8. `src/server/rules.rs`：改写 `get_knowdb_config_logic`，读 `knowdb.toml` 文件
9. `src/server/rules.rs`：改写 `save_knowdb_config_logic`，写 `knowdb.toml` 文件
10. `src/db/default_rules_loader.rs`：改写为直接将内嵌 `default_configs/` 写入 `project_root`
11. `src/server/app.rs`：更新初始化调用，传入 `project_root` 而非 DB 连接
12. `src/db/rule_config.rs`：删除文件
13. `src/db/knowledge_config.rs`：删除文件
14. `src/db/mod.rs`：清理对应导出
15. 新建 `crates/migrations/src/m20260413_000002_drop_rule_knowledge_tables.rs`（删表迁移）
16. `crates/migrations/src/lib.rs`：注册新迁移
17. `crates/migrations/src/entity/rule_config.rs`：删除文件
18. `crates/migrations/src/entity/knowledge_config.rs`：删除文件
19. `crates/migrations/src/entity/mod.rs`：移除这两个模块声明
20. `AGENTS.md`：更新快查表（数据层描述）和核心流程描述

---

# 任务进度

（执行后追加）

---

# 最终审查

（完成后填写）
