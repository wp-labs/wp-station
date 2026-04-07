use crate::db::get_pool;
use crate::error::{DbError, DbResult};
use crate::server::sandbox::{RunOptions, SandboxRun, TaskStatus};
use crate::utils::constants::DEFAULT_HISTORY_LIMIT;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DbErr, EntityTrait, JsonValue, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde::Serialize;
use wp_station_migrations::entity::sandbox_run::{self, ActiveModel as SandboxRunActiveModel};

fn to_json_value<T: Serialize>(value: &T) -> Result<JsonValue, DbError> {
    serde_json::to_value(value).map_err(|err| DbError::Db(DbErr::Custom(err.to_string())))
}

fn from_json_value<T: serde::de::DeserializeOwned>(value: JsonValue) -> Result<T, DbError> {
    serde_json::from_value(value).map_err(|err| DbError::Db(DbErr::Custom(err.to_string())))
}

/// 新增一条沙盒运行记录。
pub async fn insert_sandbox_run_record(run: &SandboxRun) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let stages_json = to_json_value(&run.stages)?;
    let conclusion_json = match &run.conclusion {
        Some(c) => Some(to_json_value(c)?),
        None => None,
    };
    let options_json = to_json_value(&run.options)?;

    let active = SandboxRunActiveModel {
        task_id: Set(run.task_id.clone()),
        release_id: Set(run.release_id),
        status: Set(run.status.as_str().to_string()),
        stages_json: Set(stages_json),
        conclusion_json: Set(conclusion_json),
        options_json: Set(options_json),
        workspace_path: Set(run.workspace_path.clone()),
        daemon_ready: Set(None),
        wpgen_exit_code: Set(None),
        started_at: Set(run.started_at),
        ended_at: Set(run.ended_at),
        created_at: Set(run.created_at),
        ..Default::default()
    };

    active.insert(db).await?;
    Ok(())
}

/// 根据 task_id 更新沙盒运行记录。
pub async fn update_sandbox_run_record(run: &SandboxRun) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    let stages_json = to_json_value(&run.stages)?;
    let conclusion_json = match &run.conclusion {
        Some(c) => Some(to_json_value(c)?),
        None => None,
    };
    let options_json = to_json_value(&run.options)?;

    let model = sandbox_run::Entity::find()
        .filter(sandbox_run::Column::TaskId.eq(run.task_id.clone()))
        .one(db)
        .await?
        .ok_or(DbError::not_found("sandbox run"))?;

    let mut active: SandboxRunActiveModel = model.into();
    active.status = Set(run.status.as_str().to_string());
    active.stages_json = Set(stages_json);
    active.conclusion_json = Set(conclusion_json);
    active.options_json = Set(options_json);
    active.workspace_path = Set(run.workspace_path.clone());
    active.started_at = Set(run.started_at);
    active.ended_at = Set(run.ended_at);

    active.update(db).await?;
    Ok(())
}

/// 通过 task_id 查询沙盒运行记录。
pub async fn find_sandbox_run_by_task_id(task_id: &str) -> DbResult<Option<SandboxRun>> {
    let pool = get_pool();
    let db = pool.inner();

    let record = sandbox_run::Entity::find()
        .filter(sandbox_run::Column::TaskId.eq(task_id))
        .one(db)
        .await?;

    record.map(build_sandbox_run_from_model).transpose()
}

/// 查询指定 release 的历史记录，按创建时间倒序。
pub async fn list_sandbox_runs_by_release(
    release_id: i32,
    limit: Option<u64>,
) -> DbResult<Vec<SandboxRun>> {
    let pool = get_pool();
    let db = pool.inner();
    let real_limit = limit.unwrap_or(DEFAULT_HISTORY_LIMIT);

    let records = sandbox_run::Entity::find()
        .filter(sandbox_run::Column::ReleaseId.eq(release_id))
        .order_by_desc(sandbox_run::Column::CreatedAt)
        .limit(real_limit)
        .all(db)
        .await?;

    records
        .into_iter()
        .map(build_sandbox_run_from_model)
        .collect()
}

/// 统计指定 release 的沙盒运行次数。
pub async fn count_sandbox_runs_by_release(release_id: i32) -> DbResult<u64> {
    let pool = get_pool();
    let db = pool.inner();

    let count = sandbox_run::Entity::find()
        .filter(sandbox_run::Column::ReleaseId.eq(release_id))
        .count(db)
        .await?;
    Ok(count)
}

/// 查询 release 最新的一条沙盒记录。
pub async fn find_latest_sandbox_run(release_id: i32) -> DbResult<Option<SandboxRun>> {
    let pool = get_pool();
    let db = pool.inner();

    let record = sandbox_run::Entity::find()
        .filter(sandbox_run::Column::ReleaseId.eq(release_id))
        .order_by_desc(sandbox_run::Column::CreatedAt)
        .one(db)
        .await?;

    record.map(build_sandbox_run_from_model).transpose()
}

/// 删除指定 task_id 的沙盒记录（通常用于 enqueue 失败回滚）。
pub async fn delete_sandbox_run_record(task_id: &str) -> DbResult<()> {
    let pool = get_pool();
    let db = pool.inner();

    sandbox_run::Entity::delete_many()
        .filter(sandbox_run::Column::TaskId.eq(task_id))
        .exec(db)
        .await?;
    Ok(())
}

fn build_sandbox_run_from_model(model: sandbox_run::Model) -> DbResult<SandboxRun> {
    let stages = from_json_value(model.stages_json)?;
    let conclusion = match model.conclusion_json {
        Some(value) => Some(from_json_value(value)?),
        None => None,
    };
    let options: RunOptions = from_json_value(model.options_json)?;
    let status = TaskStatus::from_str_value(&model.status)
        .ok_or_else(|| DbError::Db(DbErr::Custom(format!("未知 TaskStatus: {}", model.status))))?;

    Ok(SandboxRun {
        task_id: model.task_id,
        release_id: model.release_id,
        status,
        stages,
        overrides: Vec::new(), // 历史记录无需回放覆盖内容
        options,
        workspace_path: model.workspace_path,
        conclusion,
        created_at: model.created_at,
        started_at: model.started_at,
        ended_at: model.ended_at,
    })
}
