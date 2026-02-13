use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::{AppError, AppState};

#[derive(FromRow, Debug, Serialize)]
pub struct ActionPlan {
    pub id: uuid::Uuid,
    pub name: String,
}

#[derive(Serialize)]
pub struct ActionPlanList {
    action_plans: Vec<ActionPlan>,
}

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let action_plans = sqlx::query_as!(
        ActionPlan,
        r#"SELECT id as "id: uuid::Uuid", name FROM action_plans"#
    )
    .fetch_all(&state.db)
    .await?;

    let template = state
        .jinja
        .get_template("action_plan_list.html")
        .expect("template is loaded");
    let rendered = template.render(&ActionPlanList { action_plans })?;

    Ok(Html(rendered))
}

pub async fn new_get(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let plan = ActionPlanEdit {
        id: None,
        form_action: "/action_plan/new".to_string(),
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
        return Err(AppError::not_found(format!(
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
        return Err(AppError::not_found(format!(
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
        return Err(AppError::not_found(format!(
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

    let plan = ActionPlanShow {
        id: plan.id,
        name: plan.name,
        items,
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
    name: String,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
pub struct ActionPlanShow {
    id: Uuid,
    name: String,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
pub struct ActionPlanItem {
    pub name: String,
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
