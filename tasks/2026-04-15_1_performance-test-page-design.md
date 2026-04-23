# 背景

文件名：2026-04-15_1
创建于：2026-04-15
创建者：wensiwei
主分支：main
任务分支：待定
Yolo 模式：Off

---

# 任务描述

完善调试页中的“性能测试”页面，将当前“样本数据 + TOML 数据生成配置 + 前端模拟结果”的占位实现，改造成可配置、可执行、可刷新结果的真实性能测试能力。

核心需求：

- 数据生成配置改为表单选项，不再让用户直接编辑完整 TOML。
- 测试数量使用输入框，默认 `1W`，后端按 `10000` 处理。
- 左侧输入源下拉框：`文本`、`UDP`、`TCP`、`Kafka`。
- 右侧输出目标下拉框：`文本`、`黑洞`、`Kafka`。
- 选择 Kafka 时必须配置 Kafka 的 `IP/Brokers` 和 `Topic`。
- 点击测试后真实执行：
  - 启动 `wparse daemon --stat 1 -p`
  - 发送 `wpgen sample --stat 1 -p`
- 右侧支持刷新执行结果。
- 优先复用沙盒预发布底层能力，但不要把性能测试强行塞进沙盒业务状态机。

---

# 设计结论

性能测试应做成独立的轻量任务系统：

```
前端性能测试页
    ↓
services/debug.js
    ↓
/api/debug/performance/run
    ↓
src/server/debug.rs 参数校验与任务创建
    ↓
src/server/performance_runner.rs 异步执行
    ↓
临时 workspace + wparse daemon + wpgen sample
    ↓
performance_tasks / performance_results
    ↓
/api/debug/performance/{taskId} 刷新结果
```

复用沙盒的底层工具：

- 临时工作区复制 `project_root`。
- 配置文件覆盖写入。
- 进程启动、日志重定向、daemon 终止。
- `wpgen` 输出解析、运行结果文件检查。

不复用沙盒完整状态机：

- 沙盒绑定发布 ID、预发布阶段、诊断建议、`sandbox_runs` 表。
- 性能测试属于调试页临时任务，不应影响发布的 `sandbox_ready`。
- 复用完整状态机会造成概念污染，也会让后续“停止任务、历史列表、结果刷新”的边界变复杂。

---

# 当前现状

## 前端现状

文件：`web/src/views/pages/simulate-debug/index.jsx`

当前性能测试页存在这些问题：

- 页面只是两列布局：左侧样本数据、TOML 配置，右侧执行结果。
- 点击“测试”只用 `setTimeout` 模拟 2 秒执行。
- 执行结果是前端硬编码字符串。
- 页面没有调用 `web/src/services/debug.js` 中的 `runPerformanceTest`。
- 页面没有 `taskId`、状态刷新、运行中轮询、错误日志展示。

## Service 现状

文件：`web/src/services/debug.js`

已有 `runPerformanceTest(options)`：

- 调用 `POST /debug/performance/run`。
- 当前请求结构是 `{ test_type, config }`。
- 与后端实际 `DebugPerformanceRunRequest { sample, config }` 不一致。

## 后端现状

文件：`src/api/debug.rs`

已有接口：

- `POST /api/debug/performance/run`
- `GET /api/debug/performance/{taskId}`

文件：`src/server/debug.rs`

已有逻辑：

- `debug_performance_run_logic(sample, config)` 只创建 `performance_tasks` 记录。
- 创建后直接返回 `running`，没有启动真实任务。
- `debug_performance_get_logic(task_id)` 从 DB 查询任务与结果。

文件：`src/db/performance.rs`

已有数据层：

- `performance_tasks`：任务主表。
- `performance_results`：sink 结果表。
- 已有创建任务、查询任务、更新状态、添加结果、获取结果函数。
- 缺少更新任务汇总字段的函数，例如 `total_lines`、`duration`、`avg_qps`。

---

# 目标与非目标

## 目标

- 页面配置从“自由编辑 TOML”改为“受控选项 + 条件参数”。
- 用户能明确选择输入源和输出目标。
- 后端按选项生成临时运行配置，不污染 `project_root`。
- 真实启动 `wparse daemon --stat 1 -p`。
- 真实执行 `wpgen sample --stat 1 -p`，并按测试数量发送样本。
- 右侧能手动刷新任务结果。
- 任务执行失败时能展示明确失败原因和关键日志。
- 保留后续扩展空间：停止任务、历史记录、自动轮询、日志下载。

## 非目标

- 不把性能测试结果作为发布前置条件。
- 不更新 `sandbox_ready`。
- 不改规则/配置保存链路。
- 不要求本阶段支持多任务并发压测。
- 不在本阶段做复杂图表，只做清晰结果展示和日志定位。

---

# 用户体验设计

## 页面布局

建议保持调试页现有左右结构，但调整左侧为“配置区”，右侧为“结果区”。

```
┌──────────────────────────────┬────────────────────────────────────┐
│ 性能测试配置                  │ 执行结果                           │
│                              │                                    │
│ 样本数据                      │ 状态卡片                           │
│ [CodeEditor]                 │ running/completed/failed           │
│                              │                                    │
│ 测试数量 [10000]              │ 指标摘要                           │
│                              │ total / duration / qps             │
│ 输入源 [文本 v]               │                                    │
│ Kafka Brokers [条件展示]      │ Sink 结果表                         │
│ Kafka Topic   [条件展示]      │                                    │
│                              │ 日志面板                           │
│ 输出目标 [黑洞 v]             │ daemon / wpgen / analysis           │
│ Kafka Brokers [条件展示]      │                                    │
│ Kafka Topic   [条件展示]      │ [刷新] [查看日志]                   │
│                              │                                    │
│ [开始测试] [重置]             │                                    │
└──────────────────────────────┴────────────────────────────────────┘
```

## 配置区字段

| 字段 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| 样本数据 | CodeEditor | 当前示例日志 | 文本输入源或生成规则模式的基础样本 |
| 测试数量 | InputNumber | `10000` | 页面显示可标注“默认 1W” |
| 输入源 | Select | `文本` | 选项：文本、UDP、TCP、Kafka |
| 输入 Kafka Brokers | Input | 空 | 仅输入源为 Kafka 时展示 |
| 输入 Kafka Topic | Input | 空 | 仅输入源为 Kafka 时展示 |
| 输出目标 | Select | `黑洞` | 选项：文本、黑洞、Kafka |
| 输出 Kafka Brokers | Input | 空 | 仅输出目标为 Kafka 时展示 |
| 输出 Kafka Topic | Input | 空 | 仅输出目标为 Kafka 时展示 |

## 默认值建议

- 测试数量：`10000`
- 输入源：`UDP`
- 输出目标：`黑洞`

原因：

- `UDP → 黑洞` 最适合作为基础性能测试路径。
- `黑洞` 能减少磁盘 IO 对解析性能结果的干扰。
- 当前沙盒已有 UDP 运行时常量和端口检测逻辑，落地成本最低。

如果更贴近用户输入，可以把输入源默认设为 `文本`，但第一版建议用 `UDP`，因为 `wpgen sample` 更容易直接向 daemon source 发压测流量。

## 结果区展示

状态卡片：

| 状态 | 展示 |
|------|------|
| `idle` | 尚未开始测试 |
| `running` | 运行中，展示 taskId、开始时间、已运行时长 |
| `completed` | 完成，展示耗时、总量、平均 QPS |
| `failed` | 失败，展示错误摘要和日志入口 |
| `stopped` | 已停止，后续如支持停止任务再启用 |

指标摘要：

- `测试数量`
- `实际发送数量`
- `成功输出数量`
- `耗时`
- `平均 QPS`
- `wpgen exit code`
- `wparse exit/status`

结果表：

| Sink | Lines | QPS | Status | Error |
|------|-------|-----|--------|-------|
| all_sink | 10000 | 8000 | OK | - |
| error | 0 | - | OK | - |
| miss | 0 | - | OK | - |

日志面板：

- 默认折叠。
- 失败时自动展开错误摘要。
- 日志类型：
  - `daemon.log`
  - `wpgen.log`
  - `analysis.log`
  - `workspace.log`

## 按钮行为

开始测试：

- 校验表单。
- 禁用按钮，防止重复提交。
- 调用 `runPerformanceTest`。
- 保存返回的 `taskId`。
- 立即刷新一次结果。
- 可选自动轮询，直到 `completed/failed/stopped`。

刷新：

- 只有存在 `taskId` 时可用。
- 调用 `GET /api/debug/performance/{taskId}`。
- 不重新启动任务。

重置：

- 清空 `taskId` 和结果。
- 保留用户当前配置。
- 样本数据不强制恢复默认，避免误删用户输入。

---

# 前端改造方案

## 修改文件

主要修改：

- `web/src/views/pages/simulate-debug/index.jsx`
- `web/src/services/debug.js`
- `web/src/styles/theme.css`
- `web/src/i18n/locales/zh-CN.json`
- `web/src/i18n/locales/en-US.json`

不要修改：

- `web/src/views/pages/simulate-debug/index-old.jsx`
- `web/src/views/pages/simulate-debug/index-backup.jsx`
- `web/dist/*`

## 前端状态设计

新增或替换现有性能测试状态：

```js
const [performanceForm, setPerformanceForm] = useState({
  sample: EXAMPLE_LOG,
  count: 10000,
  inputType: 'udp',
  outputType: 'blackhole',
  inputKafka: {
    brokers: '',
    topic: '',
  },
  outputKafka: {
    brokers: '',
    topic: '',
  },
});

const [performanceTaskId, setPerformanceTaskId] = useState('');
const [performanceStatus, setPerformanceStatus] = useState('idle');
const [performanceResult, setPerformanceResult] = useState(null);
const [performanceError, setPerformanceError] = useState('');
const [performanceLoading, setPerformanceLoading] = useState(false);
const [performanceRefreshing, setPerformanceRefreshing] = useState(false);
```

说明：

- 不再保留 `performanceConfig` 的 TOML 编辑状态。
- `performanceSample` 可合并到 `performanceForm.sample`。
- loading 不建议继续复用页面全局 `loading`，否则可能影响解析、转换、知识库等其他 tab。

## Service 设计

`runPerformanceTest` 入参改为完整表单：

```js
export async function runPerformanceTest(options) {
  return httpRequest.post('/debug/performance/run', {
    sample: options.sample,
    count: options.count,
    input_type: options.inputType,
    output_type: options.outputType,
    input_kafka: options.inputKafka,
    output_kafka: options.outputKafka,
  });
}
```

新增刷新接口：

```js
export async function fetchPerformanceTest(taskId) {
  return httpRequest.get(`/debug/performance/${taskId}`);
}
```

后续可新增停止接口：

```js
export async function stopPerformanceTest(taskId) {
  return httpRequest.post(`/debug/performance/${taskId}/stop`);
}
```

## 前端校验

开始测试前校验：

- `count` 必须是 `1 <= count <= 10000000` 的整数。
- `sample` 在输入源为 `文本` 时不能为空。
- 输入源为 `Kafka` 时，`inputKafka.brokers` 和 `inputKafka.topic` 必填。
- 输出目标为 `Kafka` 时，`outputKafka.brokers` 和 `outputKafka.topic` 必填。
- Kafka topic 只允许非空字符串，是否限制字符集由后端最终校验。

## 自动轮询策略

第一版建议同时支持“手动刷新”和“运行中自动轮询”：

- 点击开始后立即获取一次结果。
- `running` 状态下每 2 秒轮询。
- 页面离开性能测试 tab 时停止轮询。
- 任务进入 `completed/failed/stopped` 后停止轮询。
- 右侧仍保留“刷新”按钮，方便用户手动拉取最新结果。

如果第一阶段希望降低实现复杂度，可以只做手动刷新，但体验会弱一些。

---

# API 设计

## 启动性能测试

路径：

```http
POST /api/debug/performance/run
```

请求：

```json
{
  "sample": "222.133.52.20 - - ...",
  "count": 10000,
  "input_type": "udp",
  "output_type": "blackhole",
  "input_kafka": {
    "brokers": "127.0.0.1:9092",
    "topic": "wp-input"
  },
  "output_kafka": {
    "brokers": "127.0.0.1:9092",
    "topic": "wp-output"
  }
}
```

字段说明：

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `sample` | string | 条件必填 | 文本样本；输入源为文本时必填 |
| `count` | number | 是 | 测试数量 |
| `input_type` | string | 是 | `text` / `udp` / `tcp` / `kafka` |
| `output_type` | string | 是 | `text` / `blackhole` / `kafka` |
| `input_kafka` | object | 条件必填 | 输入源为 Kafka 时必填 |
| `output_kafka` | object | 条件必填 | 输出目标为 Kafka 时必填 |

响应：

```json
{
  "task_id": "perf-1770000000000",
  "status": "running"
}
```

## 查询性能测试

路径：

```http
GET /api/debug/performance/{taskId}
```

响应：

```json
{
  "task_id": "perf-1770000000000",
  "status": "completed",
  "start_time": "2026-04-15T10:00:00Z",
  "end_time": "2026-04-15T10:00:12Z",
  "summary": {
    "requested_count": 10000,
    "generated_count": 10000,
    "output_count": 10000,
    "duration_ms": 12000,
    "avg_qps": 833,
    "wpgen_exit_code": 0
  },
  "sinks": [
    {
      "name": "all_sink",
      "lines": 10000,
      "qps": 833,
      "status": "ok",
      "error_message": null
    }
  ],
  "logs": {
    "daemon": "tmp/performance/perf-1770000000000/logs/daemon.log",
    "wpgen": "tmp/performance/perf-1770000000000/logs/wpgen.log",
    "analysis": "tmp/performance/perf-1770000000000/logs/analysis.log"
  },
  "error": null
}
```

兼容性说明：

- 现有 `DebugPerformanceGetResponse` 只有 `total_lines/duration/avg_qps`，可以增量扩展。
- 前端显示时要兼容旧字段为空的情况。

## 获取日志

第一版可不新增日志接口，只在查询接口返回日志摘要或相对路径。

更完整方案建议新增：

```http
GET /api/debug/performance/{taskId}/logs/{name}
```

其中 `name` 支持：

- `daemon`
- `wpgen`
- `analysis`
- `workspace`

返回：

```json
{
  "task_id": "perf-1770000000000",
  "name": "daemon",
  "content": "...",
  "truncated": true,
  "total_lines": 1500
}
```

日志默认最多返回 500 行，避免大日志撑爆页面。

## 停止任务

第一版可暂不实现。后续建议增加：

```http
POST /api/debug/performance/{taskId}/stop
```

行为：

- 如果任务运行中，终止 daemon/wpgen。
- 任务状态置为 `stopped`。
- 已完成/失败任务重复 stop 返回当前状态。

---

# 后端数据结构

## 请求结构

替换当前 `DebugPerformanceRunRequest`：

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DebugPerformanceRunRequest {
    pub sample: Option<String>,
    pub count: u32,
    pub input_type: PerformanceInputType,
    pub output_type: PerformanceOutputType,
    #[serde(default)]
    pub input_kafka: Option<KafkaEndpointConfig>,
    #[serde(default)]
    pub output_kafka: Option<KafkaEndpointConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceInputType {
    Text,
    Udp,
    Tcp,
    Kafka,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceOutputType {
    Text,
    Blackhole,
    Kafka,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct KafkaEndpointConfig {
    pub brokers: String,
    pub topic: String,
}
```

## Runner 参数

```rust
#[derive(Debug, Clone)]
pub struct PerformanceRunOptions {
    pub task_id: String,
    pub sample: Option<String>,
    pub count: u32,
    pub input_type: PerformanceInputType,
    pub output_type: PerformanceOutputType,
    pub input_kafka: Option<KafkaEndpointConfig>,
    pub output_kafka: Option<KafkaEndpointConfig>,
    pub daemon_ready_timeout_ms: u64,
    pub wpgen_timeout_ms: u64,
    pub collect_wait_ms: u64,
    pub keep_workspace: bool,
}
```

默认值：

| 字段 | 默认 |
|------|------|
| `daemon_ready_timeout_ms` | `30000` |
| `wpgen_timeout_ms` | `600000` |
| `collect_wait_ms` | `3000` |
| `keep_workspace` | 失败保留，成功可清理 |

## DB 字段策略

当前 `performance_tasks` 字段可先复用：

| 字段 | 用法 |
|------|------|
| `task_id` | 外部任务 ID |
| `status` | `running/completed/failed/stopped` |
| `sample_data` | 保存 sample |
| `config_content` | 保存本次表单配置序列化 JSON |
| `start_time` | 开始时间 |
| `end_time` | 结束时间 |
| `total_lines` | 实际处理/输出数量 |
| `duration` | 总耗时，当前类型是 `i32`，建议继续保存秒数或迁移为毫秒 |
| `avg_qps` | 平均 QPS |

建议新增或扩展：

- `error_message`：任务失败原因。
- `requested_count`：用户请求数量。
- `generated_count`：wpgen 实际生成数量。
- `workspace_path`：保留 workspace 时方便读取日志。

如果不想第一版加迁移：

- `config_content` 保存 JSON，包含 `requested_count`、输入输出类型、Kafka 配置、日志相对路径。
- `performance_results.error_message` 保存 sink 级错误。
- 任务级错误暂时写入 `analysis.log`，查询接口从日志摘要中返回。

---

# 后端模块设计

## 新增模块

建议新增：

- `src/server/performance_runner.rs`
- `src/utils/performance_workspace.rs` 或抽象通用 workspace 工具

`src/server/debug.rs` 只负责：

- DTO。
- 参数校验。
- 创建任务。
- 调用 runner spawn。
- 查询任务和格式化响应。

`performance_runner.rs` 负责：

- 准备临时工作区。
- 生成 runtime 配置。
- 启动 daemon。
- 执行 wpgen。
- 收集和分析结果。
- 更新 DB。
- 清理资源。

## Runner 生命周期

```
1. mark running
2. prepare workspace
3. write sample/config overrides
4. preflight check
5. spawn wparse daemon --stat 1 -p
6. wait daemon ready
7. run wpgen sample --stat 1 -p
8. wait collect window
9. analyse output/logs
10. write performance_results
11. update performance_tasks completed/failed
12. terminate daemon
13. cleanup workspace
```

## 任务状态机

| 状态 | 说明 |
|------|------|
| `running` | 任务已创建并在后台执行 |
| `completed` | 执行完成且结果收集成功 |
| `failed` | 执行失败或结果分析失败 |
| `stopped` | 用户主动停止 |

第一版可以只实现 `running/completed/failed`。

---

# 沙盒能力复用设计

## 可复用能力

| 现有位置 | 可复用内容 |
|----------|------------|
| `src/utils/sandbox_workspace.rs` | 复制 `project_root`、写覆盖文件、日志路径管理、目录树渲染 |
| `src/utils/process_guard.rs` | 启动 daemon、运行 wpgen、命令版本检查、端口检查、进程终止 |
| `src/server/sandbox_analyzer.rs` | 解析 wpgen 生成数量、分析输出文件、形成结果摘要 |
| `src/utils/constants.rs` | UDP 默认端口、输出路径、sink fallback 语义 |

## 需要抽象或扩展

### Workspace

当前 `SandboxWorkspace::prepare` 会强制调用 `ensure_sandbox_runtime_configs`，写死 UDP source 和 UDP wpgen 配置。

性能测试不能直接复用这个方法，否则无法支持文本、TCP、Kafka、黑洞输出。

建议抽象：

```rust
pub struct RuntimeWorkspace {
    pub root: PathBuf,
    pub project_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub source_project_root: PathBuf,
}

impl RuntimeWorkspace {
    pub fn prepare(kind: &str, task_id: &str, overrides: &[FileOverride]) -> Result<Self, AppError>;
}
```

其中：

- 沙盒使用 `kind = "sandbox"`。
- 性能测试使用 `kind = "performance"`.
- 是否写入固定运行时配置由调用方决定。

如果不做通用抽象，也可以新增 `PerformanceWorkspace`，复制少量逻辑，但后续维护成本会增加。

### process_guard

当前：

- `spawn_daemon(project_dir, log_path)` 执行 `wparse daemon`。
- `run_wpgen(project_dir, log_path, sample_count, timeout)` 执行 `wpgen sample -w . -n count --print_stat`。

需要扩展为可传参数：

```rust
pub async fn spawn_daemon_with_args(
    project_dir: &Path,
    log_path: &Path,
    args: &[&str],
) -> Result<DaemonProcess, AppError>;

pub async fn run_wpgen_with_args(
    project_dir: &Path,
    log_path: &Path,
    args: &[String],
    timeout: Duration,
) -> Result<WpgenOutput, AppError>;
```

沙盒原函数保留，避免行为回归：

```rust
pub async fn spawn_daemon(...) {
    spawn_daemon_with_args(project_dir, log_path, &["daemon"]).await
}

pub async fn run_wpgen(...) {
    run_wpgen_with_args(project_dir, log_path, &[
        "sample", "-w", ".", "-n", count, "--print_stat"
    ], timeout).await
}
```

性能测试使用：

```text
wparse daemon --stat 1 -p
wpgen sample --stat 1 -p
```

注意：如果 `wpgen sample --stat 1 -p` 仍需要显式数量参数，应保留 `-n {count}` 或使用工具支持的等价参数。实现前需要通过本地 `wpgen sample --help` 确认最终参数组合。

---

# 运行配置生成

性能测试不能修改真实 `project_root`，必须只覆盖临时 workspace。

## 输入源映射

| 页面选项 | 内部值 | source connect | 说明 |
|----------|--------|----------------|------|
| 文本 | `text` | `file_src` | 从临时文件读取样本 |
| UDP | `udp` | `syslog_udp_src` | 本机端口接收 wpgen 发送 |
| TCP | `tcp` | `syslog_tcp_src` 或 `tcp_src` | 本机端口接收 TCP |
| Kafka | `kafka` | `kafka_src` | 从指定 Kafka topic 读取 |

## 输出目标映射

| 页面选项 | 内部值 | sink connect | 说明 |
|----------|--------|--------------|------|
| 文本 | `text` | `file_json_sink` 或 `file_raw_sink` | 写入临时输出文件 |
| 黑洞 | `blackhole` | `blackhole_sink` | 丢弃输出，用于纯性能测试 |
| Kafka | `kafka` | `kafka_sink` | 写入指定 Kafka topic |

## 文本输入配置

文本输入建议流程：

- 将用户样本写入临时文件，例如 `data/in_dat/gen.dat`。
- 如果 `count > 1`，可写入多行，或者交给 `wpgen sample` 按规则生成。
- `topology/sources/wpsrc.toml` 启用 `gen_file`，禁用 UDP/TCP/Kafka。

示例：

```toml
[[sources]]
key = "gen_file"
enable = true
connect = "file_src"

[sources.params]
encode = "text"
file = "gen.dat"
```

## UDP 输入配置

示例：

```toml
[[sources]]
key = "gen_udp"
enable = true
connect = "syslog_udp_src"

[sources.params]
addr = "0.0.0.0"
port = 31601
protocol = "udp"
header_mode = "strip"
```

## TCP 输入配置

示例：

```toml
[[sources]]
key = "gen_tcp"
enable = true
connect = "syslog_tcp_src"

[sources.params]
addr = "0.0.0.0"
port = 31601
protocol = "tcp"
header_mode = "strip"
prefer_newline = true
tcp_recv_bytes = 10485760
```

如果实际项目中使用 `tcp_src` 而不是 `syslog_tcp_src`，应按现有 connector 文件决定，不在页面层硬编码。

## Kafka 输入配置

示例：

```toml
[[sources]]
key = "gen_kafka"
enable = true
connect = "kafka_src"

[sources.params]
brokers = "127.0.0.1:9092"
topic = "wp-input"
```

## 文本输出配置

示例：

```toml
version = "1.0"

[sink_group]
name = "performance_text_sink"
oml = ["*"]
parallel = 1

[[sink_group.sinks]]
name = "all_sink"
connect = "file_json_sink"
tags = []

[sink_group.sinks.params]
base = "./data/out_dat/"
file = "all.json"
```

## 黑洞输出配置

示例：

```toml
version = "1.0"

[sink_group]
name = "performance_blackhole_sink"
oml = ["*"]
parallel = 1

[[sink_group.sinks]]
name = "blackhole"
connect = "blackhole_sink"
tags = []
```

## Kafka 输出配置

示例：

```toml
version = "1.0"

[sink_group]
name = "performance_kafka_sink"
oml = ["*"]
parallel = 1

[[sink_group.sinks]]
name = "kafka_sink"
connect = "kafka_sink"
tags = []

[sink_group.sinks.params]
brokers = "127.0.0.1:9092"
topic = "wp-output"
```

## wpgen 配置

`conf/wpgen.toml` 需要根据输入源决定输出到哪里。

UDP 示例：

```toml
version = "1.0"

[generator]
mode = "rule"
count = 10000
speed = 0
parallel = 1

[output]
connect = "syslog_udp_sink"

[output.params]
addr = "127.0.0.1"
port = 31601
protocol = "udp"

[logging]
level = ""
module_levels = []
output = ""
file_path = "./data/logs"

[presets]
```

TCP 示例：

```toml
[output]
connect = "syslog_tcp_sink"

[output.params]
addr = "127.0.0.1"
port = 31601
protocol = "tcp"
```

Kafka 示例：

```toml
[output]
connect = "kafka_sink"

[output.params]
brokers = "127.0.0.1:9092"
topic = "wp-input"
```

文本输入模式需要单独确认 `wpgen sample` 是否仍参与发送。如果文本模式表示“直接读取文件作为 source”，则不需要 wpgen 发送到网络端口；但用户需求明确点击测试要发送 `wpgen sample --stat 1 -p`，所以第一版建议所有模式都生成 `wpgen.toml`，文本模式作为“样本生成来源”，而不是绕过 wpgen。

---

# 命令执行设计

## wparse daemon

命令：

```bash
wparse daemon --stat 1 -p
```

要求：

- 工作目录必须是临时 workspace 的 `project` 目录。
- stdout/stderr 写入 `daemon.log`。
- 需要等待 ready marker。
- 如果 ready marker 现有逻辑不适配 `--stat` 输出，要调整为多 marker 判断。

可能的 ready 判断：

- 日志出现 `ready`
- 日志出现 `listening`
- 端口可连接或 UDP 端口被占用
- daemon 进程仍存活并等待固定启动时间

第一版可以沿用沙盒现有 marker，如果失败率高再增强。

## wpgen sample

命令：

```bash
wpgen sample --stat 1 -p
```

需要确认是否追加数量参数：

```bash
wpgen sample --stat 1 -p -n 10000
```

或：

```bash
wpgen sample --stat 1 -p --count 10000
```

设计上不要把数量只写入 TOML 后假定生效，runner 应明确保证用户输入的 `count` 传递到 wpgen 或写入 `conf/wpgen.toml`。

stdout/stderr 写入 `wpgen.log`。

## 超时

默认策略：

- daemon ready 超时：30 秒。
- wpgen 执行超时：按 count 动态计算，最低 60 秒，最高 10 分钟。
- 结果收集等待：3 秒。

动态 wpgen 超时示例：

```text
timeout_ms = clamp(60000 + count / 1000 * 1000, 60000, 600000)
```

---

# 结果分析设计

## 数据来源

优先级：

1. `wpgen.log` 中的 generated/count/stat 输出。
2. `daemon.log` 中的 stat 输出。
3. 输出文件行数，例如 `data/out_dat/all.json`。
4. `performance_results` 表中的 sink 结果。

## 指标计算

| 指标 | 来源 |
|------|------|
| requested_count | 请求参数 |
| generated_count | wpgen 日志 |
| output_count | 输出文件行数或 daemon stat |
| duration_ms | 任务开始到结束时间 |
| avg_qps | `generated_count / duration_seconds` |
| sink lines | 输出文件行数 |
| error_count | `data/out_dat/error.dat` 行数或 daemon ERROR 统计 |
| miss_count | `data/out_dat/miss.dat` 行数 |

## 成功判定

第一版建议：

- `wpgen` exit code 为 0。
- `wparse daemon` 未异常退出。
- `generated_count >= requested_count` 或能解析到等于请求量。
- 如果输出目标为 `黑洞`，不强制要求输出文件行数等于 count。
- 如果输出目标为 `文本`，要求输出文件存在且行数大于 0，是否必须等于 count 取决于规则是否过滤。
- 如果输出目标为 `Kafka`，第一版只要 sink 无错误即可，不强制消费端校验。

失败判定：

- 命令不存在。
- `wproj check` 失败。
- 端口不可用。
- Kafka 参数缺失或连接失败。
- daemon ready 超时。
- wpgen 超时或非 0 退出。
- 日志包含明确 panic/error 且无法降级。

---

# 错误处理

## 前端错误

表单错误：

- 显示在字段下方。
- 不发请求。

任务启动错误：

- 页面右侧显示错误卡片。
- 保留用户配置。

刷新错误：

- 不清空旧结果。
- 显示“刷新失败，请稍后重试”。

## 后端错误码建议

| code | 场景 |
|------|------|
| `PERFORMANCE_INVALID_COUNT` | 测试数量非法 |
| `PERFORMANCE_KAFKA_REQUIRED` | Kafka 配置缺失 |
| `PERFORMANCE_WORKSPACE_FAILED` | 准备工作区失败 |
| `PERFORMANCE_PREFLIGHT_FAILED` | 命令或配置检查失败 |
| `PERFORMANCE_PORT_UNAVAILABLE` | 端口不可用 |
| `PERFORMANCE_DAEMON_START_FAILED` | daemon 启动失败 |
| `PERFORMANCE_DAEMON_READY_TIMEOUT` | daemon 就绪超时 |
| `PERFORMANCE_WPGEN_FAILED` | wpgen 执行失败 |
| `PERFORMANCE_ANALYSIS_FAILED` | 结果分析失败 |

日志规范：

- 使用中文日志。
- 不打印 Kafka 密码、Token、认证头。
- 不打印完整大样本，只记录长度和前若干字符摘要。
- 错误日志必须带 `task_id` 和原因。

---

# 并发与资源控制

## 并发限制

第一版建议全局只允许一个性能测试任务运行。

原因：

- 默认端口可能冲突。
- 同时跑多个 `wparse daemon` 会争抢 CPU 和磁盘。
- 页面当前也只展示一个任务结果。

实现方式：

- 内存态 `PerformanceState` 保存 running task。
- 或查询 DB 是否存在 `running` 状态任务。
- 如果已有运行中任务，返回 409 或直接返回当前 running task。

建议返回：

```json
{
  "success": false,
  "error": {
    "code": "PERFORMANCE_TASK_RUNNING",
    "message": "已有性能测试任务正在执行，请等待完成或停止后重试"
  }
}
```

## 端口策略

当前沙盒使用固定 UDP 端口 `31601`。

性能测试选择：

- 第一版复用 `31601`，启动前检测端口。
- 后续支持动态端口，避免沙盒和性能测试互相冲突。

如果沙盒任务正在运行，性能测试应提示端口占用，不能强行终止沙盒 daemon。

## 清理策略

- 成功任务：可清理 `project` 目录，保留 `logs`。
- 失败任务：保留完整 workspace，便于排查。
- 定时清理超过 N 天的 `tmp/performance/*`。

---

# 安全与边界

- Kafka brokers/topic 只作为配置写入临时 workspace，不写入真实 `project_root`。
- 不允许用户传任意命令参数。
- count 设置上限，避免误触发超大压测。
- 日志读取接口必须限制在当前 task workspace 内，防止路径穿越。
- 所有覆盖文件路径由后端固定生成，不接受前端传文件路径。
- 不在操作日志中记录完整样本和 Kafka 敏感配置。

---

# 国际化文案

新增中文文案示例：

```json
{
  "simulateDebug": {
    "performance": {
      "configTitle": "性能测试配置",
      "sampleData": "样本数据",
      "testCount": "测试数量",
      "testCountHint": "默认 1W",
      "inputType": "输入源",
      "outputType": "输出目标",
      "inputText": "文本",
      "inputUdp": "UDP",
      "inputTcp": "TCP",
      "inputKafka": "Kafka",
      "outputText": "文本",
      "outputBlackhole": "黑洞",
      "outputKafka": "Kafka",
      "kafkaBrokers": "Kafka IP/Brokers",
      "kafkaTopic": "Topic",
      "start": "开始测试",
      "refresh": "刷新结果",
      "reset": "重置",
      "running": "执行中",
      "completed": "执行完成",
      "failed": "执行失败",
      "emptyResult": "点击开始测试后查看执行结果"
    }
  }
}
```

英文文案同步添加，不允许只改中文。

---

# 验收标准

## UI 验收

- 性能测试页面不再展示 TOML 配置编辑器。
- 测试数量默认显示 `10000` 或 `1W`，实际提交为 `10000`。
- 输入源下拉包含 `文本/UDP/TCP/Kafka`。
- 输出目标下拉包含 `文本/黑洞/Kafka`。
- 选择 Kafka 后展示 brokers 和 topic 输入框。
- 非 Kafka 时不展示 Kafka 配置项。
- 点击开始后按钮进入 loading，不允许重复点击。
- 右侧能看到 `taskId`、状态、开始时间。
- 点击刷新能更新结果。
- 失败时能看到明确错误原因。

## API 验收

- `POST /api/debug/performance/run` 能接受新请求结构。
- 参数非法时返回统一 `AppError` JSON。
- 创建任务后立即返回 `task_id`。
- `GET /api/debug/performance/{taskId}` 能查询任务状态和结果。
- 不存在的 `taskId` 返回 NotFound。

## 执行验收

- 后端真实启动 `wparse daemon --stat 1 -p`。
- 后端真实执行 `wpgen sample --stat 1 -p`。
- 命令 stdout/stderr 写入日志文件。
- 任务完成后 daemon 被终止。
- 成功任务状态变为 `completed`。
- 失败任务状态变为 `failed`，并能查看日志摘要。

## 回归验收

- 沙盒预发布功能不受影响。
- 原有 `process_guard::spawn_daemon` 和 `run_wpgen` 行为不变。
- `simulate-debug/index-old.jsx` 和 `index-backup.jsx` 未修改。
- `web/dist` 未作为源码修改。

---

# 测试建议

## 前端

```bash
cd web
npm run build
```

重点手动验证：

- 下拉切换。
- Kafka 条件字段。
- 表单校验。
- 开始测试 loading。
- 刷新结果。
- 结果区域横向/纵向滚动。

## 后端

```bash
cargo clippy --lib -- -D warnings
```

建议新增单元测试：

- 参数校验。
- Kafka 条件必填。
- count 上限。
- workspace 配置生成。
- process_guard 参数拼接。

如果本地有 `wparse/wpgen`：

```bash
cargo run
```

然后在页面手动执行：

- `UDP → 黑洞`
- `UDP → 文本`
- `Kafka → 黑洞`
- `UDP → Kafka`

---

# 分阶段实现计划

## 第一阶段：真实任务跑通

- 扩展 `DebugPerformanceRunRequest`。
- 新增 `performance_runner.rs`。
- 扩展 `process_guard` 支持自定义参数。
- 准备临时 workspace。
- 支持 `UDP → 黑洞`。
- 页面接真实 API。
- 右侧支持刷新结果。

## 第二阶段：补齐输入输出类型

- 支持 `文本` 输入。
- 支持 `TCP` 输入。
- 支持 `Kafka` 输入。
- 支持 `文本` 输出。
- 支持 `Kafka` 输出。
- 完善配置模板和校验。

## 第三阶段：体验增强

- 运行中自动轮询。
- 日志查看接口。
- 停止任务。
- 历史记录。
- 结果图表。
- 动态端口。
- workspace 清理任务。

---

# 待确认问题

1. `wpgen sample --stat 1 -p` 是否还需要显式追加 `-n 10000` 或其他 count 参数？
2. “文本”输入到底表示读取用户填写的样本文件，还是表示 wpgen 以文本 connector 发送？
3. “文本”输出期望格式是 raw text、JSON，还是 proto-text？
4. Kafka brokers 是否只需要 `ip:port`，还是支持多个 broker，例如 `10.0.0.1:9092,10.0.0.2:9092`？
5. 第一版是否允许性能测试和沙盒预发布同时运行？如果不允许，需要统一任务锁。
6. 成功判定是否要求输出数量等于测试数量？对于黑洞和 Kafka 输出，这个条件可能不可靠。
7. 是否需要把性能测试任务写操作日志？当前调试查询一般不写操作日志，但真实性能测试会消耗资源，可以考虑记录。

