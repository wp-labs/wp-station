# AGENTS.md

`wp-station` 的 AI 开发导航。接需求前先读 `README.md` 了解项目全貌，再用本文件定位改哪些文件、注意哪些联动、遵守哪些规范。

## 阅读顺序

1. `README.md`：项目是什么、怎么跑
2. 本文件：改什么文件、联动什么、遵守什么规范
3. 对应源码：确定改动范围后再进目录

---

## 快查表

| 功能域 | 前端页面 | 前端 service | 后端 API | 后端逻辑 | 后端数据层 |
|--------|----------|--------------|----------|----------|------------|
| 设备管理 | `views/pages/system-manage/ConnectionManage.jsx` | `services/connection.js` | `src/api/device.rs` | `src/server/device.rs` | `src/db/device.rs` |
| 发布列表/详情 | `views/pages/system-release/index.jsx` `detail.jsx` | `services/release.js` | `src/api/release.rs` | `src/server/release.rs` `release_task_runner.rs` | `src/db/release.rs` `release_target.rs` |
| 沙盒预发布 | `views/pages/system-release/prepublish.jsx` | `services/sandbox.js` | `src/api/sandbox.rs` | `src/server/sandbox*.rs` | `src/db/sandbox.rs` |
| 规则管理 | `views/pages/rule-manage/index.jsx` | `services/config.js` | `src/api/rules.rs` | `src/server/rules.rs` | `project_root` 文件 + `src/utils/project.rs` |
| 配置管理 | `views/pages/config-manage/index.jsx` | `services/config.js` | `src/api/config.rs` | `src/server/config.rs` | `project_root` 文件 + `src/utils/project.rs` |
| 调试页 | `views/pages/simulate-debug/index.jsx` | `services/debug.js` | `src/api/debug.rs` | `src/server/debug.rs` | — |
| 用户/登录 | `views/pages/login/index.jsx` `system-manage/index.jsx` | `services/auth.js` `services/user.js` | `src/api/user.rs` | `src/server/user.rs` | `src/db/user.rs` |
| 操作日志 | `views/pages/system-manage/index.jsx` | `services/operation_log.js` | `src/api/operation_log.rs` | `src/server/operation_log.rs` | `src/db/operation_log.rs` |
| AI 辅助任务 | `components/AssistTaskCenter/index.jsx` | `services/assist_task.js` | `src/api/assist_task.rs` | `src/server/assist_task.rs` | `src/db/assist_task.rs` |
| 导航/路由/国际化 | `components/Navigation.jsx` `App.jsx` | `i18n/locales/*.json` | — | — | — |

---

## 项目心智模型

主链路：

```
前端页面 → services/* → /api → src/api/* → src/server/* → src/db/*
                                                         ↓
                                              src/utils/*（文件/设备/规则/知识库）
```

关键事实：

- **规则/配置/知识库以 `project_root` 文件为主数据源**；数据库仍承载设备、发布、用户、操作日志、沙盒等运行态数据。
- **规则/配置保存不只是写文件**，通常还要：写 `project_root` → 操作日志 → 同步 Gitea → 刷新草稿发布记录。
- **发布是设备维度的**，核心在 `release_targets`，不是改 release 主表状态。

---

## 核心流程

### 1. 配置/规则保存链路

前端保存后，后端按序执行：

1. 写入 `project_root`
2. 记录操作日志
3. 提交并同步到 Gitea（`src/server/sync.rs`）
4. 创建或刷新草稿发布记录

覆盖：配置管理、规则管理、知识库保存。

### 2. 设备健康检查链路

启动时和定时任务中遍历设备（`src/utils/health_check.rs`）：

1. 调用设备状态接口
2. 刷新在线状态、客户端版本、配置版本、最后在线时间

前端刷新按钮触发 `POST /api/devices/{id}/refresh`，后端复用同一逻辑即时更新单台设备。

### 3. 发布链路

1. 前端创建或选择发布版本
2. 发布时为每台设备生成 `release_targets`
3. `release_task_runner` 周期轮询待处理任务
4. 通过 `WarpParseService`（`src/utils/warparse_service.rs`）调设备部署接口和状态接口
5. 汇总设备结果后刷新 release 聚合状态

### 4. 沙盒预发布验证链路

阶段：准备工作区 → 预检查 → 启动 wparse/wpgen → 结果分析 → 收尾

- 日志文件：`prepare.log`、`check.log`、`wparse.log`、`wpgen.log`、`analysis.log`
- 发布按钮依赖最近一次沙盒任务为通过状态（`sandbox_ready` 字段）
- 前端：`prepublish.jsx` + `components/*`；后端：`src/server/sandbox*.rs`

---

## 代码地图

### 后端

| 路径 | 职责 |
|------|------|
| `src/main.rs` | 启动入口 |
| `src/server/app.rs` | 应用装配，初始化 DB / Gitea / 健康检查 / 发布调度器，**新 API 须在此挂载** |
| `src/api/*` | HTTP 路由和入参/出参，不承载业务逻辑 |
| `src/server/*` | 业务编排主入口，绝大多数需求先改这里 |
| `src/db/*` | CRUD、分页、状态更新；规则/知识库不再建表存储 |
| `src/utils/common.rs` | 通用常量定义与展示格式化工具（北京时间格式化、sink 展示名称推导） |
| `src/utils/project.rs` | `project_root` 中规则、配置、知识库文件的读写、扫描和初始化辅助 |
| `src/utils/project_check.rs` | 项目组件完整性校验（基于 wp_proj） |
| `src/utils/warparse_service.rs` | 设备状态与发布接口，设备调用统一入口 |
| `src/utils/health_check.rs` | 设备在线检查 |
| `src/utils/sandbox.rs` | 沙盒运行时管理（工作区准备、进程启停、配置生成、输出收集） |
| `src/utils/wpl.rs` | WPL 解析与格式化 |
| `src/utils/knowledge.rs` | 知识库加载、查询、重载 |
| `src/utils/oml.rs` | OML 转换与格式化 |
| `src/error.rs` | 统一错误模型和 HTTP 错误响应格式 |
| `crates/migrations/*` | 数据库迁移与 entity |
| `crates/gitea/*` | Git/Gitea 封装，不掺杂 Station 业务判断 |

### 前端

| 路径 | 职责 |
|------|------|
| `web/src/main.jsx` | 前端入口，初始化请求配置和 i18n |
| `web/src/App.jsx` | 路由总入口、全局主题、鉴权包装、AssistTaskProvider |
| `web/src/services/*` | 所有页面优先走 service，再调 `/api` |
| `web/src/views/pages/*` | 各业务页面 |
| `web/src/views/components/*` | 导航、编辑器、diff viewer、任务中心等复用组件 |
| `web/src/contexts/AssistTaskContext.jsx` | AI/人工辅助任务全局状态和轮询 |
| `web/src/hooks/*` | 工作区、多实例逻辑 |
| `web/src/i18n/locales/*.json` | 中英文文案，新增文案必须同步两个文件 |

---

## 按功能定位修改入口

### 设备管理

改动注意：
- 设备在线状态、客户端版本、配置版本是状态接口回填，不是人工输入字段。
- 健康检查与发布都依赖 `token`，改设备字段时注意这两处联动。
- 新增接口记得在 `src/server/app.rs` 挂载。

### 发布列表 / 详情 / 执行 / 重试 / 回滚

改动注意：
- 发布围绕 `release_targets` 运行，不要只改 release 主表。
- 设备状态完成后要回写 release 聚合状态。
- 回滚是"把上一成功版本重新发布"，不是数据库回退。
- 发布备注复用 release 表的 `pipeline` 字段，前端弹窗填写后随请求提交。
- diff 组件：`web/src/components/diff/*`；Gitea 同步：`src/server/sync.rs`。

### 沙盒预发布验证

改动注意：
- 阶段日志文件名须与后端、前端展示保持一致。
- `sandbox_ready` 字段控制发布按钮是否可用，前后端同步维护。

### 规则管理（wpl / oml / knowledge）

改动注意：
- `knowledge` 与 `wpl/oml` 数据结构不完全相同。
- 保存后须完整走：写 `project_root` → 操作日志 → Gitea → 草稿发布。
- 规则校验逻辑变化时，前端成功/失败弹窗一并核对。

### 配置管理（parse / source / sink / connect）

改动注意：
- 链路与规则管理相同，也会触发文件写入、同步和草稿发布。
- `parse` 与连接配置的文件和规则类型不同，前端有一层映射。
- `services/config.js` 有真实接口与 Mock 混用，改之前先确认你动的是哪一支。

### 调试页

改动注意：
- 前端调用比后端接口"超前"，有遗留调用。
- 后端已实现的调试接口：`/api/debug/parse`、`/api/debug/knowledge/status`、`/api/debug/knowledge/query`、`/api/debug/performance/run`、`/api/debug/performance/{taskId}`、`/api/debug/wpl/format`、`/api/debug/oml/format`、`/api/debug/examples`。
- `services/debug.js` 里的 `/debug/transform`、`/debug/decode/base64` 后端尚未实现，不要默认已有。
- `index-old.jsx` 和 `index-backup.jsx` 是遗留文件，不要动。

### 用户 / 登录 / 密码 / 操作日志

改动注意：
- 前端登录态保存在 `sessionStorage`。
- 后端错误统一走 `AppError` JSON 结构。
- 新增用户字段时，前后端同步 snake_case / camelCase 映射。

### AI / 人工辅助任务

改动注意：
- 任务轮询逻辑在全局 `AssistTaskContext`，不在单页内。
- AI 和 manual 任务共用一套表和接口。
- 返回结果写回由 `/assist/reply` 驱动。

### 导航 / 国际化 / 全局 UI

改动注意：
- 新增页面要同时接路由（`App.jsx`）和导航（`Navigation.jsx`）。
- 新增 UI 文案必须同步 `zh-CN.json` 和 `en-US.json`。

---

## 常见修改套路

### 新增一个后端字段

1. `crates/migrations` 添加迁移
2. 更新对应 entity
3. `src/db/*` 模型和 CRUD
4. `src/server/*` 请求/响应 DTO
5. `src/api/*` 出参
6. `web/src/services/*` 字段映射
7. 页面组件显示和编辑
8. 相关测试

### 新增一个 API

1. `src/server/*` 先写业务逻辑
2. `src/api/*` 暴露路由
3. `src/server/app.rs` 挂载 service
4. `web/src/services/*` 增加调用封装
5. 相关页面接入

### 修改配置/规则保存行为

必须核对五个联动点：
1. 操作日志
2. 写入 `project_root`
3. 同步到 Gitea
4. 刷新草稿发布记录
5. 重新加载知识库（如涉及）

### 修改设备发布行为

必须核对五个联动点：
1. `release_targets` 状态流转
2. 设备状态回写
3. release 聚合状态刷新
4. 前端列表页和详情页状态显示
5. 操作日志

---

## 开发规范

### 日志（`tracing` 宏）

**级别选择：**

| 级别 | 用于 |
|------|------|
| `info!` | 关键业务节点的开始、完成、状态变化、数量统计 |
| `warn!` | 当前流程可继续但结果降级（如 Gitea 同步失败但配置已保存） |
| `error!` | 当前请求/链路已失败，必须中断（如数据库连接失败） |
| `debug!` | 调试细节、远端请求/响应参数，高噪音不影响正常日志 |

**格式规则：**
- 日志文本用中文，`key=value` 风格带稳定标识符（`release_id=1`、`device_id=2`）
- 表达"对象 + 动作 + 结果"：`info!("更新用户成功: id={}", id)`
- 错误日志必须带原因：`warn!("健康检查失败: device_id={}, error={}", id, err)`
- 优先在 `server` / `db` / `utils` 层打日志，`api` 层不作为主要日志承载层
- 同一事件不跨层重复打印

**禁止：**
- 打印密码、Token、完整认证头、原始密钥
- 打印大块原始日志样本、完整 SQL、超长响应体（`debug!` 也要截断）
- 只打印"进入函数"或无上下文的"失败了"
- 在循环里大量重复打印无标识的日志

**推荐模式：** 入口一条开始日志 → 成功一条结果日志 → 异常一条带原因的 `warn!` 或 `error!`

---

### 操作日志（`operation_logs` 表）

**写入时机：** 用户或系统触发有业务意义的动作时写。

常见动作：`create`、`update`、`delete`、`validate`、`publish`、`retry`、`rollback`、`login`、`reset-password`、`change-password`、`release-target`

**不写：** 普通查询、列表拉取、纯读取接口、定时轮询每一步。

**职责边界：**
- 只在 `server` 层或业务调度层写，`db` 层和 `api` 层不写。
- 统一通过 `src/server/operation_log.rs` 的公共入口写入，使用 `OperationLogBiz`、`OperationLogAction`、`OperationLogParams`、`write_operation_log_for_result(...)`。
- 业务层只传：业务类型、动作类型、少量关键参数、主业务 `result`。
- 操作日志写入失败不能覆盖主业务结果。

**字段规范：**
- `operator`：真实用户名；拿不到时用 `system`
- `operation`：受控词汇，低基数，不把业务描述直接塞进来
- `target`：面向人类阅读，带对象名和关键 ID，如 `设备 nginx-prod [ID: 3]`
- `description`：短句动作摘要，如 `修改设备配置`
- `content`：审计细节，写关键参数/版本号/设备列表，**必须脱敏**
- `status`：`success` / `error`

**推荐写法：**
```rust
// 1. 保存业务上下文
// 2. 执行业务逻辑
let result = do_something().await;
// 3. 写操作日志（best-effort，不影响主流程）
write_operation_log_for_result(biz, action, params, &result).await;
// 4. 返回原业务结果
result
```

---

### 注释

- 统一使用**中文**注释
- 对外 public 类型/函数用 `///`，局部实现用 `//`
- 注释解释"为什么这样做、边界条件、调用方注意事项"，不解释一眼就懂的语法
- 长文件用分段标题：`// ============ 请求参数结构体 ============`
- 兼容逻辑、历史原因、降级逻辑必须注释，显式标注"遗留/兼容/后续可删除"

**禁止：** 注释复述代码表面行为、大段过期注释与代码不一致、只在注释里写业务规则。

---

### 目录归属

新增代码落点：

| 场景 | 落点 |
|------|------|
| 业务编排、副作用顺序、操作日志 | `src/server/*` |
| HTTP 路由、入参、出参 | `src/api/*` |
| CRUD、分页、状态更新 | `src/db/*` |
| 外部系统调用、跨模块工具 | `src/utils/*`（含 `common.rs` 通用工具、`sandbox.rs` 沙盒管理）或独立 crate |
| 项目组件校验 | `src/utils/project_check.rs` |
| 数据库结构变更 | `crates/migrations` |
| 前端接口调用、字段映射 | `web/src/services/*` |
| 跨页面全局状态/轮询 | `web/src/contexts/*` |
| 可复用状态逻辑 | `web/src/hooks/*` |

**不要：** 把业务逻辑放进 `api` 或 `db`；把接口调用写进页面组件；把工具方法塞到 `server` 文件底部。

---

## 关键联动约束

**配置和规则：**
- 保存逻辑变化后，必须核对：写入 `project_root` → Gitea → 草稿发布。
- `project_root` 是规则/配置/知识库主数据源，默认配置初始化只能补齐缺失文件，不能覆盖用户已编辑内容。

**设备与发布：**
- `token` 是发布和健康检查的关键字段。
- 发布状态需同时看 release 主表和每台设备的 target 状态。
- 设备调用只走 `WarpParseService`，不在别处散落请求逻辑。

**前端 contract：**
- 页面层不直接拼后端字段，统一在 `services` 做适配。
- 后端返回 snake_case，前端显示层用 camelCase。
- 后端错误格式：`{ success: false, error: { code, message, details } }`

---

## 当前已知漂移

接需求时，以下是既有问题，不是你的改动引起的：

1. `services/config.js` 有真实接口与 Mock 混用。
2. `services/debug.js` 保留了后端未实现的调试接口调用。
3. `simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留文件。
4. `web/vite.config.js` 代理端口和 `config/config.toml` 监听端口须同步（默认均为 8081）。
5. Rust 测试有一部分落后于最新接口定义。
6. 前端有 1 个已知依赖导入失败用例。
7. `web/dist` 是构建产物，不要当源码改。

---

## 验证建议

按功能域验证，不做无差别全量扫：

```bash
cargo test          # 后端逻辑改动
cd web && npm test  # 前端改动
cargo run           # 联调
```

改配置/规则/发布相关逻辑后，手动验证：
1. 保存是否成功
2. 操作日志是否写入
3. `project_root` 是否更新
4. 发布列表/详情是否反映新状态
