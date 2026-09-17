# Changelog
English | 中文

All notable changes to this project will be documented in this file.

The format is based on Keep a Changelog, and the project follows Semantic Versioning where practical.

Note:
- 以下内容按 `git` 提交记录和改动文件整理。
- 同一 `Cargo.toml` 版本跨多个日期时，使用 `-1`、`-2` 这类后缀区分时间点。
- 每条只记录提交里能直接看到的事实，不依赖 `README.md` 或 `AGENTS.md` 的补充描述。
- `0.3.x` 为当前未发布的大版本更新线，与下方 `0.2.x` 历史版本分开记录。

## [0.3.6] - 2026-09-17
Commit range: `20e7f0b..36f5b0c`

### Added
- 项目导入统一为“导入配置”，同时支持选择配置文件夹或配置压缩包；只读取所选层级下的 `conf`、`connectors`、`models`、`topology`，避免递归扫描下层目录造成规则重复导入
- 发布成功记录新增“还原”能力：可将上一次成功发布的配置复制回对应 Gitea 目录，已有草稿时覆盖草稿，没有草稿时自动创建草稿
- `WFusion` 沙盒支持按 `models/scenarios` 顺序执行多个 `.wfg` 场景，并汇总生成与发送结果
- 知识库支持多配置管理与按配置查询、重载
- 接入概览支持展示 `WFusion` 的 WFS/WFL 文件名及文件内声明的规则，并提供“规则 / 窗口”切换；默认展示规则，每页 20 条

### Changed
- 导入结果改为覆盖式处理并提供导入统计；导出配置包按系统和 Unix 秒命名为 `wparse-<秒>.tar.gz` 或 `wfusion-<秒>.tar.gz`
- 补充并统一双系统默认配置：`WFusion` 的 sink 模板、`WParse` 的 `ignore` 和 `raw_log` 输出模板均会在初始化或沙盒准备时补齐，且不覆盖用户已有配置
- `WParse` 沙盒启动后等待 5000 毫秒收集输出；检查 `default.dat`、`miss.dat`、`residue.dat`、`error.dat` 和 `all.json`，同时展示 `ignore.json`、`raw_log.json`，后两者仅供查看、不参与结果判定
- 规则编辑器输入多条日志时只取第一条非空日志解析，并在检测到多条日志时给出提示；`WFusion` 编辑器同步优化缩放、滚动和实例选择交互
- 系统管理入口收敛为连接管理，移除用户、帮助中心和操作日志页面；删除操作日志的 API、服务端写入/查询链路、数据实体及相关测试，旧登录地址保留为兼容跳转
- `WFusion` 沙盒阶段名称明确区分 `wfusion` 与 `wfgen`，生成阶段使用 `--no-oracle`；调整沙盒任务清理、阶段日志保留及发布/同步流程
- 更新 WPL、OML、WFS、WFL、WFG 的 Tree-sitter 资源及构建同步逻辑，刷新前端依赖和 CI 发布动作版本
- 运行时依赖升级：`wp-motor` 相关组件至 `v1.25.8`，`wf-engine` / `wf-lang` 至 `v0.4.0`，并同步升级 WPL、知识库、模型、解析和数据格式组件
- `Cargo.toml` 与 `version.txt` 版本从 `0.3.5` 升级至 `0.3.6`

### Fixed
- 修复 `copy_event_parse` 导致 wfgen 生成数量小于解析数据数量的问题
- 修复项目导入时误扫描 `.wfusion-validation` 等下层目录，导致 WFusion 规则数量翻倍的问题
- 修复多场景 WFusion 沙盒阶段标签及运行结果统计不一致的问题
- 修复规则编辑器多日志解析时的输入选择和提醒缺失问题

## [0.3.5] - 2026-07-31
Commit: `20e7f0b`

### Added
- 登录页新增本地验证码校验：验证码在前端随机生成并展示，用户输入匹配后才发起登录请求，验证码支持点击刷新；补充对应的中英文 i18n 文案与验证码按钮的焦点、禁用态样式

### Changed
- `Cargo.toml`: version `0.3.4` -> `0.3.5`
- 沙盒任务 ID 格式从 `sandbox-<timestamp>-<suffix>` 调整为 `sandbox-<system>-<timestamp>-<suffix>`，在任务标识中显式区分 `wparse` 与 `wfusion`
- `SandboxRun::new` 新增 `system: SystemKind` 参数，用于构造带系统前缀的任务 ID
- `WFusion` 沙盒生成阶段新增 `--no-oracle` 参数，跳过场景期望输出（oracle）校验，避免 oracle 编译失败阻断事件进入已启动的 wfusion 运行时
- 沙盒历史清理策略调整：旧任务的 project 目录整体删除（不再保留合并后的配置目录），所有任务的阶段日志长期保留
- 前端路由鉴权层级调整：`RequireAuth` 提升至 `SystemProvider` 外层，避免系统 URL 同步覆盖登录跳转
- `SystemContext` 默认系统行为收敛：直接访问不带 `system` 参数的地址时始终从 WParse 开始，移除自动向 URL 追加系统参数的副作用导航
- 登录页表单字段从“可选”改为必填（用户名、密码）或本地校验（验证码），placeholder 与标签文案同步更新

## [0.3.4] - 2026-07-27
Commit: `pending`

### Added
- 新增按系统读取默认配置的规则示例库接口，`WParse` 与 `WFusion` 调试页可分别加载对应示例；`WFusion` 示例会从场景文件关联加载 WFS / WFL，并生成可直接回放的 NDJSON
- `WFusion` Rules 新增固定置顶的 `_global.wfl` 全局规则文件，用于复用提取字段；已有项目启动时仅补齐缺失文件且不覆盖现有内容，前后端同时禁止删除
- 版本接口与顶部版本信息新增 `WFusion` 运行时版本，并将系统切换入口整合到页头

### Changed
- `wp-station` 版本升级至 `0.3.4`，`wf-engine` / `wf-lang` 升级至 `v0.1.36`，同步更新 WFS / WFL / WFG 的 Tree-sitter wasm、高亮与补全资源
- `WFusion` 规则编辑器接入工作区与示例库双模式，NDJSON、WFS、WFL 分别支持多实例保存、切换和恢复，解析结果布局及不同屏幕尺寸下的编辑区域同步优化
- 默认 SSH 暴力破解示例精简为最小学习案例：3 条同源账号失败登录事件命中 1 次规则并输出 1 条告警，WFS、WFL 与场景配置保持自包含且易于理解
- `WFusion` schema、rule、scenario 新建与保存统一使用平铺文件布局，同时兼容读取和删除历史嵌套路径
- 接入概览改为展示已启用输入源、已启用输出源的实际数量；`WFusion` 规则摘要收敛为窗口结构和关联分析规则两类数量

### Fixed
- 修复覆盖式项目导入完成后才读取旧项目快照，导致删除数量统计不准确的问题
- 修复 `WFusion` 沙盒禁用管理接口时删除 `[admin_api.auth]` 配置段，造成校验错误行号与原始文件错位的问题

## [0.3.3] - 2026-07-21
Commit: `8af671a`, `5536ccf`

### Changed
- 项目导入改为覆盖式行为，导入后会清理旧文件和旧目录，不再只做增量新增
- `WFusion` 导入结果与规则展示继续收敛，导入摘要不再展开具体文件名，`scenarios` 也支持平铺与目录层级混合展示
- 接入概览的运行时输入源改为扫描 `topology/sources` 下的启用文件，按文件与类型统计 `WFusion` 输入源
- `WFusion` 沙盒配置改为保留仓库原始内容，仅补丁运行时必需项，避免覆盖手工修改的 `wfusion.toml`
- `WFusion` 场景文件读取兼容平铺与虚拟目录两种路径形式，避免旧展示值导致 404

## [0.3.2] - 2026-07-13
Commit: `pending`

### Changed
- `Cargo.toml`: version `0.3.1` -> `0.3.2`
- `Cargo.toml`: `wf-engine` / `wf-lang` tag `v0.1.25` -> `v0.1.31`
- `Cargo.toml`: `tree-sitter-wpl`、`tree-sitter-oml`、`tree-sitter-wfl` 切换为远端 `main` 分支依赖，并新增 `wasmparser`
- Tree-sitter 资源构建链路升级：开发态 `cargo run` 会自动同步并校验 `WPL / OML / WFS / WFL / WFG` 的 wasm、高亮与补全资产，前端编辑器统一按语言清单动态加载资源
- 调试能力继续补齐：`wfusion` 调试接口对齐新版运行时与语法树能力，支持 `WFS / WFL / WFG` 的格式化、试跑结果展示和错误定位增强
- `WFusion` 默认项目与沙盒工作区按最新目录布局重整，补齐 schema / rule / scenario / source / sink 样例联动，修复引用路径、输出文件覆写和 `admin_api.token` 权限问题
- 双系统默认 connectors 初始化收敛到共享镜像链路，减少 `wparse` 本地重复模板目录
- `infra` 发布链路修复共享 `connectors` 漏提交流程：发布前镜像进仓库的未跟踪文件会参与状态判断和 tag 生成，避免设备端拿到缺失 connectors 的 `infra` 版本后 reload 失败并回滚
- `WFusion` 发布结果判定改为以发布接口返回值为准：对齐新版 `accepted / result / update / current_version / resolved_tag / error` 契约，将 `restart_required` 识别为已完成但需重启生效的终态结果，同时避免仅因状态接口缺少 `config_version` 就把本次发布误判为失败

## [0.3.1] - 2026-07-09
Commit: `pending`

### Changed
- `Cargo.toml` 当前版本保持 `0.3.1`，本次仅补充变更记录，不额外修改版本号
- `WFusion` 规则编辑器完成页面级收敛：导航入口与 `规则编辑器` 菜单统一，按系统在 `wparse` 调试页与 `wfusion` 编辑页之间切换，不再出现页面来回跳转
- `WFusion` 编辑器交互改为单页双栏：顶部固定 `NDJSON` 输入，下方左侧通过按钮切换 `WFS / WFL`，右侧直接展示解析结果，支持 `表格模式 / JSON 模式`、固定高度滚动和空值开关
- `WFusion` 解析结果输出从内部告警头改为导出后的完整记录，前端可直接看到 `yield` 产出的字段，而不是仅有基础元字段
- `WFusion` 解析错误和校验失败统一内嵌到“解析结果”面板中，不再单独显示诊断区域
- `wfadm check` 编辑态校验切换为按 `--what` 指定当前文件类型，`WFusion` 规则/配置页不再默认整项目校验；发布页、导入预检和沙盒仍保留整项目校验
- `WFusion` 接入概览改为按文件数量统计：`已接入窗口结构` 统计 `.wfs`，`已接入关联分析规则` 统计 `.wfl`，并在表格中分列展示两类文件
- 接入概览页面与接口补充系统分支字段，前后端可在同一接口下分别返回 `wparse` 的设备/日志聚合摘要与 `wfusion` 的窗口/规则平铺摘要

## [0.3.0] - 2026-07-07
Commit: `daa52a7`

### Added
- 新增双系统基础模型：`SystemKind`、固定系统目录布局、共享 connectors 仓库布局，以及围绕 `wparse / wfusion` 的系统级路由分发能力
- 新增 `WFusion` 默认配置与样例项目，覆盖 `conf`、`models/windows.toml`、`models/schemas`、`models/rules`、`models/scenarios`、`topology/sources`、`topology/sinks` 与 `runtime/admin_api.token`
- 新增 `WFusion` 客户端能力与服务端配套逻辑，包括设备访问封装、发布/校验/沙盒所需的命令调用与项目目录准备
- 新增 `WFusion` 规则类型支持：`windows`、`schema`、`rule`、`scenarios`，并补充对应的读写、创建、删除、格式化与校验入口
- 新增独立的 `WFusion` 规则编辑页及后端试跑接口，支持 `WFS`、`WFL` 解析与事件回放
- 新增 Tree-sitter 语言资源与前端编辑器接入，覆盖 `WPL / OML / WFS / WFL / WFG` 的高亮、补全与资源清单
- 新增接入概览后端模块与前端页面，统一展示规则侧摘要以及输入源 / 输出源运行时摘要
- 新增项目导入导出、归档预检、规则摘要、运行时摘要等一批新测试和配套 API

### Changed
- `Cargo.toml` 当前为 `0.3.2`；`0.3.0` 作为本轮大版本基线在变更记录中单独标记，未对应一次独立的 `Cargo.toml` 提交落版
- 默认配置目录从单套结构重组为 `default_configs/wparse`、`default_configs/wfusion`、`default_configs/shared` 三部分，部署与初始化逻辑同步改造
- 服务端模块按领域拆分为 `app / config / debug / device / operation_log / overview / project / release / rules / sandbox / sync / user` 等目录化实现，替换原先的大文件布局
- 双仓库项目读写与快照能力整体下沉到 `src/utils/project_fs/*`，规则、配置、知识库、导入导出、预检和发布都改为复用同一套目录抽象
- 发布链路改为系统感知：设备、发布、回滚、草稿刷新、Gitea 推送与 tag 发布都显式带 `system` 和 `release_group`
- 沙盒链路改为系统感知：工作区准备、命令探测、进程启动、结果归集、阶段输出和结论生成均同时支持 `wparse` 与 `wfusion`
- 前端导航、路由、全局系统上下文、规则管理、配置管理、发布页、调试页、接入概览与运行监控全部接入系统切换能力
- 连接器模板与运行时扫描逻辑升级，输入源 / 输出源展示不再依赖纯前端推断，而是以后端扫描结果为准

## [0.2.8] - 2026-06-26
Commit: `pending`

### Added
- 新增接入概览规则摘要接口，后端可直接返回设备类型与日志类型聚合结果

### Changed
- `Cargo.toml`: version `0.2.7` -> `0.2.8`
- `Cargo.toml`: `zip` 依赖从 `version = "2"` 调整为 `8.6.0`
- `Dockerfile`: 二进制复制改为 `COPY --chown=appuser:appgroup`，去掉额外的 `/app` 递归 `chown -R`
- 接入概览页面改为直接读取后端规则摘要，避免前端逐页枚举 WPL 并逐个加载 `parse.wpl`，显著减少页面打开耗时
- WPL 摘要提取逻辑增强：支持 `#[copy_raw(...), tag(...)]` 等混合注解顺序，并优先使用 `dev_name` 作为设备展示名
- 调试页布局收敛：JSON 模式切换时不再改变整体布局，日志输入框改为固定高度并使用内部滚动
- 补充接入概览规则摘要接口与 WPL 提取逻辑测试

## [0.2.7] - 2026-06-22
Commit: `1d71d47`

### Changed
- `Cargo.toml`: version `0.2.6` -> `0.2.7`
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.22.4` -> `v1.23.2`
- `Cargo.toml`: `wp-knowledge "0.13"` -> `~0.14`，`wp-model-core "0.8.7"` -> `~0.8`，`wp-parse-api "0.10"` -> `~0.10`
- `Cargo.toml`: `wpl` / `wp_primitives` 依赖写法调整为带 `package` 和 `~` 约束的形式
- 代码改动集中在 `src/server/debug.rs`、`src/server/release.rs`、`src/utils/knowledge.rs`、`src/utils/sandbox.rs`
- 前端改动集中在 `web/src/services/config.js`、`web/src/services/release.js`、`web/src/views/pages/system-release/detail.jsx`

## [0.2.6-2] - 2026-06-17
Commit: `e72a713`

### Changed
- `Cargo.toml`: `sea-orm` 增加 `sqlx-sqlite` feature
- 新增接入概览能力的首版后端实现，并接通前端入口与页面展示
- 调整系统设置与数据库初始化相关实现，适配新的运行方式并同步补充测试

## [0.2.6-1] - 2026-06-16
Commit: `72bc910`

### Changed
- `Cargo.toml`: version `0.2.5` -> `0.2.6`

## [0.2.5] - 2026-05-31
Commit: `809dd7f`

### Changed
- `Cargo.toml`: version `0.2.4` -> `0.2.5`

## [0.2.4-2] - 2026-05-31
Commit: `8f303fb`

### Added
- `Cargo.toml`: 新增 `futures-util`、`tempfile`、`tar`、`flate2`、`zip`
- 新增一组默认配置模板文件，覆盖 `default_configs/data/out_dat/*` 与 `default_configs/runtime/*`

### Changed
- 项目导入导出与 Git 冲突处理链路同步更新

## [0.2.4-1] - 2026-05-28
Commit: `eff9c65`

### Changed
- `Cargo.toml`: version `0.2.3` -> `0.2.4`
- `Cargo.toml`: `config "0.15.22"` -> `0.15.23`
- `Cargo.toml`: 新增 `orion-error = "0.8.1"`
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.22.2` -> `v1.22.4`

## [0.2.3] - 2026-05-19
Commit: `ef70716`

### Added
- 新增配置模板接口与模板扫描能力
- 新增一组默认 connector 模板，覆盖 source / sink 的 DMDB 与计数类配置场景

### Changed
- `Cargo.toml`: version `0.2.2` -> `0.2.3`
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.22.1` -> `v1.22.2`
- 配置管理页接入模板能力

## [0.2.2] - 2026-05-14
Commit: `149e3e2`

### Changed
- `Cargo.toml`: version `0.2.1` -> `0.2.2`
- `Cargo.toml`: `cargo_metadata "0.20"` -> `0.23.1`
- `Cargo.toml`: `tokio "1.50"` -> `1.52.3`
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.22.0` -> `v1.22.1`
- `Cargo.toml`: `wp-model-core "0.8"` -> `0.8.7`
- `Cargo.toml`: `nix "0.28"` -> `0.30.1`

## [0.2.1] - 2026-05-12
Commit: `8606a10`

### Changed
- `Cargo.toml`: version `0.2.0` -> `0.2.1`
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.21.11` -> `v1.22.0`
- `src/server/sandbox_runner.rs`、`src/utils/sandbox.rs` 重点改动
- 多个测试文件同步修改，包括 `tests/api/assist_test.rs`、`tests/api/config_test.rs`、`tests/api/debug_test.rs`、`tests/api/release_test.rs`

## [0.2.0] - 2026-05-08
Commit: `02f1cab`

### Changed
- `Cargo.toml`: version `0.1.8` -> `0.2.0`
- `config/config.toml`、`src/server/app.rs`、`src/server/setting.rs`、`src/utils/warparse_service.rs` 同步改动
- `src/server/setting.rs`: `WarparseConf` 新增 `enabled: bool`
- `src/utils/warparse_service.rs`: `WarpParseService` 新增 `scheme` 字段，设备访问地址改为按 `http/https` 组装

## [0.1.8-3] - 2026-05-08
Commit: `f17361b`

### Changed
- `Cargo.toml`: `rust-embed "6.8"` -> `8.11.0`
- `Cargo.toml`: `config "0.14"` -> `0.15.22`
- `Cargo.toml`: `toml "0.8"` -> `1.1.2`
- `Cargo.toml`: `thiserror "1.0"` -> `2.0.18`
- `Cargo.toml`: `strum "0.26"` -> `0.28.0`
- `Cargo.toml`: `bcrypt "0.15"` -> `0.19.0`
- `Cargo.toml`: `rand "0.8"` -> `0.10.1`
- `Cargo.toml`: `which "6.0"` -> `8.0.2`
- `src/server/assist_task.rs`、`src/server/sandbox.rs`、`src/server/user.rs` 同步适配新的 `rand` API

## [0.1.8-2] - 2026-05-05
Commit: `3eb5c7d`

### Changed
- `Cargo.toml`: 增加 `#@gxl:set(version)` 标记
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.21.7` -> `v1.21.11`
- `Cargo.toml`: `wp-knowledge "0.11"` -> `0.13`
- `Cargo.toml`: `wp-parse-api "0.8"` -> `0.10`
- `Cargo.toml`: `wpl "0.1"` -> `0.3`
- `src/utils/knowledge.rs` 改用 `anyhow::Result` 与 `wp_knowledge::mem::RowData`
- 新增 `version.txt`

## [0.1.8-1] - 2026-04-30
Commit: `c5cdb04`

### Added
- 新增 `crates/migrations/src/m20260428_000002_add_release_group.rs`
- 新增 `src/db/release_group.rs`
- 新增多组 `docker/station/default_configs/connectors/sink.d/*`
- 新增 `docker/station/default_configs/connectors/source.d/40-mysql.toml`

### Changed
- `Cargo.toml`: version `0.1.7` -> `0.1.8`
- `src/server/release.rs`、`src/server/release_task_runner.rs`、`src/server/sync.rs`、`src/utils/project.rs` 大幅改动
- `web/src/services/release.js`、`web/src/views/pages/system-release/detail.jsx`、`web/src/views/pages/system-release/index.jsx` 大幅改动

## [0.1.7] - 2026-04-28
Commit: `6b094e1`

### Changed
- `Cargo.toml`: version `1.1.0` -> `0.1.7`
- `src/utils/constants.rs` -> `src/utils/common.rs`
- `src/utils/check.rs` -> `src/utils/project_check.rs`
- `src/utils/sandbox_workspace.rs` -> `src/utils/sandbox.rs`
- `src/utils/process_guard.rs` 被删除

## [1.1.0-6] - 2026-04-23
Commit: `6ef65ce`

### Added
- 新增 `tasks/2026-04-15_1_performance-test-page-design.md`

### Changed
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.20.3` -> `v1.20.6`
- `src/server/debug.rs`、`src/utils/knowledge.rs`、`src/utils/sandbox_workspace.rs`、`web/src/views/pages/simulate-debug/index.jsx` 同步改动

## [1.1.0-5] - 2026-04-22
Commit: `de3d57d`

### Changed
- `Cargo.toml`: `wp_oml` / `wp-proj` / `wp_data_utils` tag `v1.20.0` -> `v1.20.3`
- `src/db/default_rules_loader.rs` 大幅改动，默认配置初始化从“仅嵌入资源”改为“优先运行时目录，缺失时回退嵌入资源”

## [1.1.0-4] - 2026-04-17
Commit: `62ba8a2`

### Changed
- `src/server/device.rs`、`src/utils/warparse_service.rs` 同步改动
- `web/src/services/config.js`、`web/src/views/pages/config-manage/index.jsx`、`web/src/views/pages/system-manage/ConnectionManage.jsx` 同步改动

## [1.1.0-3] - 2026-04-16
Commit: `40d6152`

### Changed
- `AGENTS.md`、`README.md`、`crates/migrations/README.md` 同步改动
- 删除 `crates/migrations/src/entity/knowledge_config.rs`
- 删除 `crates/migrations/src/entity/rule_config.rs`
- 删除 `src/db/knowledge_config.rs`
- 删除 `src/db/rule_config.rs`
- 新增 `src/db/rule_type.rs`
- `src/server/config.rs`、`src/server/rules.rs`、`src/utils/project.rs` 大幅改动

## [1.1.0-2] - 2026-04-13
Commit: `c2340db`

### Added
- 新增 `src/api/project.rs`
- 新增 `src/server/project.rs`
- 新增 `web/src/services/features.js`
- 新增 `web/src/services/project.js`

### Changed
- `src/server/assist_task.rs`、`src/utils/project.rs`、`web/src/views/pages/rule-manage/index.jsx`、`web/src/views/pages/system-manage/index.jsx` 同步改动

## [1.1.0-1] - 2026-04-07
Commit: `de713f8`

### Added
- 首次提交 `Cargo.toml`、`Cargo.lock`、`Dockerfile`、`build.rs`、`config/config.toml`
- 首次提交 `crates/gitea/*`、`crates/migrations/*`
- 首次提交 `default_configs/*`
- 首次提交 `src/api/*`、`src/db/*`、`src/server/*`、`src/utils/*`
- 首次提交 `tests/*`
- 首次提交 `web/src/*`、`web/public/*`、`web/package.json`、`web/package-lock.json`
- 首次提交 `.github/workflows/*`
