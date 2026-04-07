use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::{Rng, distributions::Alphanumeric};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::db::{
    count_sandbox_runs_by_release, delete_sandbox_run_record, find_release_by_id,
    find_sandbox_run_by_task_id, insert_sandbox_run_record, list_sandbox_runs_by_release,
    update_sandbox_run_record,
};
use crate::error::AppError;
use crate::server::{
    OperationLogAction, OperationLogBiz, OperationLogParams, OperationLogStatus, Setting,
    write_operation_log, write_operation_log_for_result,
};
use crate::utils::constants::MAX_LINES;

use super::sandbox_runner;

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
        log_sandbox_execution_result(&snapshot).await;
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
}

impl SandboxTaskHandle {
    /// 根据初始运行记录创建句柄。
    pub fn new(run: SandboxRun) -> Self {
        let task_id = run.task_id.clone();
        SandboxTaskHandle {
            run: Arc::new(RwLock::new(run)),
            cancel_token: CancellationToken::new(),
            task_id,
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
    pub fn new(release_id: i32, overrides: Vec<FileOverride>, options: RunOptions) -> Self {
        let created_at = Utc::now();
        let random_suffix: String = rand::thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect();
        let task_id = format!(
            "sandbox-{}-{}",
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

/// 创建沙盒运行任务并将其排入队列。
pub async fn create_sandbox_run_logic(
    state: SandboxState,
    request: CreateSandboxRunRequest,
) -> Result<CreateSandboxRunResponse, AppError> {
    // 确保 release 存在
    let release = find_release_by_id(request.release_id)
        .await?
        .ok_or_else(|| AppError::not_found("发布记录不存在"))?;
    let release_version = release.version.clone();
    let release_id = release.id;

    let sanitized_options = request.options.clone().sanitized();
    let options_for_log = sanitized_options.clone();
    let overrides = request.overrides.clone();
    let overrides_len = overrides.len();

    let result = async {
        let run = SandboxRun::new(release_id, overrides, sanitized_options);
        insert_sandbox_run_record(&run)
            .await
            .map_err(AppError::from)?;
        let task_id = run.task_id.clone();
        let task_handle = Arc::new(SandboxTaskHandle::new(run));
        let placement = match state.enqueue_task(task_handle.clone()).await {
            Ok(p) => p,
            Err(err) => {
                let _ = delete_sandbox_run_record(&task_id).await;
                return Err(err);
            }
        };

        if matches!(placement, QueuePlacement::Immediate) {
            sandbox_runner::spawn_sandbox_execution(state.clone(), task_handle.clone());
        }

        let snapshot = task_handle.snapshot().await;
        info!(
            "创建沙盒任务成功: task_id={}, release_id={}, placement={:?}",
            snapshot.task_id, snapshot.release_id, placement
        );

        Ok(CreateSandboxRunResponse {
            task_id: snapshot.task_id,
            status: snapshot.status,
            queue_position: placement.position(),
        })
    }
    .await;

    let mut log_params = OperationLogParams::new()
        .with_target_id(release_id.to_string())
        .with_target_name(release_version)
        .with_field("sample_count", options_for_log.sample_count.to_string())
        .with_field("overrides", overrides_len.to_string())
        .with_field("keep_workspace", options_for_log.keep_workspace.to_string());

    if let Ok(resp) = &result {
        log_params = log_params
            .with_field("task_id", resp.task_id.clone())
            .with_field("queue_position", resp.queue_position.to_string());
    }

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Validate,
        log_params,
        &result,
    )
    .await;

    result
}

/// 查询指定任务 ID 的沙盒运行记录。
pub async fn get_sandbox_run_logic(
    state: SandboxState,
    task_id: &str,
) -> Result<SandboxRun, AppError> {
    if let Some(run) = state.snapshot_by_id(task_id).await {
        return Ok(run);
    }
    if let Some(run) = find_sandbox_run_by_task_id(task_id)
        .await
        .map_err(AppError::from)?
    {
        return Ok(run);
    }
    Err(AppError::not_found("沙盒任务不存在"))
}

/// 查询指定发布的沙盒历史记录，包含正在执行的任务。
pub async fn list_sandbox_history_logic(
    state: SandboxState,
    release_id: i32,
    limit: u64,
) -> Result<SandboxHistoryResponse, AppError> {
    find_release_by_id(release_id)
        .await?
        .ok_or_else(|| AppError::not_found("发布记录不存在"))?;

    let capped = limit.max(1);
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    let active_runs = state.active_runs_for_release(release_id).await;
    for run in active_runs {
        seen.insert(run.task_id.clone());
        items.push(history_item_from_run(&run));
    }

    let remaining = capped.saturating_sub(items.len() as u64);
    if remaining == 0 {
        items.truncate(capped as usize);
        return Ok(SandboxHistoryResponse {
            total: items.len(),
            items,
        });
    }

    let db_limit = remaining.saturating_add(seen.len() as u64);
    let db_runs = list_sandbox_runs_by_release(release_id, Some(db_limit))
        .await
        .map_err(AppError::from)?;

    for run in db_runs {
        if seen.contains(&run.task_id) {
            continue;
        }
        items.push(history_item_from_run(&run));
        if items.len() as u64 >= capped {
            break;
        }
    }

    let total_count = count_sandbox_runs_by_release(release_id)
        .await
        .map_err(AppError::from)?
        .max(items.len() as u64) as usize;

    Ok(SandboxHistoryResponse {
        total: total_count,
        items,
    })
}

/// 停止正在执行或等待中的沙盒任务。
pub async fn stop_sandbox_run_logic(
    state: SandboxState,
    task_id: &str,
) -> Result<SandboxRun, AppError> {
    let result = if let Some(task) = state.find_current_task(task_id).await {
        task.cancel_token().cancel();
        task.with_run_mut(|run| {
            run.status = TaskStatus::Stopped;
            let now = Utc::now();
            run.ended_at.get_or_insert(now);
            for stage in &mut run.stages {
                if matches!(stage.status, StageStatus::Pending | StageStatus::Running) {
                    stage.status = StageStatus::Stopped;
                    stage
                        .summary
                        .get_or_insert_with(|| "任务已被用户终止".to_string());
                    stage.ended_at.get_or_insert(now);
                }
            }
            if run.conclusion.is_none() {
                run.conclusion = Some(Conclusion::stopped());
            }
        })
        .await;
        let snapshot = task.snapshot().await;
        if let Err(err) = update_sandbox_run_record(&snapshot).await {
            warn!("更新沙盒记录失败: {}", err);
        } else {
            info!(
                "已停止运行中的沙盒任务: task_id={}, release_id={}",
                snapshot.task_id, snapshot.release_id
            );
        }
        Ok(snapshot)
    } else if let Some(task) = state.stop_queued_task(task_id).await {
        task.with_run_mut(|run| {
            run.status = TaskStatus::Stopped;
            let now = Utc::now();
            run.ended_at = Some(now);
            for stage in &mut run.stages {
                if matches!(stage.status, StageStatus::Pending | StageStatus::Running) {
                    stage.status = StageStatus::Stopped;
                    stage.summary = Some("任务在排队阶段被终止".to_string());
                    let created_at = run.created_at;
                    stage.started_at.get_or_insert(created_at);
                    stage.ended_at = Some(now);
                    stage.duration_ms = Some(0);
                }
            }
            run.conclusion = Some(Conclusion::stopped());
        })
        .await;

        let snapshot = task.snapshot().await;
        if let Err(err) = update_sandbox_run_record(&snapshot).await {
            warn!("更新沙盒记录失败: {}", err);
        } else {
            info!(
                "已停止排队中的沙盒任务: task_id={}, release_id={}",
                snapshot.task_id, snapshot.release_id
            );
        }
        Ok(snapshot)
    } else if find_sandbox_run_by_task_id(task_id)
        .await
        .map_err(AppError::from)?
        .is_some()
    {
        Err(AppError::conflict_with_code(
            "TASK_NOT_RUNNING",
            "当前没有正在执行或等待的沙盒任务",
        ))
    } else {
        Err(AppError::not_found("沙盒任务不存在或已完成"))
    };

    let mut log_params = OperationLogParams::new().with_field("task_id", task_id.to_string());
    if let Ok(run) = &result {
        log_params = log_params
            .with_target_id(run.release_id.to_string())
            .with_field("final_status", run.status.as_str().to_string());
        if let Some(version) = fetch_release_version(run.release_id).await {
            log_params = log_params.with_target_name(version);
        }
    }

    write_operation_log_for_result(
        OperationLogBiz::Release,
        OperationLogAction::Cancel,
        log_params,
        &result,
    )
    .await;

    result
}

/// 获取指定阶段的日志内容。
pub async fn get_stage_logs_logic(
    state: SandboxState,
    task_id: &str,
    stage: SandboxStage,
) -> Result<SandboxStageLogResponse, AppError> {
    let run = if let Some(current) = state.snapshot_by_id(task_id).await {
        current
    } else {
        find_sandbox_run_by_task_id(task_id)
            .await
            .map_err(AppError::from)?
            .ok_or_else(|| AppError::not_found("沙盒任务不存在"))?
    };

    let stage_info = run
        .stages
        .iter()
        .find(|item| item.stage == stage)
        .ok_or_else(|| AppError::not_found("阶段信息不存在"))?;

    let content = if let Some(path) = &stage_info.log_path {
        read_log_content(path)
    } else {
        Ok(String::new())
    }?;

    Ok(SandboxStageLogResponse {
        task_id: run.task_id,
        stage,
        content,
        log_path: stage_info.log_path.clone(),
    })
}

/// 查询指定发布最近一次沙盒结果。
pub async fn get_latest_sandbox_run_logic(
    state: SandboxState,
    release_id: i32,
) -> Result<SandboxLatestResponse, AppError> {
    if let Some(run) = state.latest_for_release(release_id).await {
        return Ok(SandboxLatestResponse {
            release_id,
            task_id: run.task_id,
            status: run.status,
            passed: run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false),
            failed_stage: run.conclusion.as_ref().and_then(|c| c.failed_stage),
            ended_at: run.ended_at,
        });
    }

    let mut records = list_sandbox_runs_by_release(release_id, Some(1))
        .await
        .map_err(AppError::from)?;
    if let Some(run) = records.pop() {
        return Ok(SandboxLatestResponse {
            release_id,
            task_id: run.task_id,
            status: run.status,
            passed: run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false),
            failed_stage: run.conclusion.as_ref().and_then(|c| c.failed_stage),
            ended_at: run.ended_at,
        });
    }

    Err(AppError::not_found("该发布暂无沙盒记录"))
}

async fn fetch_release_version(release_id: i32) -> Option<String> {
    match find_release_by_id(release_id).await {
        Ok(Some(release)) => Some(release.version),
        Ok(None) => {
            warn!(
                "未找到 release 记录，无法补全沙盒日志: release_id={}",
                release_id
            );
            None
        }
        Err(err) => {
            warn!(
                "查询 release 失败，无法补全沙盒日志: release_id={}, error={}",
                release_id, err
            );
            None
        }
    }
}

async fn log_sandbox_execution_result(run: &SandboxRun) {
    let mut params = OperationLogParams::new()
        .with_target_id(run.release_id.to_string())
        .with_field("task_id", run.task_id.clone())
        .with_field("final_status", run.status.as_str().to_string());

    if let Some(version) = fetch_release_version(run.release_id).await {
        params = params.with_target_name(version);
    }

    if let Some(conclusion) = run.conclusion.as_ref() {
        params = params
            .with_field("passed", conclusion.passed.to_string())
            .with_field(
                "runtime_error_count",
                conclusion.runtime_error_count.to_string(),
            )
            .with_field(
                "runtime_miss_count",
                conclusion.runtime_miss_count.to_string(),
            )
            .with_field("input_count", conclusion.input_count.to_string());

        if let Some(stage) = conclusion.failed_stage {
            params = params.with_field("failed_stage", stage.as_str().to_string());
        }
    }

    let log_status = if run
        .conclusion
        .as_ref()
        .map(|c| c.passed)
        .unwrap_or(run.status == TaskStatus::Success)
    {
        OperationLogStatus::Success
    } else {
        OperationLogStatus::Error
    };

    write_operation_log(
        OperationLogBiz::Release,
        OperationLogAction::Validate,
        params,
        log_status,
    )
    .await;
}

fn read_log_content(path: &str) -> Result<String, AppError> {
    if path.is_empty() {
        return Ok("日志文件路径为空".to_string());
    }
    let resolved = resolve_log_path(path);
    let path_ref = resolved.as_path();
    if !path_ref.exists() {
        return Ok("日志文件不存在，可能沙盒目录已清理或尚未生成。".to_string());
    }
    let content = std::fs::read_to_string(path_ref).map_err(AppError::internal)?;
    if content.trim().is_empty() {
        return Ok("日志为空（命令未产生任何输出）".to_string());
    }
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Ok("日志为空（命令未产生任何输出）".to_string());
    }
    if lines.len() > MAX_LINES {
        let start = lines.len() - MAX_LINES;
        let mut truncated = lines[start..].join("\n");
        truncated.push_str(&format!(
            "\n...（日志超出 {} 行，已截断，仅展示最新内容）",
            MAX_LINES
        ));
        Ok(truncated)
    } else {
        Ok(content)
    }
}

fn resolve_log_path(path: &str) -> PathBuf {
    let path_ref = Path::new(path);
    if path_ref.is_absolute() {
        path_ref.to_path_buf()
    } else {
        Setting::workspace_root().join(path_ref)
    }
}

fn history_item_from_run(run: &SandboxRun) -> SandboxHistoryItem {
    let passed = run.conclusion.as_ref().map(|c| c.passed).unwrap_or(false);
    let failed_stage = run.conclusion.as_ref().and_then(|c| c.failed_stage);
    SandboxHistoryItem {
        task_id: run.task_id.clone(),
        status: run.status,
        passed,
        failed_stage,
        sample_count: run.options.sample_count,
        started_at: run.started_at,
        ended_at: run.ended_at,
        duration_ms: compute_duration_ms(run.started_at, run.ended_at),
    }
}

fn compute_duration_ms(
    started_at: Option<DateTime<Utc>>,
    ended_at: Option<DateTime<Utc>>,
) -> Option<u64> {
    match (started_at, ended_at) {
        (Some(start), Some(end)) => {
            let duration = end.signed_duration_since(start);
            if duration.num_milliseconds() < 0 {
                Some(0)
            } else {
                Some(duration.num_milliseconds() as u64)
            }
        }
        _ => None,
    }
}
