// 调试功能业务逻辑层

use crate::db::{
    NewPerformanceTask, create_performance_task, find_performance_task_by_id,
    get_knowledge_config_status_list, get_performance_results,
};
use crate::error::AppError;
use crate::server::Setting;
use crate::utils::{load_knowledge, sql_query, warp_check_record};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use wp_data_fmt::{FormatType, Json, RecordFormatter};
use wp_model_core::model::DataRecord;

// SharedRecord 类型定义：用于在 API 之间共享解析结果
pub type SharedRecord = Arc<Mutex<Option<DataRecord>>>;

// ============ 请求参数结构体 ============

#[derive(Deserialize)]
pub struct DebugParseRequest {
    pub rules: String,
    pub logs: String,
}

#[derive(Deserialize)]
pub struct DebugTransformRequest {
    pub oml: String,
    #[serde(default)]
    pub parse_result: Option<serde_json::Value>,
}

#[derive(Deserialize)]
pub struct DebugKnowledgeStatusQuery {}

#[derive(Deserialize)]
pub struct DebugKnowledgeQueryRequest {
    pub table: String,
    pub sql: String,
}

#[derive(Deserialize, Serialize)]
pub struct DebugPerformanceRunRequest {
    pub sample: String,
    pub config: String,
}

#[derive(Deserialize)]
pub struct DebugPerformanceGetQuery {
    pub task_id: String,
}

// ============ 响应结构体 ============

#[derive(Serialize, Deserialize)]
pub struct RecordResponseRaw {
    pub fields: DataRecord,
    pub format_json: String,
}

#[derive(Serialize)]
pub struct DebugKnowledgeStatusItem {
    pub tag_name: String,
    pub is_active: bool,
}

#[derive(Serialize)]
pub struct DebugKnowledgeQueryResponse {
    pub success: bool,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total: usize,
}

#[derive(Deserialize, Serialize)]
pub struct DebugPerformanceRunResponse {
    pub task_id: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct DebugPerformanceSummary {
    pub total_lines: Option<i64>,
    pub duration: Option<i32>,
    pub avg_qps: Option<i32>,
}

#[derive(Serialize)]
pub struct DebugPerformanceSinkItem {
    pub name: String,
    pub lines: Option<i64>,
    pub qps: Option<i32>,
    pub status: Option<String>,
}

#[derive(Serialize)]
pub struct DebugPerformanceGetResponse {
    pub task_id: String,
    pub status: String,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub summary: DebugPerformanceSummary,
    pub sinks: Vec<DebugPerformanceSinkItem>,
}

// ============ 业务逻辑函数 ============

/// 解析日志并返回字段列表
pub async fn debug_parse_logic(
    shared_record: Arc<Mutex<Option<DataRecord>>>,
    rules: String,
    logs: String,
) -> Result<RecordResponseRaw, AppError> {
    // 调用 warp_check_record 获取 DataRecord
    let record = warp_check_record(&rules, &logs)?;

    // 存入 SharedRecord，供后续转换使用
    let mut record_guard = shared_record.lock().await;
    *record_guard = Some(record.clone());

    // 生成 format_json
    let formatter = FormatType::Json(Json);
    let json_string = formatter.fmt_record(&record);

    // 返回 RecordResponseRaw，包含完整的 DataRecord 和 format_json
    Ok(RecordResponseRaw {
        fields: record,
        format_json: json_string,
    })
}

/// 使用最近一次解析结果执行 OML 转换
pub async fn debug_transform_logic(
    shared_record: Arc<Mutex<Option<DataRecord>>>,
    oml: String,
) -> Result<RecordResponseRaw, AppError> {
    let record = {
        let record_guard = shared_record.lock().await;
        record_guard.clone()
    }
    .ok_or(AppError::NoParseResult)?;

    let transformed = crate::utils::oml::convert_record(&oml, record).await?;
    let formatter = FormatType::Json(Json);
    let json_string = formatter.fmt_record(&transformed);

    Ok(RecordResponseRaw {
        fields: transformed,
        format_json: json_string,
    })
}

/// 查询知识库配置状态列表
pub async fn debug_knowledge_status_logic() -> Result<Vec<DebugKnowledgeStatusItem>, AppError> {
    // 查询知识库配置状态列表
    let list = get_knowledge_config_status_list().await?;

    let items: Vec<DebugKnowledgeStatusItem> = list
        .into_iter()
        .map(|(file_name, is_active)| DebugKnowledgeStatusItem {
            tag_name: file_name.clone(),
            is_active,
        })
        .collect();

    Ok(items)
}

/// 执行知识库 SQL 查询（调试用）
pub async fn debug_knowledge_query_logic(
    _table: String,
    _sql: String,
) -> Result<DebugKnowledgeQueryResponse, AppError> {
    // 确保知识库已初始化，首次调用时加载配置
    let setting = Setting::load();
    if let Err(e) = load_knowledge(&setting.project_root) {
        warn!("加载知识库用于 SQL 调试失败: {}", e);
    }

    let fields = sql_query(&_sql)
        .await
        .map_err(|e| AppError::validation(format!("执行知识库 SQL 失败: {}", e)))?;

    let columns: Vec<String> = fields.iter().map(|f| f.get_name().to_string()).collect();
    let row: Vec<String> = fields.iter().map(|f| f.get_value().to_string()).collect();
    let rows = if columns.is_empty() {
        vec![]
    } else {
        vec![row]
    };
    let total = rows.len();

    Ok(DebugKnowledgeQueryResponse {
        success: true,
        columns,
        rows,
        total,
    })
}

/// 启动性能测试任务（调试用）
pub async fn debug_performance_run_logic(
    sample: String,
    config: String,
) -> Result<DebugPerformanceRunResponse, AppError> {
    // 创建性能测试任务
    let task_id = format!("perf-{}", chrono::Utc::now().timestamp_millis());
    let new_task = NewPerformanceTask {
        task_id: task_id.clone(),
        sample_data: Some(sample.clone()),
        config_content: Some(config.clone()),
        created_by: None,
    };

    create_performance_task(new_task).await?;

    // 这里可以异步启动真实性能测试逻辑，当前先省略
    Ok(DebugPerformanceRunResponse {
        task_id,
        status: "running".to_string(),
    })
}

/// 查询性能测试任务详情及结果
pub async fn debug_performance_get_logic(
    task_id: String,
) -> Result<DebugPerformanceGetResponse, AppError> {
    // 查询性能测试任务
    let task_res = find_performance_task_by_id(&task_id).await?;

    if let Some(task) = task_res {
        let results = get_performance_results(task.id).await?;

        let sinks: Vec<DebugPerformanceSinkItem> = results
            .into_iter()
            .map(|r| DebugPerformanceSinkItem {
                name: r.sink_name,
                lines: r.lines,
                qps: r.qps,
                status: r.status,
            })
            .collect();

        let resp = DebugPerformanceGetResponse {
            task_id: task.task_id,
            status: task.status,
            start_time: Some(task.start_time.to_rfc3339()),
            end_time: task.end_time.map(|t| t.to_rfc3339()),
            summary: DebugPerformanceSummary {
                total_lines: task.total_lines,
                duration: task.duration,
                avg_qps: task.avg_qps,
            },
            sinks,
        };

        Ok(resp)
    } else {
        Err(AppError::NotFound("性能测试任务不存在".to_string()))
    }
}

/// WPL 代码格式化
pub fn wpl_format_logic(code: String) -> Result<String, AppError> {
    use crate::utils::WplFormatter;

    let formatter = WplFormatter::new();
    formatter
        .format_with_error(&code)
        .map_err(|e| AppError::validation(format!("格式化 WPL 代码失败: {}", e)))
}

/// OML 代码格式化
pub fn oml_format_logic(code: String) -> Result<String, AppError> {
    use crate::utils::OmlFormatter;

    let formatter = OmlFormatter::new();
    formatter
        .format_with_error(&code)
        .map_err(|e| AppError::validation(format!("格式化 OML 代码失败: {}", e)))
}

/// 获取调试示例列表
pub fn debug_examples_logic() -> BTreeMap<String, serde_json::Value> {
    // wp-station 通过连接管理访问项目，示例应该从连接的项目中加载
    // 目前返回空列表，让前端使用默认示例
    BTreeMap::new()
}
