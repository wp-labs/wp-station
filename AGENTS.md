# AGENTS.md

本文件是 `wp-station` 的项目级开发导航。后续无论是人还是 AI，要接前后端需求，都应先读根目录 `README.md` 和本文件，再决定是否深入某个子目录。

目标不是替代源码，而是让你在不知道全仓细节的情况下，先判断：

- 需求属于哪个业务域
- 需要改前端哪些页面和 service
- 需要改后端哪些 API / server / db / utils
- 哪些联动点不能漏
- 哪些文件是遗留代码，不要误改

## 阅读顺序

1. 先读根目录 `README.md`
2. 再读本文件
3. 只有在明确改动范围后，再进入对应目录看源码

如果只是一般前后端功能开发，通常不需要先遍历整个仓库。

## 项目心智模型

`wp-station` 的主链路是：

1. 前端页面调用 `web/src/services/*`
2. service 经 `/api` 调用 Rust 后端
3. `src/api/*` 做 HTTP 路由
4. `src/server/*` 做业务编排
5. `src/db/*` 读写 PostgreSQL
6. 必要时 `src/utils/*` 负责导出项目目录、调用设备、校验规则、加载知识库
7. 配置类改动最终会导出到 `project_root`，并尽可能同步到 Gitea

### 关键事实

- 数据库是运行时主数据源。
- `project_root` 是导出目录，不是主编辑源。
- 规则/配置保存通常不只是“写库”，而是“写库 + 导出 + Git/Gitea + 草稿发布 + 操作日志”。
- 发布是按设备维度执行，不是简单改 release 表状态。

## 代码地图

### 后端

- `src/main.rs`
  - 启动入口。
- `src/server/app.rs`
  - 应用装配入口。
  - 初始化数据库、默认配置、Gitea、本地静态资源、健康检查、发布调度器。
- `src/api/*`
  - API 路由和请求/响应入口。
- `src/server/*`
  - 业务逻辑主入口，绝大多数需求应先在这里改。
- `src/db/*`
  - Repository 和模型。
- `src/utils/project.rs`
  - 从数据库导出规则/知识库到 `project_root`。
- `src/utils/warparse_service.rs`
  - 访问设备状态、发布接口。
- `src/utils/health_check.rs`
  - 设备在线检查。
- `src/utils/wpl.rs`
  - WPL 解析和格式化相关能力。
- `src/utils/knowledge.rs`
  - 知识库加载、查询、重载。
- `src/error.rs`
  - 统一错误模型和 HTTP 错误响应格式。
- `crates/migrations/*`
  - 数据库迁移与 entity。
- `crates/gitea/*`
  - Git/Gitea 封装。

### 前端

- `web/src/main.jsx`
  - 前端入口，初始化请求配置和 i18n。
- `web/src/App.jsx`
  - 路由总入口、全局主题、认证包装、AssistTaskProvider 注入。
- `web/src/services/*`
  - 页面不要直接写接口，优先改这里。
- `web/src/views/pages/*`
  - 各业务页面。
- `web/src/views/components/*`
  - 复用组件，如导航、编辑器、diff viewer、任务中心。
- `web/src/contexts/AssistTaskContext.jsx`
  - AI/人工辅助任务全局状态和轮询。
- `web/src/hooks/*`
  - 工作区、多实例逻辑。
- `web/src/i18n/locales/*.json`
  - 所有 UI 文案都要同步中英文资源。

## 按功能定位修改入口

### 1. 设备管理

功能入口：

- 前端页面：`web/src/views/pages/system-manage/ConnectionManage.jsx`
- 前端 service：`web/src/services/connection.js`
- 后端 API：`src/api/device.rs`
- 后端逻辑：`src/server/device.rs`
- 后端数据层：`src/db/device.rs`
- 健康检查：`src/utils/health_check.rs`
- 设备状态/版本拉取：`src/utils/warparse_service.rs`

改动时要注意：

- 设备创建和更新都依赖 `token`。
- 保存后会触发在线检查，前端会直接感知是否可连通。
- 设备在线状态、客户端版本、配置版本不是人工输入字段，而是状态接口回填。
- 前端连接卡片右上角的刷新按钮会调用 `POST /api/devices/{id}/refresh`，后端复用健康检查逻辑即时更新单台设备状态；新增接口记得在 `src/server/app.rs` 注册。

### 2. 发布列表、发布详情、发布执行、重试、回滚

功能入口：

- 前端列表页：`web/src/views/pages/system-release/index.jsx`
- 前端详情页：`web/src/views/pages/system-release/detail.jsx`
- 前端 service：`web/src/services/release.js`
- diff 组件：`web/src/components/diff/*`
- 后端 API：`src/api/release.rs`
- 后端逻辑：`src/server/release.rs`
- 发布调度器：`src/server/release_task_runner.rs`
- Gitea/Git 同步：`src/server/sync.rs`
- 设备调用：`src/utils/warparse_service.rs`
- 设备级发布表：`src/db/release_target.rs`
- 发布表：`src/db/release.rs`

改动时要注意：

- 发布是围绕 `release_targets` 跑的，不要只改 release 主表。
- 设备状态更新完成后，要回写 release 聚合状态。
- 回滚是“把上一成功版本重新发布”，不是简单数据库回退。
- 发布前可能需要先推 Git tag。
- 发布详情页的设备状态来源是 release target + device 聚合结果。
- 发布备注复用 release 表的 `pipeline` 字段：创建或发布时，如果填写了备注，就会写入该字段；前端发布弹窗也需要把备注随请求一并提交。

### 2.1 沙盒预发布验证

功能入口：

- 前端页面：`web/src/views/pages/system-release/prepublish.jsx`
- 前端组件：`web/src/views/pages/system-release/components/*`（阶段时间轴、历史记录、日志查看、结果面板）
- 前端 service：`web/src/services/sandbox.js`
- 后端 API：`src/api/sandbox.rs`
- 后端逻辑：`src/server/sandbox.rs`、`sandbox_runner.rs`、`sandbox_analyzer.rs`、`sandbox_diagnostics.rs`
- 后端数据层：`src/db/sandbox.rs`、`crates/migrations/src/entity/sandbox_run.rs`

改动时要注意：

- 沙盒阶段（准备目录、预检查、启动 wparse、启动 wpgen、结果分析、收尾）需要同步维护 UI 与后端逻辑，日志文件命名为 `prepare.log`、`check.log`、`wparse.log`、`wpgen.log`、`analysis.log`。
- 沙盒运行依赖 `docs/sandbox-runtime-validation-design.md` 中定义的流程与诊断规则；前后端文案需保持一致。
- 发布前置校验依赖最近一次沙盒任务为通过状态，发布按钮与页面提示都引用 `sandbox_ready` 字段。

### 3. 规则管理

功能范围：

- `wpl`
- `oml`
- `knowledge`
- 规则文件列表/内容/新增/删除/校验/保存

功能入口：

- 前端页面：`web/src/views/pages/rule-manage/index.jsx`
- 前端 service：`web/src/services/config.js`
- 后端 API：`src/api/rules.rs`
- 后端逻辑：`src/server/rules.rs`
- 后端数据层：`src/db/rule_config.rs`
- 知识库数据层：`src/db/knowledge_config.rs`
- 规则校验：`src/utils/check.rs`
- 知识库加载：`src/utils/knowledge.rs`
- 项目导出：`src/utils/project.rs`
- Git/Gitea：`src/server/sync.rs`

改动时要注意：

- `knowledge` 与普通 `wpl/oml` 不共用完全相同的数据结构。
- 保存规则后，通常需要：
  - 写数据库
  - 记录操作日志
  - 导出到 `project_root`
  - 同步到 Gitea
  - 更新草稿发布记录
- 改规则校验逻辑时，前端成功/失败弹窗也要一起核对。

### 4. 配置管理

功能范围：

- `parse`
- `source`
- `sink`
- `source_connect`
- `sink_connect`

功能入口：

- 前端页面：`web/src/views/pages/config-manage/index.jsx`
- 前端 service：`web/src/services/config.js`
- 后端 API：`src/api/config.rs`
- 后端逻辑：`src/server/config.rs`
- 后端数据层：`src/db/rule_config.rs`
- 项目导出：`src/utils/project.rs`
- Git/Gitea：`src/server/sync.rs`

改动时要注意：

- 这条链路和规则管理类似，也会触发导出、同步和草稿发布。
- `parse` 与连接配置的文件和规则类型不同，前端会做一层映射。
- `web/src/services/config.js` 当前存在真实接口与 Mock 混用，修改前先确认你动的是哪一支逻辑。

### 5. 调试页

功能范围：

- 日志解析
- OML 转换
- 知识库查询
- 示例
- 性能测试占位
- AI 辅助填充

功能入口：

- 前端页面：`web/src/views/pages/simulate-debug/index.jsx`
- 前端 service：`web/src/services/debug.js`
- 规则/知识库辅助 service：`web/src/services/config.js`
- 多实例/工作区：`web/src/hooks/useMultipleInstances.js`、`web/src/hooks/useWorkspace.js`
- Assist 上下文：`web/src/contexts/AssistTaskContext.jsx`
- 后端 API：`src/api/debug.rs`
- 后端逻辑：`src/server/debug.rs`
- WPL 解析：`src/utils/wpl.rs`
- 知识库工具：`src/utils/knowledge.rs`

改动时要注意：

- 当前调试页前端代码比后端接口更“超前”，存在遗留接口调用。
- 目前真实后端明确存在的调试接口主要是：
  - `/api/debug/parse`
  - `/api/debug/knowledge/status`
  - `/api/debug/knowledge/query`
  - `/api/debug/performance/run`
  - `/api/debug/performance/{taskId}`
  - `/api/debug/wpl/format`
  - `/api/debug/oml/format`
  - `/api/debug/examples`
- `web/src/services/debug.js` 里仍有 `/debug/transform`、`/debug/decode/base64` 等前端遗留调用，修改时不要默认后端已经实现。
- `web/src/views/pages/simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留文件，优先不要动。

### 6. 用户、登录、密码、操作日志

功能入口：

- 前端页面：`web/src/views/pages/login/index.jsx`
- 前端页面：`web/src/views/pages/system-manage/index.jsx`
- 前端 service：`web/src/services/auth.js`
- 前端 service：`web/src/services/user.js`
- 前端 service：`web/src/services/operation_log.js`
- 后端 API：`src/api/user.rs`
- 后端 API：`src/api/operation_log.rs`
- 后端逻辑：`src/server/user.rs`
- 后端逻辑：`src/server/operation_log.rs`
- 后端数据层：`src/db/user.rs`
- 后端数据层：`src/db/operation_log.rs`

改动时要注意：

- 前端登录态目前主要保存在 `sessionStorage`。
- 后端错误返回统一走 `AppError` JSON 结构。
- 若新增用户字段，前后端都要同步 snake_case / camelCase 映射。

### 7. AI / 人工辅助任务

功能入口：

- 前端上下文：`web/src/contexts/AssistTaskContext.jsx`
- 前端 service：`web/src/services/assist_task.js`
- 前端入口组件：`web/src/views/components/AssistTaskCenter/index.jsx`
- 调试页结果抽屉：`web/src/views/pages/simulate-debug/components/*`
- 后端 API：`src/api/assist_task.rs`
- 后端逻辑：`src/server/assist_task.rs`
- 后端数据层：`src/db/assist_task.rs`

改动时要注意：

- 任务轮询逻辑在前端全局 context，不在单页内。
- AI 和 manual 任务共用一套表和接口。
- 返回结果写回由 `/assist/reply` 驱动。

### 8. 导航、国际化、全局 UI

功能入口：

- 导航：`web/src/views/components/Navigation.jsx`
- 路由：`web/src/App.jsx`
- 鉴权：`web/src/views/components/RequireAuth.jsx`
- 国际化初始化：`web/src/i18n/index.js`
- 文案：`web/src/i18n/locales/zh-CN.json`、`web/src/i18n/locales/en-US.json`

改动时要注意：

- 新增页面要同时接路由和导航。
- 新增 UI 文案时必须同时补中文和英文资源。

## 常见修改套路

### 新增一个后端字段

通常要一起检查：

1. `crates/migrations` 迁移
2. 对应 entity
3. `src/db/*` 模型和 CRUD
4. `src/server/*` 请求/响应 DTO
5. `src/api/*` 出参与前端 contract
6. `web/src/services/*` 字段映射
7. 页面组件显示和编辑
8. 相关测试

### 新增一个 API

通常要一起检查：

1. `src/server/*` 先写逻辑
2. `src/api/*` 暴露路由
3. `src/server/app.rs` 确认已挂载 service
4. `web/src/services/*` 增加调用封装
5. 相关页面接入

### 修改配置/规则保存行为

不要只改前端保存按钮或只改数据库写入。至少确认这些联动点：

1. 是否需要操作日志
2. 是否需要导出到 `project_root`
3. 是否需要同步到 Gitea
4. 是否需要刷新草稿发布记录
5. 是否需要重新加载知识库

### 修改设备发布行为

至少确认这些联动点：

1. `release_targets` 状态流转
2. 设备状态回写
3. release 聚合状态刷新
4. 前端列表页和详情页状态显示
5. 操作日志

## 关键联动约束

### 配置和规则

- 配置/规则/知识库保存的真实主链路在后端。
- `project_root` 是导出结果，不是编辑源。
- 只要改了保存逻辑，就必须重新核对：
  - 导出逻辑
  - Gitea 同步
  - 草稿发布记录

### 设备与发布

- 设备 `token` 是发布和健康检查的关键字段。
- 发布状态不能只靠 release 主表，要看每台设备的 target 状态。
- `WarpParseService` 是设备调用统一入口，避免在别处散落请求逻辑。

### 前端 contract

- 前端页面尽量不要直接拼后端字段，统一在 `services` 做适配。
- 后端大多返回 snake_case，前端显示层多用 camelCase。
- 后端错误统一是：
  - `success: false`
  - `error.code`
  - `error.message`
  - `error.details`

### 文案与国际化

- 新增或修改可见文案时，同时更新 `zh-CN.json` 和 `en-US.json`。

## 统一优化规范

以下规范是后续统一代码优化、重构和补齐一致性时的准绳。即使当前仓库中仍存在历史遗留写法，后续新改动也应尽量向这些规则收敛。

### 1. 日志打印规范

适用范围：

- Rust 后端运行日志
- `tracing` 宏：`info!`、`warn!`、`error!`、`debug!`

目标：

- 让日志能回答“谁在做什么、做到哪一步、为什么失败”
- 避免无意义噪音和敏感信息泄漏

分级规则：

- `info!`
  - 记录关键业务节点的开始、完成、状态变化、数量统计。
  - 适合启动流程、发布流程、导出流程、保存成功、分页查询结果数量。
- `warn!`
  - 记录“当前流程可继续，但结果降级或部分失败”的情况。
  - 例如：Gitea 同步失败但配置已保存、健康检查失败但服务仍可运行、后台任务单条失败但调度循环继续。
- `error!`
  - 记录“当前请求/启动/关键链路已失败，必须中断或返回错误”的情况。
  - 例如：数据库连接失败、迁移失败、初始化 Gitea 失败。
- `debug!`
  - 记录调试细节、远端请求参数、响应结构、内部状态。
  - 默认只用于高噪音、排障导向的信息，不应影响正常运行日志可读性。

内容规则：

- 日志文本优先使用中文，格式尽量稳定。
- 一条日志只表达一个事件，不把多个阶段揉进同一条。
- 优先包含稳定标识符，建议使用 `key=value` 风格。
  - 例如：`release_id=1`、`device_id=2`、`task_id=xxx`
- 优先记录“对象 + 动作 + 结果”。
  - 例如：`更新用户成功: id=3`
  - 例如：`发布 API 响应状态: 500`
- 成功日志尽量记录关键结果，不重复打印整个对象。
- 错误日志必须带错误原因。
  - 推荐：`warn!("健康检查失败: device_id={}, error={}", device_id, err);`

边界规则：

- 不打印密码、Token、完整认证头、原始密钥。
- 不打印大块原始日志样本、完整 SQL、超长响应体，除非明确处于 `debug!` 且已做截断。
- API 层不作为主要日志承载层；运行日志优先写在 `server`、`db`、`utils`。
- 同一事件避免跨层重复打印同一语义日志。
  - 例如 `server` 已记录“用户创建成功”后，`api` 不必再重复记录。

推荐模式：

1. 入口记录一条开始日志
2. 成功结束记录一条结果日志
3. 异常分支记录一条带原因的 `warn!` 或 `error!`

不推荐模式：

- 只打印“进入函数”
- 打印无上下文的“失败了”
- 在循环里大量重复打印无标识的日志

### 2. 操作日志写入规范

适用范围：

- 写入 `operation_logs` 表的审计类日志
- 数据结构：`src/db/operation_log.rs` 中的 `NewOperationLog`

目标：

- 记录“谁做了什么业务动作，作用于什么对象，结果如何”
- 面向审计、回溯、页面展示，不等同于运行日志

职责边界：

- 操作日志只在 `server` 层或明确的业务调度层写入。
- `db` 层不负责写操作日志。
- `api` 层不直接写操作日志。
- 后台设备级发布结果允许在调度器中补充细粒度日志，例如 `release-target`。

何时写：

- 用户或系统触发了有业务意义的动作时写。
- 典型动作：
  - `create`
  - `update`
  - `delete`
  - `validate`
  - `publish`
  - `retry`
  - `rollback`
  - `login`
  - `reset-password`
  - `change-password`
  - `release-target`

何时不写：

- 普通查询、列表拉取、纯读取接口默认不写。
- 内部函数调用、纯格式化、纯校验辅助步骤默认不单独写。
- 数据库 CRUD 细节、定时轮询每一步默认不写，除非有明确审计价值。

字段规范：

- `operator`
  - 真实操作者用户名优先。
  - 暂时拿不到用户上下文时，统一使用 `system`。
- `operation`
  - 使用受控词汇，保持低基数。
  - 避免把业务描述直接塞到 `operation` 字段里。
- `target`
  - 面向人类阅读，描述“操作对象是谁”。
  - 推荐带对象名和关键 ID。
  - 例如：`设备 nginx-prod [ID: 3]`
- `description`
  - 页面展示摘要，用短句说明动作。
  - 例如：`修改设备配置`、`触发发布到目标设备`
- `content`
  - 审计细节，写关键参数、数量、版本号、设备列表、上下文信息。
  - 必须脱敏，不写密码、Token、完整凭证。
- `status`
  - 当前统一为 `success` / `error`

写入时机：

- 推荐业务逻辑先执行，再根据结果统一写一条操作日志。
- 推荐模式：
  1. 保存业务必要上下文
  2. 执行业务逻辑，得到 `result`
  3. 按 `result.is_ok()` 组装 `status`
  4. best-effort 写入操作日志
  5. 返回原业务结果

失败处理：

- 操作日志写入失败不能覆盖主业务结果。
- 推荐统一通过 `src/server/operation_log.rs` 提供的公共入口 best-effort 写入。
- 只有后台关键审计链路才考虑额外 `warn!` 提示，但仍不能反向影响主流程。

粒度规则：

- 一个用户动作对应一条主操作日志。
- 如果该动作会派生出异步子任务，可由后台再补充子日志。
- 不要为同一动作在多个层级写多条重复审计日志。

当前统一实现规范：

- 业务代码不要直接构造 `src/db/operation_log.rs` 中的 `NewOperationLog`。
- 业务代码不要在各自模块里继续写 `build_save_log`、`build_delete_log` 之类的重复拼装函数。
- 操作日志统一通过 `src/server/operation_log.rs` 写入。
- 推荐使用：
  - `OperationLogBiz`
  - `OperationLogAction`
  - `OperationLogParams`
  - `write_operation_log_for_result(...)`
- 业务层只负责传：
  - 业务类型
  - 动作类型
  - 少量关键参数
  - 主业务 `result`
- `server/operation_log.rs` 统一负责生成：
  - `operation`
  - `target`
  - `description`
  - `content`
  - `status`

推荐模式：

1. 保存业务必要上下文
2. 执行业务逻辑，得到 `result`
3. 调用 `write_operation_log_for_result(biz, action, params, &result).await`
4. 返回原业务结果

参数约定：

- `OperationLogBiz`
  - 表示业务域，例如 `ConfigFile`、`RuleFile`、`KnowledgeConfig`、`AssistTask`
- `OperationLogAction`
  - 表示动作，例如 `Create`、`Update`、`Delete`、`Submit`、`Cancel`、`Reply`
- `OperationLogParams`
  - 只放少量业务参数
  - 推荐使用：
    - `with_target_name(...)`
    - `with_target_id(...)`
    - `with_field(key, value)`

迁移要求：

- 后续逐个业务域收敛时，优先删除手写 `NewOperationLog` 拼装代码。
- 新增业务功能时，默认直接接入 `src/server/operation_log.rs` 的统一入口。
- 除非遇到当前公共抽象无法表达的特殊场景，否则不要回退到模块内自定义日志组装。

### 3. 注释规范

目标：

- 注释解释“为什么这样做、边界条件是什么、调用方需要注意什么”
- 不解释一眼就能看懂的语法动作

语言规范：

- 仓库统一使用中文注释。
- 对外类型和函数优先使用 Rust 文档注释 `///`。
- 局部实现说明使用普通注释 `//`。

推荐写法：

- 文件头说明模块职责。
  - 例如：`// 设备管理业务逻辑层`
- 长文件使用分段标题。
  - 例如：`// ============ 请求参数结构体 ============`
- 对公开函数写 `///`，说明用途和边界。
  - 例如：`/// 删除指定 ID 的设备（软删除）`
- 对复杂逻辑块写“为什么”。
  - 例如：为什么要先导出再推 Gitea、为什么只在某状态允许发布。
- 对兼容逻辑、历史原因、降级逻辑写注释。

不推荐写法：

- 注释复述代码表面行为。
  - 差例：`// 给变量赋值`
- 大段过期注释与代码不一致。
- 把业务规则只写在注释里，代码本身没有表达。
- 用注释代替提炼函数。

具体要求：

- 新增 public struct / enum / public fn 时，优先补 `///`。
- 复杂格式化器、解析器、状态机、调度器必须写清楚核心流程与不变量。
- 对临时兼容、遗留接口适配，要显式标注“遗留/兼容/后续可删除”。
- 前端组件内仅在状态复杂、交互跨区块时加注释；简单 JSX 结构不必逐段解释。

### 4. 目录结构与功能归属规范

目标：

- 让开发者看到需求后，能快速定位“该改哪层”
- 避免逻辑散落、跨层职责混乱

后端目录职责：

- `src/api`
  - 只负责 HTTP 路由、入参解析、调用 `server`、返回响应。
  - 不承载核心业务逻辑。
- `src/server`
  - 业务编排主入口。
  - 校验业务状态、组织多仓储调用、决定副作用顺序、写操作日志。
- `src/db`
  - 数据访问层。
  - 负责 CRUD、筛选、分页、批量更新。
  - 不负责业务流程编排，不负责操作日志。
- `src/utils`
  - 跨模块工具和外部系统封装。
  - 包括导出项目目录、调用 WarpParse、知识库加载、WPL/OML 工具等。
- `crates/migrations`
  - 数据库结构变更入口。
- `crates/gitea`
  - Git/Gitea 基础封装，不掺杂 Station 的业务判断。

前端目录职责：

- `web/src/services`
  - 前端访问后端的唯一标准入口。
  - 负责接口调用、字段映射、响应兼容。
- `web/src/views/pages`
  - 页面容器和页面级状态。
- `web/src/views/components`
  - 可复用组件、页面内共用部件。
- `web/src/hooks`
  - 可复用状态逻辑与行为抽象。
- `web/src/contexts`
  - 跨页面的全局状态与轮询逻辑。
- `web/src/i18n`
  - 多语言资源和初始化。

放置规则：

- 新增业务能力，优先按现有业务域扩展，不新建“杂项目录”。
- API contract 变更时，前后端都应有各自稳定入口：
  - 前端改 `services`
  - 后端改 `api + server`
- 纯数据库字段变化不能只改 `db`，必须同步检查 `server`、前端 service、页面和测试。
- 纯页面样式问题不要把展示逻辑塞进 `services`。
- 纯工具方法不要塞进页面组件或 `server` 文件底部，优先提到 `utils` 或页面同目录私有函数。

建议的功能改动落点：

1. 新接口：`server` 先行，`api` 暴露，`services` 接入
2. 新字段：迁移、entity、db、server、service、page、test 一起改
3. 新副作用：明确归属到 `server`，不要散落在 `api` 或 `db`
4. 新外部集成：优先落到 `utils` 或独立 crate

## 统一优化检查清单

后续做统一优化或重构时，建议按下面顺序逐项检查。

### A. 日志治理检查

- 是否使用了正确的日志级别
- 是否补齐了关键链路的开始/结束/失败日志
- 是否去掉了重复、空泛、无标识的日志
- 是否避免打印密码、Token、完整认证信息、超长原始内容
- 是否统一成稳定的 `key=value` 或固定句式

### B. 操作日志治理检查

- 当前动作是否真的需要审计日志
- 操作日志是否只在 `server` 或业务调度层写入
- `operation` 是否使用受控词汇
- `target`、`description`、`content` 是否职责清晰
- `content` 是否已脱敏
- 操作日志失败是否不会影响主业务返回

### C. 注释治理检查

- 文件头是否写清模块职责
- public 类型/函数是否有 `///`
- 复杂状态机、调度器、格式化器是否解释了“为什么”
- 是否删除了过期注释
- 是否去掉了纯复述代码的低价值注释

### D. 目录归属治理检查

- 业务逻辑是否落在 `server`，而不是 `api` 或 `db`
- 数据访问是否收敛在 `db`
- 外部系统调用是否收敛在 `utils` 或独立 crate
- 前端页面是否只通过 `services` 访问后端
- 公共状态是否沉淀到 `hooks` / `contexts`

### E. 功能联动治理检查

- 字段变更是否同步了迁移、实体、DTO、service、页面和测试
- 配置/规则保存是否核对了导出、Gitea、草稿发布、操作日志
- 发布逻辑是否核对了 `release_targets`、设备状态、聚合状态、页面展示
- 文案变更是否同步了 `zh-CN` 和 `en-US`

## 当前已知漂移

这些问题是当前仓库的既有状态。后续接需求时不要误判为你的改动引起：

1. `web/src/services/config.js` 有真实接口与 Mock 并存。
2. `web/src/services/debug.js` 保留了部分后端未实现的调试接口调用。
3. `web/src/views/pages/simulate-debug/index-old.jsx` 和 `index-backup.jsx` 是遗留文件。
4. `web/vite.config.js` 默认代理 `8080`，`config/config.toml` 也默认监听 8080；改端口时务必两端同步。
5. Rust 测试有一部分仍落后于最新接口定义。
6. 前端依赖测试当前有 1 个已知失败用例。
7. `web/dist` 是构建产物，不要当源码改。

## 验证建议

改动完成后，优先按功能域验证，而不是无差别全量扫：

- 后端逻辑改动：`cargo test`
- 前端改动：`cd web && npm test`
- 联调：`cargo run` 或 `cd web && npm run dev`

如果你改的是配置/规则/发布相关逻辑，至少手动验证：

1. 保存是否成功
2. 操作日志是否写入
3. `project_root` 是否更新
4. 发布列表/详情是否反映新状态

## 文档约定

后续如果项目结构或主链路发生变化，优先更新：

1. 根目录 `README.md`
2. 根目录 `AGENTS.md`

不要再把新的项目级规则散落到子目录说明里。
