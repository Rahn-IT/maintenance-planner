use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::{AppError, AppState, format_unix_timestamp};

#[derive(FromRow, Debug, Serialize)]
pub struct ActionPlan {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct ActionPlanList {
    action_plans: Vec<ActionPlanListItem>,
}

#[derive(Serialize)]
pub struct ActionPlanListItem {
    id: Uuid,
    name: String,
    active_execution_id: Option<Uuid>,
    last_finished_display: Option<String>,
}

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let action_plans = sqlx::query_as!(
        ActionPlan,
        r#"SELECT id as "id: uuid::Uuid", name FROM action_plans"#
    )
    .fetch_all(&state.db)
    .await?;

    let mut action_plan_list = Vec::with_capacity(action_plans.len());
    for action_plan in action_plans {
        let active_execution_id = sqlx::query_scalar!(
            r#"
            SELECT id as "id: uuid::Uuid"
            FROM action_plan_executions
            WHERE action_plan = $1
                AND (finished IS NULL OR finished <= 0)
            ORDER BY started DESC
            LIMIT 1
            "#,
            action_plan.id
        )
        .fetch_optional(&state.db)
        .await?;

        let last_finished = sqlx::query_scalar!(
            r#"
            SELECT finished as "finished: i64"
            FROM action_plan_executions
            WHERE action_plan = $1
                AND finished > 0
            ORDER BY finished DESC
            LIMIT 1
            "#,
            action_plan.id
        )
        .fetch_optional(&state.db)
        .await?;

        action_plan_list.push(ActionPlanListItem {
            id: action_plan.id,
            name: action_plan.name,
            active_execution_id: active_execution_id.flatten(),
            last_finished_display: last_finished.map(format_unix_timestamp),
        });
    }

    let template = state
        .jinja
        .get_template("action_plan_list.html")
        .expect("template is loaded");
    let rendered = template.render(&ActionPlanList {
        action_plans: action_plan_list,
    })?;

    Ok(Html(rendered))
}

pub async fn new_get(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let plan = ActionPlanEdit {
        id: None,
        form_action: "/action_plan/new".to_string(),
        cancel_url: "/".to_string(),
        name: String::new(),
        items: Vec::new(),
    };

    edit_action_plan(&state, &plan)
}

#[derive(Serialize, Deserialize)]
pub struct ActionPlanForm {
    name: String,
    items: Option<Vec<String>>,
}

pub async fn new_post(
    State(state): State<AppState>,
    Form(form): Form<ActionPlanForm>,
) -> Result<Redirect, AppError> {
    let mut tx = state.db.begin().await?;

    let plan_id = Uuid::new_v4();

    sqlx::query!(
        "INSERT INTO action_plans (id, name) VALUES ($1, $2)",
        plan_id,
        form.name
    )
    .execute(&mut *tx)
    .await?;

    update_plan_items(tx, plan_id, form).await
}

pub async fn edit_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let plan = sqlx::query_as!(
        ActionPlan,
        r#"SELECT id as "id: uuid::Uuid", name FROM action_plans WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(plan) = plan else {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No action plan exists for id: {}",
            id
        )));
    };

    let items = sqlx::query_as!(
        ActionPlanItem,
        r#"
        SELECT actions.name as "name!"
        FROM action_items
        INNER JOIN actions ON actions.id = action_items.action
        WHERE action_items.action_plan = $1
        ORDER BY action_items.order_index ASC
        "#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    let plan = ActionPlanEdit {
        id: Some(plan.id),
        form_action: format!("/action_plan/{}/edit", plan.id),
        cancel_url: format!("/action_plan/{}", plan.id),
        name: plan.name,
        items,
    };

    edit_action_plan(&state, &plan)
}

pub async fn edit_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<ActionPlanForm>,
) -> Result<Redirect, AppError> {
    let mut tx = state.db.begin().await?;

    let update_result = sqlx::query!(
        "UPDATE action_plans SET name = $1 WHERE id = $2",
        form.name,
        id
    )
    .execute(&mut *tx)
    .await?;
    if update_result.rows_affected() == 0 {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No action plan exists for id: {}",
            id
        )));
    }

    update_plan_items(tx, id, form).await
}

async fn update_plan_items<'c>(
    mut tx: Transaction<'c, Sqlite>,
    plan_id: Uuid,
    form: ActionPlanForm,
) -> Result<Redirect, AppError> {
    sqlx::query!("DELETE FROM action_items WHERE action_plan = $1", plan_id)
        .execute(&mut *tx)
        .await?;

    let normalized_items = normalize_items(form.items);

    for (order, item) in normalized_items.iter().enumerate() {
        let action = sqlx::query!("SELECT id FROM actions WHERE name = $1", item)
            .fetch_optional(&mut *tx)
            .await?;

        let action = match action {
            Some(action) => Uuid::from_slice(&action.id)?,
            None => {
                let action_id = Uuid::new_v4();
                sqlx::query!(
                    "INSERT INTO actions (id, name) VALUES ($1, $2)",
                    action_id,
                    item
                )
                .execute(&mut *tx)
                .await?;

                action_id
            }
        };
        let order = order as i64;
        let item_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO action_items (id, order_index, action_plan, action) VALUES ($1, $2, $3, $4)",
            item_id,
            order,
            plan_id,
            action
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Redirect::to(&format!("/action_plan/{}", plan_id)))
}

pub async fn show_action_plan(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let plan = sqlx::query_as!(
        ActionPlan,
        r#"SELECT id as "id: uuid::Uuid", name FROM action_plans WHERE id = $1"#,
        id
    )
    .fetch_optional(&state.db)
    .await?;
    let Some(plan) = plan else {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No action plan exists for id: {}",
            id
        )));
    };

    let items = sqlx::query_as!(
        ActionPlanItem,
        r#"
        SELECT actions.name as "name!"
        FROM action_items
        INNER JOIN actions ON actions.id = action_items.action
        WHERE action_items.action_plan = $1
        ORDER BY action_items.order_index ASC
        "#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    let active_execution_rows = sqlx::query_as!(
        PlanExecutionActiveRow,
        r#"
        SELECT
            id as "id!: uuid::Uuid",
            started as "started!"
        FROM action_plan_executions
        WHERE action_plan = $1
            AND (finished IS NULL OR finished <= 0)
        ORDER BY started DESC
        "#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    let finished_execution_rows = sqlx::query_as!(
        PlanExecutionFinishedRow,
        r#"
        SELECT
            id as "id!: uuid::Uuid",
            started as "started!",
            finished as "finished!"
        FROM action_plan_executions
        WHERE action_plan = $1
            AND finished > 0
        ORDER BY finished DESC
        "#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    let active_executions: Vec<PlanExecutionActive> = active_execution_rows
        .into_iter()
        .map(|row| PlanExecutionActive {
            id: row.id,
            started_display: format_unix_timestamp(row.started),
        })
        .collect();

    let finished_executions: Vec<PlanExecutionFinished> = finished_execution_rows
        .into_iter()
        .map(|row| PlanExecutionFinished {
            id: row.id,
            started_display: format_unix_timestamp(row.started),
            finished_display: format_unix_timestamp(row.finished),
        })
        .collect();

    let active_execution_link = active_executions.first().map(|execution| execution.id);

    let plan = ActionPlanShow {
        id: plan.id,
        name: plan.name,
        items,
        active_executions,
        finished_executions,
        active_execution_link,
    };

    let template = state
        .jinja
        .get_template("action_plan_show.html")
        .expect("template is loaded");
    let rendered = template.render(&plan)?;

    Ok(Html(rendered))
}

#[derive(Serialize)]
pub struct ActionPlanEdit {
    id: Option<Uuid>,
    form_action: String,
    cancel_url: String,
    name: String,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
pub struct ActionPlanShow {
    id: Uuid,
    name: String,
    items: Vec<ActionPlanItem>,
    active_executions: Vec<PlanExecutionActive>,
    finished_executions: Vec<PlanExecutionFinished>,
    active_execution_link: Option<Uuid>,
}

#[derive(Serialize)]
pub struct ActionPlanItem {
    pub name: String,
}

#[derive(FromRow, Serialize)]
struct PlanExecutionActive {
    id: Uuid,
    started_display: String,
}

#[derive(FromRow, Serialize)]
struct PlanExecutionFinished {
    id: Uuid,
    started_display: String,
    finished_display: String,
}

#[derive(FromRow)]
struct PlanExecutionActiveRow {
    id: Uuid,
    started: i64,
}

#[derive(FromRow)]
struct PlanExecutionFinishedRow {
    id: Uuid,
    started: i64,
    finished: i64,
}

fn edit_action_plan(state: &AppState, plan: &ActionPlanEdit) -> Result<Html<String>, AppError> {
    let template = state
        .jinja
        .get_template("action_plan_edit.html")
        .expect("template is loaded");
    let rendered = template.render(plan)?;

    Ok(Html(rendered))
}

fn normalize_items(items: Option<Vec<String>>) -> Vec<String> {
    items
        .unwrap_or_else(|| Vec::new())
        .into_iter()
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}
