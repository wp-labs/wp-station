# wp-station

`wp-station` 是 WParse / WFusion 的配置、调试与发布控制台，也负责把外部数据同步到运行时使用的知识库。前端通过 `/api` 调用 Rust 后端；规则和配置保存在 `project_models`、`project_infra` 两个仓库，设备、发布和任务运行记录保存在 Station 数据库。

变更记录见 [CHANGELOG.md](./CHANGELOG.md)，开发入口与联动约束见 [AGENTS.md](./AGENTS.md)。

## 主要能力

- **设备管理**：维护设备 IP、端口、Token、在线状态、客户端版本、配置版本，支持一键刷新单台设备状态。
- **规则管理**：维护 `wpl`、`oml`、`knowledge` 等规则文件，支持格式化、校验、保存。
- **配置管理**：维护 `parse`、`source`、`sink`、`source_connect`、`sink_connect` 配置；`来源配置` 与 `业务 sink 配置` 支持按 connector 模板实时追加片段。
- **知识库**：维护知识库配置、建表 SQL、插入 SQL、样例数据，支持 SQL 查询。
- **知识库同步**：定时同步资产、手动导入 Geo MMDB、定时同步 Doris 情报到 Redis，查看状态与历史。
- **调试**：日志解析、知识库查询、WPL/OML 格式化、性能任务占位。
- **接入概览**：汇总规则覆盖的设备类型 / 日志类型，以及运行时已启用的输入源 / 业务输出源。
- **发布**：按设备维度驱动发布、轮询状态、失败重试、人工回滚；支持填写发布备注。
- **沙盒预发布验证**：在真实样例数据上模拟执行，展示阶段日志、诊断建议、历史记录，通过后才允许正式发布。
- **连接管理**：维护 wparse / wfusion 设备连接和运行状态。
- **项目导入导出**：支持目录导入、压缩包预检导入、确认导入和项目归档导出。
- **AI 辅助**：提交 AI/人工辅助任务，前端全局轮询并回填建议。

## 数据与运行架构

运行中的 `wp-station` 有以下核心角色：

| 角色 | 说明 |
|------|------|
| Rust 后端 | 提供 API、持久化、任务调度、规则校验、设备健康检查 |
| React 前端 | 控制台 UI，通过 `/api` 调后端 |
| Station SQLite / PostgreSQL | 承载设备、发布、用户及任务运行记录 |
| `project_models` | `wpl` / `oml` / `knowledge` 等 model 侧文件主数据源 |
| `project_infra` | `conf` / `connectors` / `topology` 等 infra 侧文件主数据源 |
| Gitea / WarpParse 设备 | 分别承接配置版本化和远端配置加载 |
| 目标 PostgreSQL / Redis | 分别承接资产、Geo 查询数据和 IP / 域名情报集合 |

> 当前运行时采用双仓库布局：`project_models` 保存 `models/*`，`project_infra` 保存 `conf`、`connectors`、`topology`。`default_configs/` 只补齐缺失文件，不覆盖用户已编辑内容。

业务配置保存与知识库数据同步是两条独立链路：前者进入仓库和发布流程，后者更新 PostgreSQL 或 Redis 中的查询数据。

## 前置依赖

**必需**

- Rust
- Node.js
- PostgreSQL 或 SQLite

**按功能需要**

- Gitea：首次初始化本地仓库、配置同步、发布差异查看
- WarpParse 设备：健康检查、发布、回滚
- Assist 服务：AI/人工辅助任务

## 本地启动

### 1. 配置后端

Station 自身配置位于 `config/config.toml`。本地最小示例：

```toml
[web]
host = "0.0.0.0"
port = 8081

project_models = "./project_models"
project_infra = "./project_infra"

[database]
url = "sqlite://db/station.db"

[gitea]
base_url = "http://localhost:3000"
username = "gitea"
password = "<本地 Gitea 密码>"
```

`project_models` 和 `project_infra` 默认位于当前工作目录。常规配置支持环境变量覆盖，例如 `WP_STATION__WEB__PORT=8082`。知识库同步的数据库、Redis、Doris 地址及凭据只由部署环境变量注入，不写入 `config/config.toml`，也不在页面编辑。

设备连接 Token 使用 AES-256-GCM 加密后保存在 Station 数据库。默认使用程序内置的固定密钥，启动无需配置密钥；可选用 `WP_STATION_DEVICE_TOKEN_KEY` 覆盖，值为 **32 字节随机密钥的 Base64 编码**（可用 `openssl rand -base64 32` 生成一次并保存）。首次启动会在一个事务内把已有明文 Token 原位加密，以后启动会验证已有密文。**已加密数据必须继续使用原密钥**：曾用环境密钥加密的库，移除环境变量后无法用内置密钥解密，Station 会停止启动而不会改写密文。内置密钥随程序分发，只用于避免数据库直接存放明文；如需防止持有程序文件的人解密数据库，应使用独立的部署密钥。设备列表与刷新接口只返回 `has_token` 状态，编辑连接时 Token 留空表示沿用原值。

本机同时验证资产、Geo 和情报同步时，可在项目根目录执行以下示例。先启动本地 PostgreSQL、Doris 和 Redis，并在 PostgreSQL 中准备 `dysec-asset.public.asset_subject` 的本地副本、`pg_cron`、`dblink` 和 `btree_gist`；将待导入的 MMDB 放进 `mmdb/`。示例密码仅适用于本地测试。

```bash
export WP_KSYNC_TARGET_URL='postgres://postgres:123456@127.0.0.1:5432/postgres'
export WP_KSYNC_ASSET_SOURCE_DATABASE='dysec-asset'
export WP_KSYNC_ASSET_CRON='*/5 * * * *'
export WP_KSYNC_MMDB_DIR='mmdb/'
export WP_KSYNC_DORIS_HOST='127.0.0.1'
export WP_KSYNC_REDIS_URL='redis://:123456@127.0.0.1:6379/0'
cargo run
```

资产源库与目标库共用 PostgreSQL 主机、端口和账号密码，只通过 `WP_KSYNC_ASSET_SOURCE_DATABASE` 指定不同的源库；Geo 使用同一个目标库。Doris 默认使用 `9030` 端口和 `root` 用户，数据库固定为 `intel`；情报同步默认每 300 秒执行，写入本地 Redis 0 号库。生产容器默认读取 `/app/mmdb`，无需设置 `WP_KSYNC_MMDB_DIR`；本机 `cargo run` 才设置为 `mmdb/`。资产在启动时只创建同步对象和 `pg_cron` 任务，到下一个周期才执行；情报任务同样等待下一个整周期，Geo 需在页面选择文件后手动导入。实际部署请注入自己的凭据，不要把密码写进版本库。

### 2. 启动后端

```bash
cargo run
```

启动时自动初始化连接池和数据库迁移。非 release 构建下，`build.rs` 会自动执行前端 `npm install` 和 `npm run build`，之后可直接访问站点。

### 3. 单独运行前端开发服务（可选）

```bash
cd web
npm install
npm run dev
```

前端开发走 Vite 代理，默认代理到 `http://localhost:8081`。

部署配置、脚本和编排文件维护在独立项目：[cust-wp-deploy](https://cnb.cool/dy-sec/core-engine/data-parse/cust-wp-deploy)。

## 知识库同步

顶部的 **知识库同步** 菜单（`/knowledge-sync`）展示任务状态、数量、周期、异常和历史。连接信息由服务端读取；页面提供 MMDB 文件选择与导入操作，资产和情报历史弹窗可确认后清空执行记录。Geo 导入历史不提供清空操作。

| 数据 | 触发方式 | 查询数据 | 历史记录 |
| --- | --- | --- | --- |
| 资产 | PostgreSQL `pg_cron` 按配置周期执行 | `asset_lookup.asset_subject_exclude_biz_sync`，视图 `public.v_asset_subject_exclude_biz` | `cron.job_run_details`，成功数量在 `asset_lookup.sync_run_summary` |
| IP Geo | 从服务端 MMDB 目录选择文件后手动导入 | `public.ip_geo` | Station SQLite `geo_sync_runs` |
| IP / 域名情报 | Station 按周期从 Doris 全量拉取 | Redis SET `threat_intelligence:ip`、`threat_intelligence:domain` | Station SQLite `intel_sync_runs` |

Geo 与情报运行记录使用 Station 自身的 SQLite `station.db`；启用这两项功能时，Station 主数据库需配置为 SQLite。资产同步过程在目标 PostgreSQL 内执行，成功摘要也保存在该库中。

### 资产同步

资产任务执行连接默认使用目标 PostgreSQL 报告的第一个 Unix socket 目录。Station 创建或检查已有任务时会校正 `cron.job.nodename`；如需其他连接地址，可设置 `WP_KSYNC_ASSET_CRON_HOST`。该路径属于 PostgreSQL 服务端，不是 Station Pod 的挂载路径。

已有 `dysec_asset_source` 与部署环境变量不同时，Station 启动时原位更新主机、端口、库名和用户映射；使用新配置连接源库成功后才提交更新。验证失败会回滚并在页面显示连接错误，原有定义继续可用。历史中的 `connection failed` 是 pg_cron 在建立执行连接前记录的摘要，更具体的认证或网络原因需查看 PostgreSQL 服务端日志。

Station 启动时检查并创建目标表、GiST 索引、视图、同步过程与 `pg_cron` 任务；已有任务的周期与部署环境变量不同时，原位更新周期，启动不执行同步。过程从外部 `dysec-asset.public.asset_subject` 读取 `HOST`、`TERMINAL` 的 `ip` 和 `public_ip`，以及 `NET_WORK` 的 `data.cidr`，把 IP 和网段转换为数值区间。同一资产同时有 `ip` 和 `public_ip` 时写入两行，资产属性相同，地址分别写入 `ip` 字段；公网地址行的内部 `source_key` 带 `:public_ip` 后缀。先在临时表校验非空、主键及区间，再在同一 PostgreSQL 事务中刷新正式表；失败时回滚。成功后记录写入总行数、HOST / TERMINAL / NET_WORK 行数、IPv4 / IPv6 行数。资产历史直接读取 `pg_cron` 执行结果。

启动日志以 INFO 汇总三类知识库的配置、对象和任务状态，并分别列出最近执行结果与最近成功时间。`startup_sync=false` 表示启动只初始化对象和调度器；最近执行结果属于历史任务，不表示此次启动已同步。

### Geo 导入

容器默认扫描 `/app/mmdb`。本机 `cargo run` 可设置 `WP_KSYNC_MMDB_DIR=mmdb/`，相对路径以 Station 启动目录为基准。未配置目录时页面显示“数据未配置”。点击导入后记录文件版本与 SHA-256，并展示阶段、进度、已扫描 IPv4 / IPv6 网段和执行时长。

当前只把 IPv4 写入 `public.ip_geo`，IPv6 仅统计。新版本先写入独立临时表，建好主键和 GiST 索引并校验，再用短事务切换正式表；导入期间业务查询继续读取旧表。切换遇到读锁时限时重试，失败保留旧表。已成功导入的相同文件名、SHA-256 或相同数据库类型与构建时间，在点击导入时拒绝。开发样本可设置 `WP_KSYNC_GEO_MAX_ROWS=5000`，生产环境不设置此项。

### Doris 情报同步

Station 启动时只初始化运行记录和定时器。到达周期边界后，从 Doris `intel.stix_object` 分页全量读取有效 IP、域名情报，先写 Redis 临时 SET，校验后用单个 Lua 脚本切换两个正式键。读取、构建或校验失败时清理临时键，正式键保持原样；切换不创建备份键。任务不重叠，执行时间跨越周期时跳过错过的触发点。

`WP_KSYNC_INTEL_INTERVAL` 默认 `300` 秒，最小 `60` 秒。周期按 Unix 时间边界对齐：设置 `300` 秒时，每小时 `:00、:05、:10…` 开始；Station 启动后等待下一个边界。

### 部署环境变量

| 参数 | 用途 |
| --- | --- |
| `WP_STATION_DEVICE_TOKEN_KEY` | 可选；覆盖程序内置密钥，值为 32 字节随机密钥的 Base64 编码；同一数据库需持续使用同一密钥 |
| `WP_KSYNC_TARGET_URL` | 资产和 Geo 共用的目标 PostgreSQL URL；资产源复用其中的主机、端口及凭据 |
| `WP_KSYNC_ASSET_SOURCE_DATABASE` | 资产源 PostgreSQL 数据库名，其他连接参数默认取自目标 URL |
| `WP_KSYNC_ASSET_SOURCE_HOST` / `PORT` / `USERNAME` / `PASSWORD` | 可选覆盖项，仅在资产源连接参数与目标库不同时使用 |
| `WP_KSYNC_ASSET_CRON` | 资产 `pg_cron` 表达式，例如 `*/5 * * * *` |
| `WP_KSYNC_ASSET_CRON_HOST` | 可选；资产任务在 PostgreSQL 服务端使用的连接地址，默认取目标库第一个 Unix socket 目录 |
| `WP_KSYNC_ASSET_READER_ROLE` | 可选的目标库只读角色 |
| `WP_KSYNC_MMDB_DIR` / `WP_KSYNC_MMDB_WORK_DIR` | MMDB 扫描目录与导入临时目录 |
| `WP_KSYNC_GEO_MAX_ROWS` | 仅用于本地快速验证的 Geo 行数上限 |
| `WP_KSYNC_DORIS_HOST` / `PORT` / `USER` / `PASSWORD` | Doris FE 连接信息，数据库固定为 `intel`，密码可以为空 |
| `WP_KSYNC_REDIS_URL` | Redis 连接 URL |
| `WP_KSYNC_INTEL_INTERVAL` | 情报同步周期，单位秒 |

资产同步要求目标 PostgreSQL 可用 `dblink`、`btree_gist` 和预加载的 `pg_cron`；Geo 索引要求 `btree_gist`。目标库还需允许 Station 创建表、索引和扩展。开发与性能验证只使用本地副本和本地服务。运行记录和清理范围的细节见 [知识库同步实现说明](./KNOWLEDGE_SYNC_NOTES.md)。

## 资产和 Geo 查询性能检查

OML 中的 `geo.ip_geo`、`asset.v_asset_subject_exclude_biz` 是知识库连接名加表名。本地目标 PostgreSQL 的实际对象为 `public.ip_geo` 和 `public.v_asset_subject_exclude_biz`。将 `@__sip_num`、`@__sip_asset_num` 作为 `numeric` 参数传入后，可用下面的 SQL 复现同等查询。在连接到**本地目标库**的同一个 `psql` 会话执行：

```sql
SET plan_cache_mode = force_generic_plan;

PREPARE geo_lookup(numeric) AS
SELECT country_iso_code, country_name, province_name, city_name,
       continent_name, latitude, longitude
FROM public.ip_geo
WHERE start_ip_num <= $1 AND end_ip_num >= $1
ORDER BY start_ip_num DESC
LIMIT 1;

PREPARE asset_lookup(numeric) AS
SELECT asset_id, asset_name, asset_type, system_id, system_name, company_name
FROM public.v_asset_subject_exclude_biz
WHERE start_ip_num <= $1 AND end_ip_num >= $1
ORDER BY start_ip_num DESC, asset_priority DESC, asset_id ASC
LIMIT 1;

EXPLAIN (ANALYZE, BUFFERS) EXECUTE geo_lookup(134744072);
EXPLAIN (ANALYZE, BUFFERS) EXECUTE geo_lookup(2130706433);
EXPLAIN (ANALYZE, BUFFERS) EXECUTE asset_lookup(168427859);
EXPLAIN (ANALYZE, BUFFERS) EXECUTE asset_lookup(134744072);
```

这些数字分别对应 `8.8.8.8`、`127.0.0.1`、`10.10.0.83`。`force_generic_plan` 用于检查参数化查询复用通用计划时是否仍能使用索引。检查计划中的 `Index Cond` 是否同时包含区间两侧条件，以及是否使用 `ip_geo_num_lookup_gist_idx` 或 `asset_subject_exclude_biz_sync_num_gist_idx`。资产视图是普通视图，优化器会把查询下推到同步表。资产规则的 OML 查询也需在原有排序后加上 `asset_priority DESC, asset_id ASC`，使同一 IP 按 `HOST → TERMINAL → NET_WORK` 返回，同类重复 IP 按资产 ID 稳定选取；只更新 Station 视图而不更新 OML 查询，不能保证命中优先级。额外排序只作用于区间条件命中的候选行，应结合实际返回行数、`Buffers` 和 `Execution Time` 判断成本。`EXPLAIN ANALYZE` 会真正执行查询。

对应的资产 OML 查询排序应写为：

```sql
__sip_asset_id, __sip_asset_name, __sip_asset_type, __sip_system_id, __sip_system_name, __sip_company_name = select asset_id, asset_name, asset_type, system_id, system_name, company_name from asset.v_asset_subject_exclude_biz where start_ip_num <= @__sip_asset_num and end_ip_num >= @__sip_asset_num order by start_ip_num desc, asset_priority desc, asset_id asc limit 1;
```

此前的本地副本包含 Geo **6,157,739** 行、资产 **1,980** 行。强制通用计划后，两条查询都使用 GiST `Index Only Scan`，这批样本的 `Heap Fetches` 为 0：

| 查询 | 样本 | 执行耗时 |
| --- | --- | ---: |
| Geo 命中 | `8.8.8.8` | 约 1.2 ms |
| Geo 命中 | `1.1.1.1` | 约 0.59 ms |
| Geo 未命中 | `127.0.0.1`，缓存热时 | 约 0.04 ms |
| 资产命中 | `10.10.0.83`，同时匹配 HOST 和网段，取 HOST | 约 0.07 ms |
| 资产未命中 | `8.8.8.8` | 约 0.02 ms |

Geo 未命中查询首次读取磁盘页时曾达到约 **8.5 ms**，再次执行约 **0.04 ms**。这些数字只代表本地单次查询，不代表并发业务负载或其他机器的延迟；实际部署还需观察缓存命中和请求端到端耗时。

三类资产转换后的本地样本为 **15,373** 行。使用 14 个参数执行通用计划查询，旧排序与加入类别优先级及资产 ID 排序后均在 **14/14** 次查询中命中资产 GiST 索引，执行时间中位数分别约 **0.024 ms**、**0.025 ms**。这是本机热缓存小样本结果，不能代替生产并发测试。

## 配置模板

- `来源配置` 页新增 `新增输入源` 按钮，编辑的是 `project_infra/topology/sources/wpsrc.toml`。
- `输出配置` 页在 `business.d/*` 文件下新增 `新增输出源` 按钮，编辑的是 `project_infra/topology/sinks/business.d/*.toml`。
- 模板列表来自运行时扫描的 `project_infra/connectors/source.d` 与 `project_infra/connectors/sink.d`，不是后端内置常量表。
- 弹窗会展示 `必填参数`、`默认带出参数`、`已省略高级调优参数`；`batch`、`poll_interval_ms`、`error_backoff_ms`、`connect_timeout_secs`、`query_timeout_secs` 等高级字段默认不自动插入。
- 重复打开模板弹窗会重新读取 connector 文件；弹窗打开期间不会自动热刷新。

## 接入概览

- 顶部导航已提供独立入口 `接入概览`，对应页面路由 `/integration-overview`。
- 页面同时读取规则文件和运行时拓扑：
  - 规则覆盖摘要来自 `wpl` 文件解析
  - 运行时输入源 / 业务输出源摘要来自 `GET /api/integration-overview/runtime`
- 页面只展示业务视角字段，不暴露目录路径或内部实现术语。
- `设备类型` 优先取 WPL `package` 顶部 tag 中的 `dev_type`，缺失时回退到 `dev_name` 或原始标识。
- `日志类型` 优先取 `rule` 上的 `log_desc`，缺失时回退到原始标识。
- `ignore` 类型规则不展示、不计数。
- 顶部统计展示已覆盖设备类型、已覆盖日志类型、已启用输入源、已启用业务输出源。

## 当前状态与已知问题

1. 可运行的产品骨架，不是 demo，仍有工程收尾项。
2. 后端测试部分不是全绿，测试代码落后于最新接口和数据结构。
3. 前端测试基本可跑，有 1 个已知依赖导入失败。
4. `web/src/services/config.js` 存在真实接口与 Mock 混用。
5. `web/src/services/debug.js` 保留了部分后端未实现的遗留调用。
6. `web/vite.config.js` 代理端口和 `config/config.toml` 监听端口须保持一致（默认均为 8081）。
7. `web/dist/` 是构建产物，不是源代码，不要在此改功能。
8. `web/src/views/pages/simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留文件。
9. 连接配置左侧展示名仍依赖 `web/src/services/config.js` 中的 `LEGACY_CONNECTION_DISPLAY_NAMES` 做兼容映射。

## 开发指南

接需求或做修改前，请先阅读 [AGENTS.md](./AGENTS.md)。前端依赖统一使用 npm 和 `web/package-lock.json`；Tree-sitter 语言资产来自 `Cargo.lock` 锁定的依赖。

```bash
# 后端
cargo test

# 前端
cd web
npm run build
npm test
```

重置本地知识库同步产物时，先停止 Station，再运行根目录的 `python3 clean_knowledge_sync.py`，脚本会直接删除资产任务及目标对象、Geo 表、Redis 情报键和 Station SQLite 同步运行记录；如需先查看范围，使用 `python3 clean_knowledge_sync.py --dry-run`。未设置连接环境变量时，脚本默认连接本地 `postgres://postgres:123456@127.0.0.1:5432/postgres` 和 `redis://:123456@127.0.0.1:6379/0`；命令行 `--pg-url`、`--redis-url` 优先级最高。本地 SQLite 文件不存在时跳过该项。脚本不会删除源资产表、Doris 原表或 MMDB 文件。
