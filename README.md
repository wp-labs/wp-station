# wp-station 1.0

`wp-station` 是 WarpParse 配置与发布控制台的 1.0 版本。它不是单纯的规则编辑器，而是一个把规则维护、知识库管理、调试验证、设备管理、版本发布、Git/Gitea 同步串起来的完整站点。

从现在开始，根目录的 `README.md` 和 `AGENTS.md` 是本仓库的一级事实来源。后续无论是人还是 AI，要理解项目或接功能，都应优先阅读这两份文档，而不是依赖各子目录里遗留的旧说明。

## 1.0 范围

- 设备管理：维护 WarpParse 目标设备的 IP、端口、Token、在线状态、客户端版本、配置版本，前端连接卡片支持“一键刷新”按钮，实时调用 WarpParse 状态 API 更新某台设备的在线信息。
- 规则管理：维护 `wpl`、`oml`、`knowledge` 等规则文件，并支持格式化、校验、保存。
- 配置管理：维护 `parse`、`source`、`sink`、`source_connect`、`sink_connect` 等配置。
- 知识库：维护知识库配置、建表 SQL、插入 SQL、样例数据，并支持 SQL 查询。
- 调试：支持日志解析、知识库查询、性能任务占位能力，以及 WPL/OML 格式化。
- 发布：基于发布记录和 `release_targets` 按设备驱动发布、轮询状态、失败重试、人工回滚；发布备注复用 release 的 `pipeline` 字段，支持在发布弹窗中填写并保存。
- 沙盒预发布验证：复用 WarpParse 运行环境，在真实样例数据上模拟执行，展示阶段日志、诊断建议、历史记录，并在最近一次通过后才允许发布。
- 系统管理：用户、登录、密码、操作日志。
- AI 辅助：提交 AI / 人工辅助任务，前端全局轮询任务状态并回填建议。

## 系统定位

运行中的 `wp-station` 有 5 个核心角色：

1. Rust 后端：提供 API、持久化、任务调度、规则校验、设备健康检查。
2. React 前端：控制台 UI，通过 `/api` 调后端。
3. PostgreSQL：系统数据库，真正的运行时数据源。
4. `project_root`：从数据库导出的项目目录，供 Gitea 和设备发布使用。
5. Gitea / WarpParse 设备：分别承接配置版本化和远端配置加载。

有一个非常重要的边界：

- 运行时“单一数据源”是数据库，不是 `project_root`。
- `project_root` 是数据库内容导出的工作目录，不应被当成主编辑入口。
- `default_configs/` 是初始化种子，不是后续运行时的实时来源。

## 核心流程

### 1. 配置/规则保存链路

前端保存配置或规则后，后端会按以下顺序处理：

1. 写入 PostgreSQL
2. 记录操作日志
3. 从数据库全量导出到 `project_root`
4. 提交并同步到 Gitea
5. 创建或刷新草稿发布记录

这一链路覆盖：

- 配置管理页面
- 规则管理页面
- 知识库保存

### 2. 设备健康检查链路

系统启动时和后台定时任务中会遍历设备：

1. 调用设备状态接口
2. 刷新在线状态
3. 回填客户端版本、配置版本、最后在线时间

若需要即时刷新单台设备，前端“连接管理”卡片右上角的刷新按钮会触发 `POST /api/devices/{id}/refresh`，后端复用同一健康检查逻辑，成功后立即返回最新状态。

### 3. 发布链路

发布不是直接改 release 状态，而是走设备级任务：

1. 前端创建或选择发布版本
2. 发布时为每台设备生成 `release_targets`
3. 后台 `release_task_runner` 周期轮询待处理任务
4. 通过 `WarpParseService` 调设备部署接口和状态接口
5. 汇总设备结果后刷新 release 总状态

### 4. 沙盒预发布验证链路

在正式发布前，可以在“系统发布 > 预发布验证”页面执行沙盒任务：

1. 后端根据 `docs/sandbox-runtime-validation-design.md` 描述的阶段（准备工作区、预检查、启动 wparse/wpgen、结果分析、终止阶段）串联任务。
2. 每个阶段会生成同名日志（`prepare.log`、`check.log`、`wparse.log`、`wpgen.log`、`analysis.log`），前端同屏展示阶段状态与日志。
3. 任务执行完毕后，会更新 `sandbox_runs` 表，历史记录、阶段日志、诊断建议都可在页面查看。
4. 发布按钮需要最近一次沙盒任务通过；若失败，需要修复后再运行。

前端入口：`web/src/views/pages/system-release/prepublish.jsx` 及同目录下的组件，服务封装在 `web/src/services/sandbox.js`；后端入口：`src/api/sandbox.rs`、`src/server/sandbox*.rs`。

### 4. 调试链路

- WPL 解析在站内执行，核心入口是 `warp_check_record`
- 知识库 SQL 查询通过 `knowledge` 工具完成
- 调试页部分前端逻辑仍保留旧接口适配，见“当前状态与已知漂移”

## 仓库结构

```text
wp-station/
├── src/                      # Rust 主应用
│   ├── api/                  # HTTP 路由层
│   ├── server/               # 业务逻辑层
│   ├── db/                   # Repository / 数据访问
│   ├── utils/                # 导出、健康检查、WarpParse、WPL/知识库工具
│   ├── error.rs              # 统一错误返回
│   ├── lib.rs
│   └── main.rs
├── crates/
│   ├── migrations/           # SeaORM 迁移与 entity
│   └── gitea/                # Git/Gitea 封装
├── tests/                    # Rust 测试
├── config/config.toml        # 后端运行配置
├── default_configs/          # 初始化默认配置种子
├── docs/                     # 设计文档
├── web/                      # React 控制台
│   ├── src/
│   │   ├── services/         # API 调用封装
│   │   ├── views/pages/      # 页面
│   │   ├── views/components/ # 通用/业务组件
│   │   ├── contexts/         # 全局状态，如 AssistTask
│   │   ├── hooks/
│   │   └── i18n/
│   ├── public/               # tree-sitter / 静态资源
│   └── dist/                 # 前端构建产物，Rust 会嵌入此目录
└── build.rs                  # 非 release 构建时会自动构建前端
```

## 后端分层约定

- `src/api/*`：只处理 HTTP 入参、出参和路由声明。
- `src/server/*`：主要业务逻辑，后续加功能一般先落这里。
- `src/db/*`：CRUD、分页、状态更新、查询封装。
- `src/utils/*`：跨模块工具，尤其是：
  - `project.rs`：导出数据库内容到 `project_root`
  - `warparse_service.rs`：设备状态与发布接口
  - `health_check.rs`：设备在线检查
  - `wpl.rs`：解析与格式化相关工具
  - `knowledge.rs`：知识库加载与 SQL 执行
- `crates/migrations`：数据库结构变更入口。

## 前端分层约定

- `web/src/services/*`：所有页面应先走 service，再调 `/api`。
- `web/src/views/pages/*`：页面容器，负责页面状态和交互。
- `web/src/views/components/*`：编辑器、导航、实例切换、diff viewer、任务中心等通用组件。
- `web/src/contexts/AssistTaskContext.jsx`：AI/人工辅助任务全局轮询。
- `web/src/hooks/*`：工作区、多实例状态。
- `web/src/i18n/*`：中英文文案资源。

## 关键运行依赖

### 必需

- Rust
- Node.js
- PostgreSQL

### 按功能需要

- Gitea：首次初始化本地项目仓库、保存后同步配置、发布版本差异查看依赖 Git/Gitea 能力。
- WarpParse 设备：健康检查、发布、回滚依赖真实设备。
- Assist 服务：AI/人工辅助任务依赖 `[assist]` 配置的外部服务。

## 启动方式

### 1. 配置后端

编辑 `config/config.toml`：

```toml
[web]
host = "0.0.0.0"
port = 8080

[database]
host = "localhost"
port = 5432
name = "wp-station"
username = "postgres"
password = "123456"

project_root = "project_root"

[gitea]
base_url = "http://localhost:3000"
username = "gitea"
password = "123456"

[assist]
base_url = "http://localhost:8888"

[warparse]
base_url = "http://localhost:19090"
```

### 2. 运行后端

```bash
cargo run
```

说明：

- 启动时会自动初始化连接池和数据库迁移。
- 在非 release 构建下，`build.rs` 会自动执行 `web` 下的 `npm install` 和 `npm run build`。
- 后端会直接托管 `web/dist`，所以 `cargo run` 后可直接访问站点。

### 3. 单独运行前端开发服务

```bash
cd web
npm install
npm run dev
```

说明：

- 前端开发默认走 Vite 代理。
- 当前仓库中，Vite 代理端口和后端配置端口默认并不一致，见“当前状态与已知漂移”。

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
npm test
```

## 当前状态与已知漂移

1. 这已经是可运行的产品骨架，不是 demo，但仍有工程收尾项。
2. 后端测试当前不是全绿，主要是测试代码落后于最新接口和数据结构。
3. 前端测试基本可跑，但仍有 1 个依赖导入测试失败。
4. 前端 `web/src/services/config.js` 仍存在“真实接口 + Mock”混用逻辑。
5. 前端 `web/src/services/debug.js` 里仍保留 `debug/transform`、`debug/decode/base64` 之类遗留调用；后端当前并没有对应完整 API。
6. `web/vite.config.js` 默认代理到 `http://localhost:8080`，`config/config.toml` 也默认监听 8080；如需切换端口请同步两处配置。
7. `web/dist/` 是构建产物，不是源代码；不要把它当成功能修改入口。
8. `web/src/views/pages/simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留页面，不是当前主入口。

## 文档边界

为避免再次出现“代码和说明不一致”的情况，本仓库约定：

- 根目录 `README.md` 负责讲清楚项目是什么、怎么跑、整体架构和当前状态。
- 根目录 `AGENTS.md` 负责讲清楚“改某类功能要进哪些文件、有哪些联动点、哪些地方不能漏”。
- 子目录文档只允许做简短跳转或局部补充，不再作为项目级权威说明。

如果后续要接前端或后端功能，请先读 `AGENTS.md`。
