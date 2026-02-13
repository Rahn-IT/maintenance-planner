use axum::{
    extract::{Path, Query, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Sqlite, Transaction};
use std::collections::HashMap;
use uuid::Uuid;

use crate::{AppError, AppState, format_unix_timestamp};

#[derive(FromRow, Debug, Serialize)]
pub struct ActionPlan {
    pub id: uuid::Uuid,
    pub name: String,
    pub deleted_at: Option<i64>,
}

#[derive(Serialize)]
pub struct ActionPlanList {
    action_plans: Vec<ActionPlanListItem>,
    current_sort: String,
    show_deleted: bool,
}

#[derive(Serialize)]
pub struct ActionPlanListItem {
    id: Uuid,
    name: String,
    active_execution_id: Option<Uuid>,
    last_finished_display: Option<String>,
}

pub async fn index(
    State(state): State<AppState>,
    Query(query): Query<ActionPlanListQuery>,
) -> Result<Html<String>, AppError> {
    let sort = query.sort.unwrap_or_else(|| "name".to_string());
    let show_deleted = query.deleted.unwrap_or(false);

    let action_plans = if show_deleted {
        sqlx::query_as!(
            ActionPlan,
            r#"
            SELECT
                id as "id: uuid::Uuid",
                name,
                deleted_at as "deleted_at?"
            FROM action_plans
            WHERE deleted_at > 0
            "#
        )
        .fetch_all(&state.db)
        .await?
    } else {
        sqlx::query_as!(
            ActionPlan,
            r#"
            SELECT
                id as "id: uuid::Uuid",
                name,
                deleted_at as "deleted_at?"
            FROM action_plans
            WHERE deleted_at IS NULL OR deleted_at <= 0
            "#
        )
        .fetch_all(&state.db)
        .await?
    };

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

        let last_execution = sqlx::query_scalar!(
            r#"
            SELECT started as "started: i64"
            FROM action_plan_executions
            WHERE action_plan = $1
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

        action_plan_list.push(ActionPlanListSortItem {
            id: action_plan.id,
            name: action_plan.name,
            active_execution_id: active_execution_id.flatten(),
            last_finished_display: last_finished.flatten().map(format_unix_timestamp),
            last_execution_unix: last_execution,
        });
    }

    match sort.as_str() {
        "last_execution_desc" => {
            action_plan_list.sort_by(|a, b| b.last_execution_unix.cmp(&a.last_execution_unix));
        }
        "last_execution_asc" => {
            action_plan_list.sort_by(|a, b| a.last_execution_unix.cmp(&b.last_execution_unix));
        }
        _ => {
            action_plan_list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
    }

    let action_plans = action_plan_list
        .into_iter()
        .map(|item| ActionPlanListItem {
            id: item.id,
            name: item.name,
            active_execution_id: item.active_execution_id,
            last_finished_display: item.last_finished_display,
        })
        .collect();

    let template = state
        .jinja
        .get_template("action_plan_list.html")
        .expect("template is loaded");
    let rendered = template.render(&ActionPlanList {
        action_plans,
        current_sort: sort,
        show_deleted,
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
        "INSERT INTO action_plans (id, name, deleted_at) VALUES ($1, $2, NULL)",
        plan_id,
        form.name
    )
    .execute(&mut *tx)
    .await?;

    update_plan_items(tx, plan_id, form, None).await
}

pub async fn edit_get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<EditContext>,
) -> Result<Html<String>, AppError> {
    let execution_id = query.execution_id;

    let plan = sqlx::query_as!(
        ActionPlan,
        r#"
        SELECT
            id as "id: uuid::Uuid",
            name,
            deleted_at as "deleted_at?"
        FROM action_plans
        WHERE id = $1
            AND (deleted_at IS NULL OR deleted_at <= 0)
        "#,
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
        form_action: if let Some(execution_id) = execution_id {
            format!("/action_plan/{}/edit?execution_id={}", plan.id, execution_id)
        } else {
            format!("/action_plan/{}/edit", plan.id)
        },
        cancel_url: if let Some(execution_id) = execution_id {
            format!("/executions/{}", execution_id)
        } else {
            format!("/action_plan/{}", plan.id)
        },
        name: plan.name,
        items,
    };

    edit_action_plan(&state, &plan)
}

pub async fn edit_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<EditContext>,
    Form(form): Form<ActionPlanForm>,
) -> Result<Redirect, AppError> {
    let execution_id = query.execution_id;
    let mut tx = state.db.begin().await?;

    let update_result = sqlx::query!(
        "UPDATE action_plans SET name = $1 WHERE id = $2 AND (deleted_at IS NULL OR deleted_at <= 0)",
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

    update_plan_items(tx, id, form, execution_id).await
}

async fn update_plan_items<'c>(
    mut tx: Transaction<'c, Sqlite>,
    plan_id: Uuid,
    form: ActionPlanForm,
    execution_id: Option<Uuid>,
) -> Result<Redirect, AppError> {
    let mut execution_state_by_name: HashMap<String, Option<i64>> = HashMap::new();

    if let Some(execution_id) = execution_id {
        let execution_items = sqlx::query!(
            r#"
            SELECT
                actions.name as "name!",
                action_item_executions.finished as "finished?"
            FROM action_item_executions
            INNER JOIN actions ON actions.id = action_item_executions.action
            WHERE action_item_executions.action_plan_execution = $1
            "#,
            execution_id
        )
        .fetch_all(&mut *tx)
        .await?;

        for item in execution_items {
            execution_state_by_name.insert(item.name, item.finished);
        }
        sqlx::query!(
            r#"
            DELETE FROM action_item_executions
            WHERE action_plan_execution = $1
            "#,
            execution_id
        )
        .execute(&mut *tx)
        .await?;
    }

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

    if let Some(execution_id) = execution_id {
        let new_plan_items = sqlx::query!(
            r#"
            SELECT
                action_items.action as "action_id: uuid::Uuid",
                action_items.order_index,
                actions.name as "name!"
            FROM action_items
            INNER JOIN actions ON actions.id = action_items.action
            WHERE action_items.action_plan = $1
            ORDER BY action_items.order_index ASC
            "#,
            plan_id
        )
        .fetch_all(&mut *tx)
        .await?;

        for item in new_plan_items {
            let execution_item_id = Uuid::new_v4();
            let finished = execution_state_by_name.get(&item.name).cloned().flatten();

            sqlx::query!(
                r#"
                INSERT INTO action_item_executions (id, action, order_index, action_plan_execution, finished)
                VALUES ($1, $2, $3, $4, $5)
                "#,
                execution_item_id,
                item.action_id,
                item.order_index,
                execution_id,
                finished
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    if let Some(execution_id) = execution_id {
        Ok(Redirect::to(&format!("/executions/{}", execution_id)))
    } else {
        Ok(Redirect::to(&format!("/action_plan/{}", plan_id)))
    }
}

pub async fn show_action_plan(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let plan = sqlx::query_as!(
        ActionPlan,
        r#"
        SELECT
            id as "id: uuid::Uuid",
            name,
            deleted_at as "deleted_at?"
        FROM action_plans
        WHERE id = $1
        "#,
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
        is_deleted: plan.deleted_at.map(|value| value > 0).unwrap_or(false),
        deleted_at_display: plan
            .deleted_at
            .filter(|value| *value > 0)
            .map(format_unix_timestamp),
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

pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let now = unix_now();
    let result = sqlx::query!(
        r#"
        UPDATE action_plans
        SET deleted_at = $1
        WHERE id = $2
            AND (deleted_at IS NULL OR deleted_at <= 0)
        "#,
        now,
        id
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No active action plan exists for id: {}",
            id
        )));
    }

    Ok(Redirect::to("/"))
}

pub async fn undelete_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let result = sqlx::query!(
        r#"
        UPDATE action_plans
        SET deleted_at = NULL
        WHERE id = $1
            AND deleted_at > 0
        "#,
        id
    )
    .execute(&state.db)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found_for("Action Plan", format!(
            "No deleted action plan exists for id: {}",
            id
        )));
    }

    Ok(Redirect::to(&format!("/action_plan/{}", id)))
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
    is_deleted: bool,
    deleted_at_display: Option<String>,
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

#[derive(Debug, Default, Deserialize)]
pub struct EditContext {
    execution_id: Option<Uuid>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ActionPlanListQuery {
    sort: Option<String>,
    deleted: Option<bool>,
}

struct ActionPlanListSortItem {
    id: Uuid,
    name: String,
    active_execution_id: Option<Uuid>,
    last_finished_display: Option<String>,
    last_execution_unix: Option<i64>,
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
