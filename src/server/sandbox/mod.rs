//! 沙盒预发布任务模型与内存调度器。

pub mod analyze;
mod control;
pub mod diagnostics;
mod progress;
mod query;
pub mod runner;
mod steps;

use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::{RngExt, distr::Alphanumeric};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::db::update_sandbox_run_record;
use crate::error::AppError;
use crate::utils::SystemKind;

pub use self::control::{create_sandbox_run_logic, stop_sandbox_run_logic};
pub use self::query::{
    get_latest_sandbox_run_logic, get_sandbox_run_logic, get_stage_logs_logic,
    list_sandbox_history_logic,
};
use self::runner as sandbox_runner;

/// 沙盒任务的内存调度器，负责串行执行与排队。
#[derive(Clone)]
pub struct SandboxState {
    pub(crate) mutex: Arc<tokio::sync::Mutex<()>>,
    current: Arc<RwLock<Option<Arc<SandboxTaskHandle>>>>,
    queued: Arc<RwLock<Option<Arc<SandboxTaskHandle>>>>,
}

impl SandboxState {
    /// 创建空的沙盒任务队列。
    pub fn new() -> Self {
        SandboxState {
            mutex: Arc::new(tokio::sync::Mutex::new(())),
            current: Arc::new(RwLock::new(None)),
            queued: Arc::new(RwLock::new(None)),
        }
    }

    /// 将任务加入队列；若没有在运行的任务，则立即开始执行。
    pub async fn enqueue_task(
        &self,
        task: Arc<SandboxTaskHandle>,
    ) -> Result<QueuePlacement, AppError> {
        let became_current = {
            let mut current = self.current.write().await;
            if current.is_none() {
                *current = Some(task.clone());
                true
            } else {
                false
            }
        };
        if became_current {
            let snapshot = task.snapshot().await;
            info!(
                "沙盒任务就绪: task_id={}, release_id={}, placement=immediate",
                snapshot.task_id, snapshot.release_id
            );
            return Ok(QueuePlacement::Immediate);
        }

        let mut queued = self.queued.write().await;
        if queued.is_some() {
            warn!(
                "沙盒任务排队失败: task_id={}, reason=queue_full",
                task.task_id()
            );
            return Err(AppError::too_many_requests_with_code(
                "SANDBOX_QUEUE_FULL",
                "已有沙盒任务在等待执行，请稍后重试",
            ));
        }
        *queued = Some(task.clone());
        let snapshot = task.snapshot().await;
        info!(
            "沙盒任务进入等待队列: task_id={}, release_id={}",
            snapshot.task_id, snapshot.release_id
        );
        Ok(QueuePlacement::Waiting)
    }

    /// 执行任务完成后的善后逻辑，包括落库与调度下一条任务。
    pub async fn on_task_completed(&self, handle: Arc<SandboxTaskHandle>) {
        let snapshot = handle.snapshot().await;
        let persist_result = update_sandbox_run_record(&snapshot).await;
        if let Err(err) = &persist_result {
            warn!("落库沙盒结果失败: {}", err);
        } else {
            info!(
                "沙盒任务完成: task_id={}, release_id={}, status={}",
                snapshot.task_id,
                snapshot.release_id,
                snapshot.status.as_str()
            );
        }
        {
            let mut current = self.current.write().await;
            if current
                .as_ref()
                .map(|existing| Arc::ptr_eq(existing, &handle))
                .unwrap_or(false)
            {
                *current = None;
            }
        }

        if let Some(next_task) = self.take_next_queued().await {
            {
                let mut current = self.current.write().await;
                *current = Some(next_task.clone());
            }
            info!("沙盒任务出队执行: task_id={}", next_task.task_id());
            sandbox_runner::spawn_sandbox_execution(self.clone(), next_task);
        }
    }

    /// 查询指定 ID 的任务快照（优先返回内存中的状态）。
    pub async fn snapshot_by_id(&self, task_id: &str) -> Option<SandboxRun> {
        if let Some(run) = self.current_snapshot(task_id).await {
            return Some(run);
        }
        if let Some(run) = self.queued_snapshot(task_id).await {
            return Some(run);
        }
        None
    }

    /// 返回指定发布最近一次在内存中存在的任务，用于实时状态展示。
    pub async fn latest_for_release(&self, release_id: i32) -> Option<SandboxRun> {
        let mut candidates: Vec<SandboxRun> = Vec::new();

        if let Some(run) = self.current_snapshot_by_release(release_id).await {
            candidates.push(run);
        }
        if let Some(run) = self.queued_snapshot_by_release(release_id).await {
            candidates.push(run);
        }
        candidates.into_iter().max_by_key(|run| {
            run.ended_at
                .or(run.started_at)
                .unwrap_or(run.created_at)
                .timestamp_millis()
        })
    }

    /// 返回指定发布当前在运行或排队的任务列表。
    pub async fn active_runs_for_release(&self, release_id: i32) -> Vec<SandboxRun> {
        let mut runs = Vec::new();
        if let Some(run) = self.current_snapshot_by_release(release_id).await {
            runs.push(run);
        }
        if let Some(run) = self.queued_snapshot_by_release(release_id).await
            && runs.iter().all(|item| item.task_id != run.task_id)
        {
            runs.push(run);
        }
        runs
    }

    /// 如果排队中的任务匹配，则返回其句柄用于终止。
    pub async fn stop_queued_task(&self, task_id: &str) -> Option<Arc<SandboxTaskHandle>> {
        let mut queued = self.queued.write().await;
        if let Some(existing) = queued.as_ref()
            && existing.task_id() == task_id
        {
            return queued.take();
        }
        None
    }

    async fn current_snapshot(&self, task_id: &str) -> Option<SandboxRun> {
        let current = self.current.read().await;
        if let Some(task) = current.as_ref()
            && task.task_id() == task_id
        {
            return Some(task.snapshot().await);
        }
        None
    }

    async fn queued_snapshot(&self, task_id: &str) -> Option<SandboxRun> {
        let queued = self.queued.read().await;
        if let Some(task) = queued.as_ref()
            && task.task_id() == task_id
        {
            return Some(task.snapshot().await);
        }
        None
    }

    async fn current_snapshot_by_release(&self, release_id: i32) -> Option<SandboxRun> {
        let current = self.current.read().await;
        if let Some(task) = current.as_ref() {
            let run = task.snapshot().await;
            if run.release_id == release_id {
                return Some(run);
            }
        }
        None
    }

    async fn queued_snapshot_by_release(&self, release_id: i32) -> Option<SandboxRun> {
        let queued = self.queued.read().await;
        if let Some(task) = queued.as_ref() {
            let run = task.snapshot().await;
            if run.release_id == release_id {
                return Some(run);
            }
        }
        None
    }

    async fn take_next_queued(&self) -> Option<Arc<SandboxTaskHandle>> {
        let mut queued = self.queued.write().await;
        queued.take()
    }

    /// 查询当前正在运行的任务句柄。
    pub async fn find_current_task(&self, task_id: &str) -> Option<Arc<SandboxTaskHandle>> {
        let current = self.current.read().await;
        if let Some(task) = current.as_ref()
            && task.task_id() == task_id
        {
            return Some(task.clone());
        }
        None
    }
}

impl Default for SandboxState {
    fn default() -> Self {
        Self::new()
    }
}

/// 沙盒任务的运行句柄，用于在后台协程间共享状态。
#[derive(Clone)]
pub struct SandboxTaskHandle {
    run: Arc<RwLock<SandboxRun>>,
    cancel_token: CancellationToken,
    task_id: String,
    system: SystemKind,
}

impl SandboxTaskHandle {
    /// 根据初始运行记录创建句柄。
    pub fn new(run: SandboxRun, system: SystemKind) -> Self {
        let task_id = run.task_id.clone();
        SandboxTaskHandle {
            run: Arc::new(RwLock::new(run)),
            cancel_token: CancellationToken::new(),
            task_id,
            system,
        }
    }

    /// 返回任务 ID。
    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// 返回取消令牌，用于停止后台任务。
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// 返回当前任务所属系统。
    pub fn system(&self) -> SystemKind {
        self.system
    }

    /// 获取当前运行快照。
    pub async fn snapshot(&self) -> SandboxRun {
        self.run.read().await.clone()
    }

    pub(crate) async fn with_run_mut<F>(&self, mutator: F)
    where
        F: FnOnce(&mut SandboxRun),
    {
        let mut run = self.run.write().await;
        mutator(&mut run);
    }
}

/// 沙盒运行记录，包含阶段状态与摘要信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxRun {
    pub task_id: String,
    pub release_id: i32,
    pub status: TaskStatus,
    pub stages: Vec<StageResult>,
    pub overrides: Vec<FileOverride>,
    pub options: RunOptions,
    pub workspace_path: Option<String>,
    pub conclusion: Option<Conclusion>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl SandboxRun {
    /// 构造新的运行记录，并根据阶段顺序初始化等待状态。
    pub fn new(
        release_id: i32,
        system: SystemKind,
        overrides: Vec<FileOverride>,
        options: RunOptions,
    ) -> Self {
        let created_at = Utc::now();
        let random_suffix: String = (&mut rand::rng())
            .sample_iter(Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();
        let task_id = format!(
            "sandbox-{}-{}-{}",
            system.as_ref(),
            created_at.timestamp_millis(),
            random_suffix
        );
        let stages = SandboxStage::ordered()
            .iter()
            .copied()
            .map(StageResult::new)
            .collect();

        SandboxRun {
            task_id,
            release_id,
            status: TaskStatus::Queued,
            stages,
            overrides,
            options,
            workspace_path: None,
            conclusion: None,
            created_at,
            started_at: None,
            ended_at: None,
        }
    }

    /// 根据阶段枚举返回可变引用。
    pub fn stage_mut(&mut self, stage: SandboxStage) -> Option<&mut StageResult> {
        self.stages.iter_mut().find(|s| s.stage == stage)
    }
}

/// 单个阶段的执行状态与日志路径。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageResult {
    pub stage: SandboxStage,
    pub status: StageStatus,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub summary: Option<String>,
    pub error_code: Option<String>,
    pub log_path: Option<String>,
    #[serde(default)]
    pub diagnostics: Vec<DiagnosticHit>,
}

impl StageResult {
    /// 根据阶段初始化默认状态为 Pending。
    pub fn new(stage: SandboxStage) -> Self {
        StageResult {
            stage,
            status: StageStatus::Pending,
            started_at: None,
            ended_at: None,
            duration_ms: None,
            summary: None,
            error_code: None,
            log_path: None,
            diagnostics: Vec::new(),
        }
    }
}

/// 诊断命中信息，用于生成建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticHit {
    pub keyword: String,
    pub suggestion: String,
    pub priority: i32,
}

/// 汇总结果，用于判断沙盒是否通过。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Conclusion {
    pub passed: bool,
    pub failed_stage: Option<SandboxStage>,
    pub output_file_checks: Vec<OutputFileStatus>,
    pub input_count: usize,
    pub runtime_miss_count: usize,
    pub runtime_error_count: usize,
    #[serde(default)]
    pub runtime_output_count: usize,
    pub suspected_files: Vec<String>,
    pub top_suggestions: Vec<String>,
    pub daemon_ready: Option<bool>,
    pub wpgen_exit_code: Option<i32>,
    pub wpgen_generated_count: Option<usize>,
}

impl Conclusion {
    /// 构造通过的结论。
    pub fn passed() -> Self {
        Conclusion {
            passed: true,
            failed_stage: None,
            output_file_checks: Vec::new(),
            input_count: 0,
            runtime_miss_count: 0,
            runtime_error_count: 0,
            runtime_output_count: 0,
            suspected_files: Vec::new(),
            top_suggestions: Vec::new(),
            daemon_ready: Some(true),
            wpgen_exit_code: Some(0),
            wpgen_generated_count: None,
        }
    }

    /// 构造停止状态的结论。
    pub fn stopped() -> Self {
        Conclusion {
            passed: false,
            top_suggestions: vec!["任务已被停止".to_string()],
            ..Default::default()
        }
    }
}

/// 沙盒输出文件的快速检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFileStatus {
    pub relative_path: String,
    pub is_empty: bool,
    pub line_count: usize,
    pub meaning: String,
    /// 是否因文件非空而影响本次沙盒通过判定。
    #[serde(default = "default_output_check_affects_pass")]
    pub affects_pass: bool,
}

fn default_output_check_affects_pass() -> bool {
    true
}

/// 前端传入的临时文件覆盖内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileOverride {
    pub rule_type: String,
    pub file: String,
    pub content: String,
}

/// 运行时参数，控制采样数量与超时。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOptions {
    pub sample_count: u32,
    pub startup_timeout_ms: u64,
    pub wpgen_timeout_ms: u64,
    pub runtime_collect_ms: u64,
    pub keep_workspace: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        RunOptions {
            sample_count: 10,
            startup_timeout_ms: 30_000,
            wpgen_timeout_ms: 60_000,
            runtime_collect_ms: 5_000,
            keep_workspace: false,
        }
    }
}

impl RunOptions {
    /// 裁剪来自前端的参数，避免异常值。
    pub fn sanitized(mut self) -> Self {
        self.sample_count = self.sample_count.clamp(1, 10_000);
        self.startup_timeout_ms = self.startup_timeout_ms.clamp(5_000, 300_000);
        self.wpgen_timeout_ms = self.wpgen_timeout_ms.clamp(5_000, 600_000);
        self.runtime_collect_ms = self.runtime_collect_ms.clamp(1_000, 60_000);
        self
    }
}

/// 沙盒执行阶段，保持 UI 与后端一致。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxStage {
    PrepareWorkspace,
    PreflightCheck,
    StartDaemon,
    AnalyseStartupLogs,
    RunWpgen,
    AnalyseWpgenResult,
    AnalyseRuntimeOutput,
    FinalizeResult,
}

impl SandboxStage {
    /// 返回执行顺序，便于循环调度。
    pub const fn ordered() -> [SandboxStage; 5] {
        [
            SandboxStage::PrepareWorkspace,
            SandboxStage::PreflightCheck,
            SandboxStage::StartDaemon,
            SandboxStage::RunWpgen,
            SandboxStage::AnalyseRuntimeOutput,
        ]
    }

    /// 返回 snake_case 字符串，用于序列化或日志。
    pub fn as_str(&self) -> &'static str {
        match self {
            SandboxStage::PrepareWorkspace => "prepare_workspace",
            SandboxStage::PreflightCheck => "preflight_check",
            SandboxStage::StartDaemon => "start_daemon",
            SandboxStage::AnalyseStartupLogs => "analyse_startup_logs",
            SandboxStage::RunWpgen => "run_wpgen",
            SandboxStage::AnalyseWpgenResult => "analyse_wpgen_result",
            SandboxStage::AnalyseRuntimeOutput => "analyse_runtime_output",
            SandboxStage::FinalizeResult => "finalize_result",
        }
    }
}

impl std::fmt::Display for SandboxStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for SandboxStage {
    type Err = ();

    /// 将字符串解析为阶段枚举。
    fn from_str(stage: &str) -> Result<Self, Self::Err> {
        match stage {
            "prepare_workspace" => Ok(SandboxStage::PrepareWorkspace),
            "preflight_check" => Ok(SandboxStage::PreflightCheck),
            "start_daemon" => Ok(SandboxStage::StartDaemon),
            "analyse_startup_logs" => Ok(SandboxStage::AnalyseStartupLogs),
            "run_wpgen" => Ok(SandboxStage::RunWpgen),
            "analyse_wpgen_result" => Ok(SandboxStage::AnalyseWpgenResult),
            "analyse_runtime_output" => Ok(SandboxStage::AnalyseRuntimeOutput),
            "finalize_result" => Ok(SandboxStage::FinalizeResult),
            _ => Err(()),
        }
    }
}

/// 单个阶段的状态枚举。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
    Stopped,
}

/// 任务整体的状态枚举。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Success,
    Failed,
    Stopped,
}

impl TaskStatus {
    /// 转换为字符串，便于持久化。
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Queued => "queued",
            TaskStatus::Running => "running",
            TaskStatus::Success => "success",
            TaskStatus::Failed => "failed",
            TaskStatus::Stopped => "stopped",
        }
    }

    /// 从字符串解析状态。
    pub fn from_str_value(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(TaskStatus::Queued),
            "running" => Some(TaskStatus::Running),
            "success" => Some(TaskStatus::Success),
            "failed" => Some(TaskStatus::Failed),
            "stopped" => Some(TaskStatus::Stopped),
            _ => None,
        }
    }
}

/// 创建沙盒运行任务的请求体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSandboxRunRequest {
    pub release_id: i32,
    #[serde(default)]
    pub overrides: Vec<FileOverride>,
    #[serde(default)]
    pub options: RunOptions,
}

/// 创建沙盒运行任务后的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSandboxRunResponse {
    pub task_id: String,
    pub status: TaskStatus,
    pub queue_position: u8,
}

/// 指定阶段日志的响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStageLogResponse {
    pub task_id: String,
    pub stage: SandboxStage,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_path: Option<String>,
}

/// 沙盒历史单条记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxHistoryItem {
    pub task_id: String,
    pub status: TaskStatus,
    pub passed: bool,
    pub failed_stage: Option<SandboxStage>,
    pub sample_count: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
}

/// 沙盒历史列表响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxHistoryResponse {
    pub total: usize,
    pub items: Vec<SandboxHistoryItem>,
}

/// 最近一次沙盒运行的简单摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxLatestResponse {
    pub release_id: i32,
    pub task_id: String,
    pub status: TaskStatus,
    pub passed: bool,
    pub failed_stage: Option<SandboxStage>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// 入队位置，Immediate 表示无需排队。
#[derive(Debug, Clone, Copy)]
pub enum QueuePlacement {
    Immediate,
    Waiting,
}

impl QueuePlacement {
    /// 返回排队序号（0 表示立即执行）。
    pub fn position(&self) -> u8 {
        match self {
            QueuePlacement::Immediate => 0,
            QueuePlacement::Waiting => 1,
        }
    }
}
