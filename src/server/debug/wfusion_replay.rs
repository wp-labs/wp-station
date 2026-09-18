//! WFusion 规则编辑器试跑逻辑。
//!
//! 从 `warp-fusion` 的 `wfl replay` 纯逻辑适配而来，只保留
//! Station 规则编辑器需要的 `WFS + WFL + NDJSON` 试跑能力，
//! 避免把 CLI 依赖链一并引入当前工程。

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::BufRead;
use std::path::Path;

use crate::error::AppError;
use chrono::DateTime;
use wf_engine::alert::OutputRecord;
use wf_engine::match_engine::{
    CepStateMachine, CloseOutput, CloseReason, EngineHashMap, Event, JoinRow, RuleExecutor,
    StepResult, Value, WindowLookup,
};
use wf_lang::WindowSchema;
use wf_lang::plan::RulePlan;

const PIPE_WINDOW_PREFIX: &str = "__wf_pipe_";
const PIPE_EVENT_TIME_FIELD: &str = "__wf_pipe_ts";

/// 试跑结果。
pub struct ReplayResult {
    pub alerts: Vec<OutputRecord>,
    pub event_count: u64,
    pub match_count: u64,
    pub error_count: u64,
}

/// 纯逻辑试跑：解析 WFL、编译执行计划并回放 NDJSON 事件。
pub fn replay_events<R: BufRead>(
    wfl_source: &str,
    schemas: &[WindowSchema],
    reader: R,
    color: bool,
) -> Result<ReplayResult, AppError> {
    let wfl_path = Path::new("rules/editor.wfl");
    let wfl_file = wf_lang::parse_wfl_with_diagnostics(wfl_source, wfl_path).map_err(|err| {
        AppError::validation(err.detail().clone().unwrap_or_else(|| err.to_string()))
    })?;
    let plans = wf_lang::compile_wfl_with_diagnostics(&wfl_file, schemas, wfl_source, wfl_path)
        .map_err(|err| {
            AppError::validation(err.detail().clone().unwrap_or_else(|| err.to_string()))
        })?;

    if plans.is_empty() {
        return Ok(ReplayResult {
            alerts: vec![],
            event_count: 0,
            match_count: 0,
            error_count: 0,
        });
    }

    replay_with_plans(
        &plans,
        schemas,
        reader,
        color,
        ReplayExecOptions {
            scan_expired_each_event: false,
            eof_action: ReplayEofAction::CloseAllEos,
        },
    )
}

/// Replay 模式下没有真实 window store，join 与 snapshot 查询统一返回空。
struct NullWindowLookup;

impl WindowLookup for NullWindowLookup {
    fn snapshot_field_values(
        &self,
        _window: &str,
        _field: &str,
    ) -> Option<std::collections::HashSet<String>> {
        None
    }

    fn snapshot(&self, _window: &str) -> Option<Vec<JoinRow>> {
        None
    }
}

struct ReplayEngine {
    machine: CepStateMachine,
    executor: RuleExecutor,
    conv_plan: Option<wf_lang::plan::ConvPlan>,
}

#[derive(Clone, Copy)]
enum ReplayEofAction {
    CloseAllEos,
}

#[derive(Clone, Copy)]
struct ReplayExecOptions {
    scan_expired_each_event: bool,
    eof_action: ReplayEofAction,
}

#[derive(Clone)]
struct ConsumerRoute {
    engine_idx: usize,
    bind_alias: String,
}

fn replay_with_plans<R: BufRead>(
    plans: &[RulePlan],
    schemas: &[WindowSchema],
    reader: R,
    color: bool,
    options: ReplayExecOptions,
) -> Result<ReplayResult, AppError> {
    let stream_to_windows = build_stream_to_windows_map(schemas);
    let window_to_binds = build_window_to_binds_map(plans);

    let mut engines: Vec<ReplayEngine> = plans
        .iter()
        .map(|plan| {
            let time_field = resolve_replay_time_field_auto(plan, schemas);
            let limits = plan.limits_plan.clone();
            let machine = CepStateMachine::with_limits(
                plan.name.clone(),
                plan.match_plan.clone(),
                time_field,
                limits,
            );
            let executor = RuleExecutor::new(plan.clone());
            ReplayEngine {
                machine,
                executor,
                conv_plan: plan.conv_plan.clone(),
            }
        })
        .collect();

    let all_routes = build_all_routes(plans);
    let lookup = NullWindowLookup;
    let mut alerts = Vec::new();
    let mut event_count = 0u64;
    let mut match_count = 0u64;
    let mut error_count = 0u64;
    let known_time_fields = collect_known_time_fields(schemas);
    let mut has_watermark = false;
    let mut last_watermark_nanos = 0i64;

    for line_result in reader.lines() {
        let line = line_result.map_err(AppError::internal)?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_error) => {
                error_count += 1;
                continue;
            }
        };

        if let Some(watermark_nanos) = infer_event_watermark_nanos(&json, &known_time_fields) {
            if !has_watermark || watermark_nanos > last_watermark_nanos {
                has_watermark = true;
                last_watermark_nanos = watermark_nanos;
            }

            if options.scan_expired_each_event {
                for i in 0..engines.len() {
                    run_timeout_scan_for_engine(
                        i,
                        watermark_nanos,
                        &all_routes,
                        &mut engines,
                        &lookup,
                        &mut alerts,
                        &mut match_count,
                        &mut error_count,
                        color,
                    );
                }
            }
        }

        let event = json_to_event_with_time_fields(&json, &known_time_fields);
        event_count += 1;

        let route_keys = resolve_event_routes(&json, &stream_to_windows, &window_to_binds);
        let mut queue = VecDeque::new();
        for route_key in route_keys {
            queue.push_back((route_key, event.clone()));
        }

        while let Some((route_key, route_event)) = queue.pop_front() {
            route_event_once(
                &all_routes,
                &mut engines,
                &lookup,
                &route_key,
                &route_event,
                &mut queue,
                &mut alerts,
                &mut match_count,
                &mut error_count,
                color,
            );
        }
    }

    match options.eof_action {
        ReplayEofAction::CloseAllEos => {
            if has_watermark {
                let _ = last_watermark_nanos;
            }
            for i in 0..engines.len() {
                let close_outputs = {
                    let engine = &mut engines[i];
                    engine
                        .machine
                        .close_all_with_conv(CloseReason::Eos, engine.conv_plan.as_ref())
                };
                handle_close_outputs_for_engine(
                    i,
                    &close_outputs,
                    &all_routes,
                    &mut engines,
                    &lookup,
                    &mut alerts,
                    &mut match_count,
                    &mut error_count,
                    color,
                );
            }
        }
    }

    Ok(ReplayResult {
        alerts,
        event_count,
        match_count,
        error_count,
    })
}

fn build_stream_to_windows_map(schemas: &[WindowSchema]) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for schema in schemas {
        for stream in &schema.streams {
            map.entry(stream.clone())
                .or_insert_with(Vec::new)
                .push(schema.name.clone());
        }
    }
    map
}

fn build_window_to_binds_map(plans: &[RulePlan]) -> HashMap<String, Vec<(usize, String)>> {
    let mut map = HashMap::new();
    for (engine_idx, plan) in plans.iter().enumerate() {
        for bind in &plan.binds {
            if !is_internal_window_name(&bind.window) {
                map.entry(bind.window.clone())
                    .or_insert_with(Vec::new)
                    .push((engine_idx, bind.alias.clone()));
            }
        }
    }
    map
}

fn resolve_event_routes(
    json: &serde_json::Value,
    stream_to_windows: &HashMap<String, Vec<String>>,
    window_to_binds: &HashMap<String, Vec<(usize, String)>>,
) -> Vec<String> {
    let mut routes = Vec::new();
    let stream_name = json
        .get("_stream")
        .and_then(|value| value.as_str())
        .unwrap_or("");
    if stream_name.is_empty() {
        return routes;
    }

    let windows = stream_to_windows
        .get(stream_name)
        .cloned()
        .unwrap_or_default();
    for window in windows {
        let binds = window_to_binds.get(&window).cloned().unwrap_or_default();
        for (_engine_idx, bind_alias) in binds {
            routes.push(external_route_key(&bind_alias));
        }
    }
    routes
}

fn resolve_replay_time_field_auto(plan: &RulePlan, schemas: &[WindowSchema]) -> Option<String> {
    if let Some(first_step) = plan.match_plan.event_steps.first()
        && let Some(first_branch) = first_step.branches.first()
    {
        let source_alias = &first_branch.source;
        if let Some(bind) = plan.binds.iter().find(|bind| bind.alias == *source_alias) {
            if is_internal_window_name(&bind.window) {
                return Some(PIPE_EVENT_TIME_FIELD.to_string());
            }
            if let Some(time_field) = schemas
                .iter()
                .find(|schema| schema.name == bind.window)
                .and_then(|schema| schema.time_field.clone())
            {
                return Some(time_field);
            }
        }
    }

    for bind in &plan.binds {
        if is_internal_window_name(&bind.window) {
            return Some(PIPE_EVENT_TIME_FIELD.to_string());
        }
        if let Some(time_field) = schemas
            .iter()
            .find(|schema| schema.name == bind.window)
            .and_then(|schema| schema.time_field.clone())
        {
            return Some(time_field);
        }
    }

    plan.binds
        .iter()
        .find(|bind| is_internal_window_name(&bind.window))
        .map(|_| PIPE_EVENT_TIME_FIELD.to_string())
}

fn build_all_routes(plans: &[RulePlan]) -> HashMap<String, Vec<ConsumerRoute>> {
    let mut routes = HashMap::new();
    for (engine_idx, plan) in plans.iter().enumerate() {
        for bind in &plan.binds {
            let route_key = if is_internal_window_name(&bind.window) {
                bind.window.clone()
            } else {
                external_route_key(&bind.alias)
            };
            routes
                .entry(route_key)
                .or_insert_with(Vec::new)
                .push(ConsumerRoute {
                    engine_idx,
                    bind_alias: bind.alias.clone(),
                });
        }
    }
    routes
}

fn external_route_key(alias: &str) -> String {
    format!("__ext__{alias}")
}

fn is_internal_window_name(name: &str) -> bool {
    name.starts_with(PIPE_WINDOW_PREFIX)
}

#[allow(clippy::too_many_arguments)]
fn route_event_once(
    routes: &HashMap<String, Vec<ConsumerRoute>>,
    engines: &mut [ReplayEngine],
    lookup: &NullWindowLookup,
    route_key: &str,
    event: &Event,
    queue: &mut VecDeque<(String, Event)>,
    alerts: &mut Vec<OutputRecord>,
    match_count: &mut u64,
    error_count: &mut u64,
    color: bool,
) {
    let consumers = routes.get(route_key).cloned().unwrap_or_default();
    for consumer in consumers {
        let step = engines[consumer.engine_idx].machine.advance_with(
            &consumer.bind_alias,
            event,
            Some(lookup),
        );
        if let StepResult::Matched(ctx) = step {
            match engines[consumer.engine_idx]
                .executor
                .execute_match_with_joins(&ctx, lookup)
            {
                Ok(Some(record)) => handle_output_record(record, queue, alerts, match_count),
                Ok(None) => {}
                Err(_error) => {
                    let _ = color;
                    *error_count += 1;
                }
            }
        }
    }
}

fn handle_output_record(
    record: OutputRecord,
    queue: &mut VecDeque<(String, Event)>,
    alerts: &mut Vec<OutputRecord>,
    match_count: &mut u64,
) {
    if is_internal_window_name(&record.yield_target) {
        queue.push_back((
            record.yield_target.to_string(),
            output_record_to_event(&record),
        ));
    } else {
        alerts.push(record);
        *match_count += 1;
    }
}

fn output_record_to_event(record: &OutputRecord) -> Event {
    let mut fields: EngineHashMap<_, _> = EngineHashMap::default();
    fields.insert(
        PIPE_EVENT_TIME_FIELD.to_string().into(),
        Value::Number(record.event_time_nanos as f64),
    );
    for (name, value) in &record.yield_fields {
        fields.insert(name.to_string().into(), value.clone());
    }
    Event { fields }
}

fn json_to_event_with_time_fields(
    json: &serde_json::Value,
    time_fields: &HashSet<String>,
) -> Event {
    let mut fields: EngineHashMap<_, _> = EngineHashMap::default();
    if let serde_json::Value::Object(map) = json {
        for (key, value) in map {
            if time_fields.contains(key)
                && let Some(nanos) = parse_json_timestamp_nanos(value)
            {
                fields.insert(key.clone().into(), Value::Number(nanos as f64));
                continue;
            }

            let value = match value {
                serde_json::Value::Number(number) => {
                    if let Some(float) = number.as_f64() {
                        Value::Number(float)
                    } else {
                        continue;
                    }
                }
                serde_json::Value::String(string) => Value::Str(string.clone().into()),
                serde_json::Value::Bool(boolean) => Value::Bool(*boolean),
                _ => continue,
            };
            fields.insert(key.clone().into(), value);
        }
    }
    Event { fields }
}

fn collect_known_time_fields(schemas: &[WindowSchema]) -> HashSet<String> {
    schemas
        .iter()
        .filter_map(|schema| schema.time_field.clone())
        .collect()
}

fn parse_json_timestamp_nanos(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64().map(|float| float as i64),
        serde_json::Value::String(string) => {
            if let Ok(datetime) = DateTime::parse_from_rfc3339(string) {
                return datetime.timestamp_nanos_opt();
            }
            string.parse::<i64>().ok()
        }
        _ => None,
    }
}

fn infer_event_watermark_nanos(
    json: &serde_json::Value,
    known_time_fields: &HashSet<String>,
) -> Option<i64> {
    if let Some(timestamp) = json.get("_timestamp").and_then(parse_json_timestamp_nanos) {
        return Some(timestamp);
    }

    for field in known_time_fields {
        if let Some(value) = json.get(field)
            && let Some(timestamp) = parse_json_timestamp_nanos(value)
        {
            return Some(timestamp);
        }
    }

    None
}

#[allow(clippy::too_many_arguments)]
fn handle_close_outputs_for_engine(
    engine_idx: usize,
    close_outputs: &[CloseOutput],
    routes: &HashMap<String, Vec<ConsumerRoute>>,
    engines: &mut [ReplayEngine],
    lookup: &NullWindowLookup,
    alerts: &mut Vec<OutputRecord>,
    match_count: &mut u64,
    error_count: &mut u64,
    color: bool,
) {
    let mut queue = VecDeque::new();
    for close in close_outputs {
        let result = {
            let engine = &mut engines[engine_idx];
            engine.executor.execute_close_with_joins(close, lookup)
        };
        match result {
            Ok(Some(record)) => handle_output_record(record, &mut queue, alerts, match_count),
            Ok(None) => {}
            Err(_error) => {
                let _ = color;
                *error_count += 1;
            }
        }
    }

    while let Some((route_key, route_event)) = queue.pop_front() {
        route_event_once(
            routes,
            engines,
            lookup,
            &route_key,
            &route_event,
            &mut queue,
            alerts,
            match_count,
            error_count,
            color,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn run_timeout_scan_for_engine(
    engine_idx: usize,
    watermark_nanos: i64,
    routes: &HashMap<String, Vec<ConsumerRoute>>,
    engines: &mut [ReplayEngine],
    lookup: &NullWindowLookup,
    alerts: &mut Vec<OutputRecord>,
    match_count: &mut u64,
    error_count: &mut u64,
    color: bool,
) {
    let close_outputs = {
        let engine = &mut engines[engine_idx];
        engine
            .machine
            .scan_expired_at_with_conv(watermark_nanos, engine.conv_plan.as_ref())
    };
    handle_close_outputs_for_engine(
        engine_idx,
        &close_outputs,
        routes,
        engines,
        lookup,
        alerts,
        match_count,
        error_count,
        color,
    );
}
