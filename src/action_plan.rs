use axum::{
    extract::State,
    response::{Html, Redirect},
};
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
    pub action_plans: Vec<ActionPlan>,
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

#[derive(Deserialize)]
pub struct ActionPlanForm {
    name: String,
    items: Vec<String>,
}

pub async fn new_post(State(state): State<AppState>, form: ActionPlanForm) -> Result<Redirect, AppError> {
    todo!()
}

#[derive(Serialize)]
pub struct ActionPlanEdit {
    name: String,
    items: Vec<ActionPlanItem>,
}

#[derive(Serialize)]
pub struct ActionPlanItem {
    id: Option<Uuid>,
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
