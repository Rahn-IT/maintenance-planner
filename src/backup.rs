use std::collections::HashMap;

use axum::{
    Json,
    extract::{Multipart, State},
    http::{HeaderValue, header},
    response::{Html, IntoResponse},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, AppState};

pub async fn index(State(state): State<AppState>) -> Result<Html<String>, AppError> {
    render_backup_page(&state, None)
}

fn render_backup_page(state: &AppState, notice: Option<BackupNotice>) -> Result<Html<String>, AppError> {
    let template = state
        .jinja
        .get_template("backup.html")
        .expect("template is loaded");
    let rendered = template.render(BackupPageView { notice })?;
    Ok(Html(rendered))
}

pub async fn export_json(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let plans = sqlx::query!(
        r#"
        SELECT
            id as "id: uuid::Uuid",
            name,
            deleted_at as "deleted_at?"
        FROM action_plans
        ORDER BY name ASC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let mut action_plans = Vec::with_capacity(plans.len());
    for plan in plans {
        let items = sqlx::query!(
            r#"
            SELECT
                action_items.order_index as "order_index!",
                actions.name as "action_name!"
            FROM action_items
            INNER JOIN actions ON actions.id = action_items.action
            WHERE action_items.action_plan = $1
            ORDER BY action_items.order_index ASC
            "#,
            plan.id
        )
        .fetch_all(&state.db)
        .await?;

        action_plans.push(BackupActionPlan {
            id: plan.id,
            name: plan.name,
            deleted_at: plan.deleted_at,
            items: items
                .into_iter()
                .map(|item| BackupPlanItem {
                    order_index: item.order_index,
                    action_name: item.action_name,
                })
                .collect(),
        });
    }

    let executions = sqlx::query!(
        r#"
        SELECT
            id as "id!: uuid::Uuid",
            action_plan as "action_plan: uuid::Uuid",
            started as "started!",
            finished as "finished?"
        FROM action_plan_executions
        ORDER BY started DESC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let mut action_plan_executions = Vec::with_capacity(executions.len());
    for execution in executions {
        let items = sqlx::query!(
            r#"
            SELECT
                action_item_executions.order_index as "order_index!",
                actions.name as "action_name!",
                action_item_executions.finished as "finished?"
            FROM action_item_executions
            INNER JOIN actions ON actions.id = action_item_executions.action
            WHERE action_item_executions.action_plan_execution = $1
            ORDER BY action_item_executions.order_index ASC
            "#,
            execution.id
        )
        .fetch_all(&state.db)
        .await?;

        action_plan_executions.push(BackupExecution {
            id: execution.id,
            action_plan: execution.action_plan,
            started: execution.started,
            finished: execution.finished,
            items: items
                .into_iter()
                .map(|item| BackupExecutionItem {
                    order_index: item.order_index,
                    action_name: item.action_name,
                    finished: item.finished,
                })
                .collect(),
        });
    }

    let backup = BackupFile {
        version: 1,
        exported_at_unix: unix_now(),
        action_plans,
        action_plan_executions,
    };

    Ok((
        [(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment; filename=\"maintenance-planner-backup.json\""),
        )],
        Json(backup),
    ))
}

pub async fn import_json(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Html<String>, AppError> {
    let mut backup_bytes = None;

    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("backup_file") {
            backup_bytes = Some(field.bytes().await?);
            break;
        }
    }

    let Some(backup_bytes) = backup_bytes else {
        return render_backup_page(
            &state,
            Some(BackupNotice::error("No backup file selected.")),
        );
    };

    let backup = match Json::<BackupFile>::from_bytes(backup_bytes.as_ref()) {
        Ok(Json(backup)) => backup,
        Err(_) => {
            return render_backup_page(
                &state,
                Some(BackupNotice::error(
                    "The uploaded file is not valid backup JSON.",
                )),
            );
        }
    };

    if backup.version != 1 {
        return render_backup_page(
            &state,
            Some(BackupNotice::error(format!(
                "Unsupported backup version: {}",
                backup.version
            ))),
        );
    }

    let mut plan_ids = std::collections::HashSet::with_capacity(backup.action_plans.len());
    for plan in &backup.action_plans {
        if !plan_ids.insert(plan.id) {
            return render_backup_page(
                &state,
                Some(BackupNotice::error(format!(
                    "Duplicate action plan id in backup: {}",
                    plan.id
                ))),
            );
        }
    }

    for execution in &backup.action_plan_executions {
        if !plan_ids.contains(&execution.action_plan) {
            return render_backup_page(
                &state,
                Some(BackupNotice::error(format!(
                    "Execution {} references unknown action plan {}",
                    execution.id, execution.action_plan
                ))),
            );
        }
    }

    let mut tx = state.db.begin().await?;

    sqlx::query!("DELETE FROM action_item_executions")
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM action_plan_executions")
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM action_items")
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM action_plans")
        .execute(&mut *tx)
        .await?;
    sqlx::query!("DELETE FROM actions").execute(&mut *tx).await?;

    let mut action_by_name: HashMap<String, Uuid> = HashMap::new();

    for plan in &backup.action_plans {
        sqlx::query!(
            "INSERT INTO action_plans (id, name, deleted_at) VALUES ($1, $2, $3)",
            plan.id,
            plan.name,
            plan.deleted_at
        )
        .execute(&mut *tx)
        .await?;

        for item in &plan.items {
            let action_id =
                ensure_action_id(&mut tx, &mut action_by_name, item.action_name.as_str()).await?;

            let item_id = Uuid::new_v4();
            sqlx::query!(
                "INSERT INTO action_items (id, order_index, action_plan, action) VALUES ($1, $2, $3, $4)",
                item_id,
                item.order_index,
                plan.id,
                action_id
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    for execution in &backup.action_plan_executions {
        sqlx::query!(
            "INSERT INTO action_plan_executions (id, action_plan, started, finished) VALUES ($1, $2, $3, $4)",
            execution.id,
            execution.action_plan,
            execution.started,
            execution.finished
        )
        .execute(&mut *tx)
        .await?;

        for item in &execution.items {
            let action_id =
                ensure_action_id(&mut tx, &mut action_by_name, item.action_name.as_str()).await?;

            let item_id = Uuid::new_v4();
            sqlx::query!(
                "INSERT INTO action_item_executions (id, action, order_index, action_plan_execution, finished) VALUES ($1, $2, $3, $4, $5)",
                item_id,
                action_id,
                item.order_index,
                execution.id,
                item.finished
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    render_backup_page(
        &state,
        Some(BackupNotice::success(format!(
            "Backup imported. Restored {} action plan(s) and {} execution(s).",
            backup.action_plans.len(),
            backup.action_plan_executions.len()
        ))),
    )
}

async fn ensure_action_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    action_by_name: &mut HashMap<String, Uuid>,
    action_name: &str,
) -> Result<Uuid, AppError> {
    if let Some(id) = action_by_name.get(action_name) {
        return Ok(*id);
    }

    let action_id = Uuid::new_v4();
    sqlx::query!(
        "INSERT INTO actions (id, name) VALUES ($1, $2)",
        action_id,
        action_name
    )
    .execute(&mut **tx)
    .await?;

    action_by_name.insert(action_name.to_string(), action_id);
    Ok(action_id)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupFile {
    version: i64,
    exported_at_unix: i64,
    action_plans: Vec<BackupActionPlan>,
    action_plan_executions: Vec<BackupExecution>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupActionPlan {
    id: Uuid,
    name: String,
    deleted_at: Option<i64>,
    items: Vec<BackupPlanItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupPlanItem {
    order_index: i64,
    action_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupExecution {
    id: Uuid,
    action_plan: Uuid,
    started: i64,
    finished: Option<i64>,
    items: Vec<BackupExecutionItem>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BackupExecutionItem {
    order_index: i64,
    action_name: String,
    finished: Option<i64>,
}

#[derive(Debug, Serialize)]
struct BackupPageView {
    notice: Option<BackupNotice>,
}

#[derive(Debug, Serialize)]
struct BackupNotice {
    message: String,
    is_error: bool,
}

impl BackupNotice {
    fn success(message: String) -> Self {
        Self {
            message,
            is_error: false,
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            is_error: true,
        }
    }
}
