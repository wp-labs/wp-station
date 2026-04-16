# wp-station

WarpParse 配置与发布控制台。把规则维护、知识库管理、调试验证、设备管理、版本发布、Git/Gitea 同步串联成完整工作流。

## 功能

- **设备管理**：维护设备 IP、端口、Token、在线状态、客户端版本、配置版本，支持一键刷新单台设备状态。
- **规则管理**：维护 `wpl`、`oml`、`knowledge` 等规则文件，支持格式化、校验、保存。
- **配置管理**：维护 `parse`、`source`、`sink`、`source_connect`、`sink_connect` 配置。
- **知识库**：维护知识库配置、建表 SQL、插入 SQL、样例数据，支持 SQL 查询。
- **调试**：日志解析、知识库查询、WPL/OML 格式化、性能任务占位。
- **发布**：按设备维度驱动发布、轮询状态、失败重试、人工回滚；支持填写发布备注。
- **沙盒预发布验证**：在真实样例数据上模拟执行，展示阶段日志、诊断建议、历史记录，通过后才允许正式发布。
- **系统管理**：用户、登录、密码、操作日志。
- **AI 辅助**：提交 AI/人工辅助任务，前端全局轮询并回填建议。

## 系统定位

运行中的 `wp-station` 有 5 个核心角色：

| 角色 | 说明 |
|------|------|
| Rust 后端 | 提供 API、持久化、任务调度、规则校验、设备健康检查 |
| React 前端 | 控制台 UI，通过 `/api` 调后端 |
| PostgreSQL | 系统数据库，承载设备、发布、用户、操作日志等运行态数据 |
| `project_root` | 规则、配置、知识库的主数据源，供 Gitea 和设备发布使用 |
| Gitea / WarpParse 设备 | 分别承接配置版本化和远端配置加载 |

> `project_root` 是规则、配置、知识库的运行时来源。`default_configs/` 是初始化种子，只补齐缺失文件，不覆盖用户已编辑内容。

## 前置依赖

**必需**

- Rust
- Node.js
- PostgreSQL

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

[database]
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

[features]
data_collect_url = "http://localhost:18080/wp-monitor"
```

> 未配置 `project_root` 时默认使用 `./project_root`；WarpParse 客户端 API 路径固定为 `/admin/v1/reloads/model` 与 `/admin/v1/runtime/status`，设备的 IP/端口由设备记录决定。

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

## 当前状态与已知问题

1. 可运行的产品骨架，不是 demo，仍有工程收尾项。
2. 后端测试部分不是全绿，测试代码落后于最新接口和数据结构。
3. 前端测试基本可跑，有 1 个已知依赖导入失败。
4. `web/src/services/config.js` 存在真实接口与 Mock 混用。
5. `web/src/services/debug.js` 保留了部分后端未实现的遗留调用。
6. `web/vite.config.js` 代理端口和 `config/config.toml` 监听端口须保持一致（默认均为 8081）。
7. `web/dist/` 是构建产物，不是源代码，不要在此改功能。
8. `web/src/views/pages/simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留文件。

## 开发指南

接需求或做修改前，请先阅读 [AGENTS.md](./AGENTS.md)。
