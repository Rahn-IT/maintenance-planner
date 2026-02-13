use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{AppError, AppState, action_plan::ActionPlanItem};

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let executions = sqlx::query_as!(
        ActionPlanExecutionListItem,
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started,
            action_plan_executions.finished
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        ORDER BY action_plan_executions.started DESC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let template = state
        .jinja
        .get_template("action_plan_execution_list.html")
        .expect("template is loaded");
    let rendered = template.render(&ActionPlanExecutionList { executions })?;

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
        return Err(AppError::not_found(format!(
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
        "/action_plan_execution/{}",
        execution_id
    )))
}

pub async fn show(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let execution = sqlx::query!(
        r#"
        SELECT
            action_plan_executions.id as "id!: uuid::Uuid",
            action_plans.id as "action_plan_id!: uuid::Uuid",
            action_plans.name as "action_plan_name!",
            action_plan_executions.started
        FROM action_plan_executions
        INNER JOIN action_plans ON action_plans.id = action_plan_executions.action_plan
        WHERE action_plan_executions.id = $1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(execution) = execution else {
        return Err(AppError::not_found(format!(
            "No todo list exists for execution id: {}",
            id
        )));
    };

    let items = sqlx::query_as!(
        ActionPlanItem,
        r#"
        SELECT actions.name as "name!"
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

    let view = ActionPlanExecutionShow {
        id: execution.id,
        action_plan_id: execution.action_plan_id,
        action_plan_name: execution.action_plan_name,
        started: execution.started,
        items,
    };

    let template = state
        .jinja
        .get_template("action_plan_execution_show.html")
        .expect("template is loaded");
    let rendered = template.render(&view)?;

    Ok(Html(rendered))
}

#[derive(Serialize)]
struct ActionPlanExecutionShow {
    id: Uuid,
    action_plan_id: Uuid,
    action_plan_name: String,
    started: i64,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
struct ActionPlanExecutionList {
    executions: Vec<ActionPlanExecutionListItem>,
}

#[derive(FromRow, Serialize)]
struct ActionPlanExecutionListItem {
    id: Uuid,
    action_plan_name: String,
    started: i64,
    finished: i64,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
