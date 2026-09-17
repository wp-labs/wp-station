# wp-station

WarpParse 配置与发布控制台。把规则维护、知识库管理、调试验证、设备管理、版本发布、Git/Gitea 同步串联成完整工作流。

变更记录见 [CHANGELOG.md](./CHANGELOG.md)。

## 变更记录规范

- 版本段按实际发布版本保留并存，不用后续版本覆盖前一版本内容。
- 每个版本优先总结用户可感知的能力、接口和发布链路变化，不写按文件路径罗列的改动清单。
- 涉及接口时，优先描述接口类别或能力边界，例如“调试接口”“发布链路”“编辑器高亮与格式化”，只在有必要时点名具体 API。
- 依赖升级仅记录对功能有直接影响的部分，例如运行时、语法树、高亮补全、构建链路等关键组件。
- 内容以提交中能直接确认的事实为准，不额外补写 README、讨论记录或口头约定里的推断。

## 功能

- **设备管理**：维护设备 IP、端口、Token、在线状态、客户端版本、配置版本，支持一键刷新单台设备状态。
- **规则管理**：维护 `wpl`、`oml`、`knowledge` 等规则文件，支持格式化、校验、保存。
- **配置管理**：维护 `parse`、`source`、`sink`、`source_connect`、`sink_connect` 配置；`来源配置` 与 `业务 sink 配置` 支持按 connector 模板实时追加片段。
- **知识库**：维护知识库配置、建表 SQL、插入 SQL、样例数据，支持 SQL 查询。
- **调试**：日志解析、知识库查询、WPL/OML 格式化、性能任务占位。
- **接入概览**：汇总规则覆盖的设备类型 / 日志类型，以及运行时已启用的输入源 / 业务输出源。
- **发布**：按设备维度驱动发布、轮询状态、失败重试、人工回滚；支持填写发布备注。
- **沙盒预发布验证**：在真实样例数据上模拟执行，展示阶段日志、诊断建议、历史记录，通过后才允许正式发布。
- **连接管理**：维护 wparse / wfusion 设备连接和运行状态。
- **项目导入导出**：支持目录导入、压缩包预检导入、确认导入和项目归档导出。
- **AI 辅助**：提交 AI/人工辅助任务，前端全局轮询并回填建议。

## 系统定位

运行中的 `wp-station` 有 6 个核心角色：

| 角色 | 说明 |
|------|------|
| Rust 后端 | 提供 API、持久化、任务调度、规则校验、设备健康检查 |
| React 前端 | 控制台 UI，通过 `/api` 调后端 |
| PostgreSQL / SQLite | 系统数据库，承载设备、发布、用户等运行态数据 |
| `project_models` | `wpl` / `oml` / `knowledge` 等 model 侧文件主数据源 |
| `project_infra` | `conf` / `connectors` / `topology` 等 infra 侧文件主数据源 |
| Gitea / WarpParse 设备 | 分别承接配置版本化和远端配置加载 |

> 当前运行时采用双仓库布局：`project_models` 保存 `models/*`，`project_infra` 保存 `conf`、`connectors`、`topology`。`default_configs/` 只补齐缺失文件，不覆盖用户已编辑内容。

## 前置依赖

**必需**

- Rust
- Node.js
- PostgreSQL 或 SQLite

**按功能需要**

- Gitea：首次初始化本地仓库、配置同步、发布差异查看
- WarpParse 设备：健康检查、发布、回滚
- Assist 服务：AI/人工辅助任务

## 快速开始

### 1. 配置后端

编辑 `config/config.toml`：

```toml
[web]
host = "0.0.0.0"
port = 8081

project_models = "./project_models"
project_infra = "./project_infra"

[database]
# 默认示例使用 SQLite
url = "sqlite:///data/wp-station/station.db"
# 以下字段仅在 url 为空时，作为 PostgreSQL 兼容配置生效
host = "localhost"
port = 5432
name = "wp-station"
username = "postgres"
password = "123456"

[gitea]
base_url = "http://localhost:3000"
username = "gitea"
password = "123456"

[assist]
base_url = "http://localhost:8888"

[admin_api]
enabled = true
ca_file = "./CA.crt"

[features]
data_collect_url = "http://localhost:18080/wp-monitor"
```

> 默认推荐直接使用 `database.url="sqlite:///data/wp-station/station.db"`。只有当 `database.url` 为空时，才会按 `host/port/name/username/password` 连接 PostgreSQL。`project_models` 默认 `./project_models`，`project_infra` 默认 `./project_infra`。配置文件支持环境变量覆盖，例如 `WP_STATION__WEB__PORT=8082`、`WP_STATION__PROJECT_INFRA=/data/project_infra`。设备管理接口路径固定为 `/admin/v1/reloads/model` 与 `/admin/v1/runtime/status`，设备的 IP/端口由设备记录决定；当前代码兼容旧配置节名 `[warparse]`，但新配置请统一使用 `[admin_api]`。

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

## 常用命令

```bash
# 后端
cargo run
cargo test

# 前端
cd web
npm install
npm run dev
npm run build
```

前端依赖统一使用 npm 和 `web/package-lock.json`。Tree-sitter 语言资产固定来自
`Cargo.lock` 锁定的依赖，不读取同级目录中的本地仓库。

## 配置管理最新变化

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

接需求或做修改前，请先阅读 [AGENTS.md](./AGENTS.md)。
