use axum::{
    Json,
    extract::{Path, State},
    response::{Html, Redirect},
};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{AppError, AppState, format_unix_timestamp};

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let unfinished_execution_rows = sqlx::query_as!(
        UnfinishedExecutionListItemRow,
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started as "started!"
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        WHERE action_plan_executions.finished IS NULL OR action_plan_executions.finished <= 0
        ORDER BY action_plan_executions.started DESC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let finished_execution_rows = sqlx::query_as!(
        FinishedExecutionListItemRow,
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started as "started!",
            action_plan_executions.finished as "finished!"
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        WHERE action_plan_executions.finished > 0
        ORDER BY action_plan_executions.finished DESC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let unfinished_executions = unfinished_execution_rows
        .into_iter()
        .map(|row| UnfinishedExecutionListItem {
            id: row.id,
            action_plan_name: row.action_plan_name,
            started_display: format_unix_timestamp(row.started),
        })
        .collect();

    let finished_executions = finished_execution_rows
        .into_iter()
        .map(|row| FinishedExecutionListItem {
            id: row.id,
            action_plan_name: row.action_plan_name,
            started_display: format_unix_timestamp(row.started),
            finished_display: format_unix_timestamp(row.finished),
        })
        .collect();

    let template = state
        .jinja
        .get_template("action_plan_execution_list.html")
        .expect("template is loaded");
    let rendered = template.render(&ActionPlanExecutionList {
        unfinished_executions,
        finished_executions,
    })?;

    Ok(Html(rendered))
}

pub async fn create_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let mut tx = state.db.begin().await?;

    let plan_exists = sqlx::query_scalar!(
        "SELECT id as \"id: uuid::Uuid\" FROM action_plans WHERE id = $1",
        id
    )
    .fetch_optional(&mut *tx)
    .await?;
    if plan_exists.is_none() {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No action plan exists for id: {}",
            id
        )));
    }

    let execution_id = Uuid::new_v4();
    let now = unix_now();

    sqlx::query!(
        "INSERT INTO action_plan_executions (id, action_plan, started, finished) VALUES ($1, $2, $3, NULL)",
        execution_id,
        id,
        now,
    )
    .execute(&mut *tx)
    .await?;

    let template_items = sqlx::query!(
        r#"
        SELECT id as "id: uuid::Uuid", order_index
        FROM action_items
        WHERE action_plan = $1
        ORDER BY order_index ASC
        "#,
        id
    )
    .fetch_all(&mut *tx)
    .await?;

    for item in template_items {
        let execution_item_id = Uuid::new_v4();
        sqlx::query!(
            r#"
            INSERT INTO action_item_executions (id, action_item, order_index, action_plan_execution, finished)
            VALUES ($1, $2, $3, $4, NULL)
            "#,
            execution_item_id,
            item.id,
            item.order_index,
            execution_id
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Redirect::to(&format!(
        "/executions/{}",
        execution_id
    )))
}

pub async fn show(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let execution = sqlx::query_as!(
        ActionPlanExecutionShowRow,
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.id as "action_plan_id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started as "started!",
            action_plan_executions.finished as "finished?"
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        WHERE action_plan_executions.id = $1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(execution) = execution else {
        return Err(AppError::not_found_for("Execution", format!(
            "No todo list exists for execution id: {}",
            id
        )));
    };

    let item_rows = sqlx::query_as!(
        ExecutionItemRow,
        r#"
        SELECT
            action_item_executions.id as "id!: uuid::Uuid",
            actions.name as "name!",
            action_item_executions.finished as "finished?",
            CASE
                WHEN action_item_executions.finished IS NULL OR action_item_executions.finished <= 0 THEN 0
                ELSE 1
            END as "is_finished!: i64"
        FROM action_item_executions
        INNER JOIN action_items ON action_items.id = action_item_executions.action_item
        INNER JOIN actions ON actions.id = action_items.action
        WHERE action_item_executions.action_plan_execution = $1
        ORDER BY action_item_executions.order_index ASC
        "#,
        id
    )
    .fetch_all(&state.db)
    .await?;
    let items: Vec<ExecutionItem> = item_rows
        .into_iter()
        .map(|row| ExecutionItem {
            id: row.id,
            name: row.name,
            is_finished: row.is_finished != 0,
            finished_display: row
                .finished
                .filter(|value| *value > 0)
                .map(format_unix_timestamp),
        })
        .collect();

    let view = ActionPlanExecutionShow {
        id: execution.id,
        action_plan_id: execution.action_plan_id,
        action_plan_name: execution.action_plan_name,
        started_display: format_unix_timestamp(execution.started),
        finished_display: execution
            .finished
            .filter(|value| *value > 0)
            .map(format_unix_timestamp),
        is_completed: execution.finished.map(|value| value > 0).unwrap_or(false),
        can_reopen: execution
            .finished
            .map(|value| value > 0 && unix_now().saturating_sub(value) <= 24 * 60 * 60)
            .unwrap_or(false),
        can_complete: !items.is_empty() && items.iter().all(|item| item.is_finished),
        items,
    };

    let template = state
        .jinja
        .get_template("action_plan_execution_show.html")
        .expect("template is loaded");
    let rendered = template.render(&view)?;

    Ok(Html(rendered))
}

pub async fn complete_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let execution_exists = sqlx::query_scalar!(
        r#"SELECT id as "id: uuid::Uuid" FROM action_plan_executions WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?;
    if execution_exists.is_none() {
        return Err(AppError::not_found_for("Execution", format!(
            "No todo list exists for execution id: {}",
            id
        )));
    }

    let incomplete_count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) as "count!: i64"
        FROM action_item_executions
        WHERE action_plan_execution = $1
            AND (finished IS NULL OR finished <= 0)
        "#,
        id
    )
    .fetch_one(&state.db)
    .await?;

    if incomplete_count > 0 {
        return Err(AppError::conflict(
            "All items must be checked before completing this execution.",
        ));
    }

    let finished_at = unix_now();
    sqlx::query!(
        r#"
        UPDATE action_plan_executions
        SET finished = $1
        WHERE id = $2
            AND (finished IS NULL OR finished <= 0)
        "#,
        finished_at,
        id
    )
    .execute(&state.db)
    .await?;

    Ok(Redirect::to(&format!("/executions/{}", id)))
}

pub async fn reopen_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let execution = sqlx::query!(
        r#"
        SELECT finished as "finished?"
        FROM action_plan_executions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    let Some(execution) = execution else {
        return Err(AppError::not_found_for("Execution", format!(
            "No todo list exists for execution id: {}",
            id
        )));
    };

    let Some(finished_at) = execution.finished else {
        return Err(AppError::conflict("Execution is already open."));
    };

    if finished_at <= 0 || unix_now().saturating_sub(finished_at) > 24 * 60 * 60 {
        return Err(AppError::conflict(
            "Execution can only be reopened within 24 hours of completion.",
        ));
    }

    sqlx::query!(
        r#"
        UPDATE action_plan_executions
        SET finished = NULL
        WHERE id = $1
        "#,
        id
    )
    .execute(&state.db)
    .await?;

    Ok(Redirect::to(&format!("/executions/{}", id)))
}

pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let mut tx = state.db.begin().await?;

    let execution = sqlx::query!(
        r#"
        SELECT finished as "finished?"
        FROM action_plan_executions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(execution) = execution else {
        return Err(AppError::not_found_for("Execution", format!(
            "No todo list exists for execution id: {}",
            id
        )));
    };

    if execution.finished.map(|value| value > 0).unwrap_or(false) {
        return Err(AppError::conflict(
            "Only open executions can be deleted.",
        ));
    }

    sqlx::query!(
        r#"
        DELETE FROM action_item_executions
        WHERE action_plan_execution = $1
        "#,
        id
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"
        DELETE FROM action_plan_executions
        WHERE id = $1
            AND (finished IS NULL OR finished <= 0)
        "#,
        id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(Redirect::to("/executions"))
}

pub async fn delete_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let execution = sqlx::query!(
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started as "started!",
            action_plan_executions.finished as "finished?"
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        WHERE action_plan_executions.id = $1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    let Some(execution) = execution else {
        return Err(AppError::not_found_for("Execution", format!(
            "No todo list exists for execution id: {}",
            id
        )));
    };

    if execution.finished.map(|value| value > 0).unwrap_or(false) {
        return Err(AppError::conflict(
            "Only open executions can be deleted.",
        ));
    }

    let view = DeleteExecutionConfirm {
        id: execution.id,
        action_plan_name: execution.action_plan_name,
        started_display: format_unix_timestamp(execution.started),
    };

    let template = state
        .jinja
        .get_template("execution_delete_confirm.html")
        .expect("template is loaded");
    let rendered = template.render(&view)?;

    Ok(Html(rendered))
}

pub async fn set_item_finished_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetItemFinishedRequest>,
) -> Result<Json<SetItemFinishedResponse>, AppError> {
    let finished = if body.finished { Some(unix_now()) } else { None };
    let result = sqlx::query!(
        "UPDATE action_item_executions SET finished = $1 WHERE id = $2",
        finished,
        id
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found_for("Execution", format!(
            "No execution item exists for id: {}",
            id
        )));
    }

    let finished_display = finished.map(format_unix_timestamp);

    Ok(Json(SetItemFinishedResponse { finished_display }))
}

#[derive(Serialize)]
struct ActionPlanExecutionShow {
    id: Uuid,
    action_plan_id: Uuid,
    action_plan_name: String,
    started_display: String,
    finished_display: Option<String>,
    is_completed: bool,
    can_reopen: bool,
    can_complete: bool,
    items: Vec<ExecutionItem>,
}

#[derive(Serialize)]
struct ExecutionItem {
    id: Uuid,
    name: String,
    is_finished: bool,
    finished_display: Option<String>,
}

#[derive(FromRow)]
struct ExecutionItemRow {
    id: Uuid,
    name: String,
    finished: Option<i64>,
    is_finished: i64,
}

#[derive(Deserialize)]
pub struct SetItemFinishedRequest {
    finished: bool,
}

#[derive(Serialize)]
pub struct SetItemFinishedResponse {
    finished_display: Option<String>,
}

#[derive(FromRow)]
struct ActionPlanExecutionShowRow {
    id: Uuid,
    action_plan_id: Uuid,
    action_plan_name: String,
    started: i64,
    finished: Option<i64>,
}

#[derive(Serialize)]
struct ActionPlanExecutionList {
    unfinished_executions: Vec<UnfinishedExecutionListItem>,
    finished_executions: Vec<FinishedExecutionListItem>,
}

#[derive(FromRow, Serialize)]
struct UnfinishedExecutionListItem {
    id: Uuid,
    action_plan_name: String,
    started_display: String,
}

#[derive(FromRow, Serialize)]
struct FinishedExecutionListItem {
    id: Uuid,
    action_plan_name: String,
    started_display: String,
    finished_display: String,
}

#[derive(FromRow)]
struct UnfinishedExecutionListItemRow {
    id: Uuid,
    action_plan_name: String,
    started: i64,
}

#[derive(FromRow)]
struct FinishedExecutionListItemRow {
    id: Uuid,
    action_plan_name: String,
    started: i64,
    finished: i64,
}

#[derive(Serialize)]
struct DeleteExecutionConfirm {
    id: Uuid,
    action_plan_name: String,
    started_display: String,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
