use axum::{
    extract::{Path, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
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
        name: String::new(),
        items: Vec::new(),
    };

    edit_action_plan(state, &plan)
}

#[derive(Serialize, Deserialize)]
pub struct ActionPlanForm {
    name: String,
    items: Vec<String>,
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

    for (order, item) in form.items.iter().enumerate() {
        let order = order as i32;
        let item_id = Uuid::new_v4();
        sqlx::query!(
            "INSERT INTO action_items (id, order_index, action_plan, name) VALUES ($1, $2, $3, $4)",
            item_id,
            order,
            plan_id,
            item
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(Redirect::to(&format!("/action_plan/{}", &plan_id)))
}

pub async fn edit_get(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    let mut tx = state.db.begin().await?;
    todo!()
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
    .fetch_one(&state.db)
    .await?;

    let items = sqlx::query_as!(
        ActionPlanItem,
        r#"SELECT name FROM action_items WHERE action_plan = $1 ORDER BY order_index ASC"#,
        id
    )
    .fetch_all(&state.db)
    .await?;

    let plan = ActionPlanEdit {
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
    name: String,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
pub struct ActionPlanItem {
    name: String,
}

fn edit_action_plan(state: AppState, plan: &ActionPlanEdit) -> Result<Html<String>, AppError> {
    let template = state
        .jinja
        .get_template("action_plan_edit.html")
        .expect("template is loaded");
    let rendered = template.render(plan)?;

    Ok(Html(rendered))
}
