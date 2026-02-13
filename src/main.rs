use std::{path::Path, sync::Arc};

use axum::{
    Router,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use chrono::{Local, TimeZone};
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};
use tokio::{signal, time::Duration};
use uuid::Uuid;

mod action_plan;
mod backup;
mod executions;

const DB_PATH: &str = "./db/db.sqlite";

#[derive(Debug, Clone)]
struct AppState {
    db: SqlitePool,
    jinja: Arc<minijinja::Environment<'static>>,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
    not_found_title: Option<String>,
}

impl AppError {
    fn internal<E>(err: E) -> Self
    where
        E: Into<anyhow::Error>,
    {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.into().to_string(),
            not_found_title: None,
        }
    }

    pub fn not_found_for(title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
            not_found_title: Some(title.into()),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
            not_found_title: None,
        }
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::internal(err)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if self.status == StatusCode::NOT_FOUND || self.status == StatusCode::CONFLICT {
            let (title, button_label, button_href): (String, &str, &str) = if self.status
                == StatusCode::NOT_FOUND
            {
                (
                    format!(
                        "{} Not Found",
                        self.not_found_title.as_deref().unwrap_or("Site")
                    ),
                    "Back Home",
                    "/",
                )
            } else {
                ("Cannot Save Changes".to_string(), "Back Home", "/")
            };

            let html = format!(
                r#"<!doctype html>
<html>
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Not Found</title>
    <link rel="stylesheet" href="/static/style.css" />
    <script src="/static/script.js"></script>
  </head>
  <body>
    <nav class="top-nav">
      <div class="top-nav-inner">
        <div class="nav-left">
          <a class="brand" href="/">Maintenance Planner</a>
          <a class="nav-link" href="/">Home</a>
          <a class="nav-link" href="/executions">Executions</a>
        </div>
        <a class="nav-link" href="/action_plan/new">New Plan</a>
      </div>
    </nav>
    <main class="page">
      <section class="content-card">
        <h1 class="page-title">{}</h1>
        <p class="muted">{}</p>
        <div class="toolbar">
          <a class="btn btn-primary" href="{}">{}</a>
        </div>
      </section>
    </main>
  </body>
</html>"#,
                title, self.message, button_href, button_label
            );

            return (
                self.status,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::TEXT_HTML_UTF_8.as_ref()),
                )],
                html,
            )
                .into_response();
        }

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.message),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() {
    if !tokio::fs::try_exists(DB_PATH).await.unwrap() {
        tokio::fs::create_dir_all(Path::new(DB_PATH).parent().unwrap())
            .await
            .unwrap();
        Sqlite::create_database(DB_PATH).await.unwrap();
    }

    let db = SqlitePool::connect(DB_PATH).await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    run_action_gc(&db).await;
    tokio::spawn(run_action_gc_scheduler(db.clone()));

    let mut jinja = minijinja::Environment::new();
    minijinja_embed::load_templates!(&mut jinja);

    let state = AppState {
        db: db.clone(),
        jinja: Arc::new(jinja),
    };

    // build our application with a route
    let app = router().with_state(state);

    // run our app with hyper, listening globally on port 3000
    let addr = "0.0.0.0:4040";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Starting webserver on: http://{}", addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = signal::ctrl_c().await;
        })
        .await
        .unwrap();
    println!("Shutting down");
    db.close().await;
}

fn router() -> Router<AppState> {
    Router::new()
        // `GET /` goes to `root`
        .route("/", get(action_plan::index))
        .route("/executions", get(executions::index))
        .route("/executions/{id}", get(executions::show))
        .route("/executions/{id}/complete", get(executions::complete_get))
        .route("/executions/{id}/reopen", get(executions::reopen_get))
        .route(
            "/executions/{id}/delete",
            get(executions::delete_get).post(executions::delete_post),
        )
        .route(
            "/execution-items/{id}/finished",
            post(executions::set_item_finished_post),
        )
        .route("/action_plan_execution/{id}", get(executions::show))
        .route("/action_plan/{id}", get(action_plan::show_action_plan))
        .route("/action_plan/{id}/execute", post(executions::create_post))
        .route("/action_plan/new", get(action_plan::new_get))
        .route("/action_plan/new", post(action_plan::new_post))
        .route("/action_plan/{id}/edit", get(action_plan::edit_get))
        .route("/action_plan/{id}/edit", post(action_plan::edit_post))
        .route("/backup", get(backup::index))
        .route("/backup/export.json", get(backup::export_json))
        .route("/backup/import", post(backup::import_json))
        .route(
            "/static/style.css",
            get((
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::TEXT_CSS_UTF_8.as_ref()),
                )],
                include_bytes!("../assets/static/style.css"),
            )),
        )
        .route(
            "/static/script.js",
            get((
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::APPLICATION_JAVASCRIPT_UTF_8.as_ref()),
                )],
                include_bytes!("../assets/static/script.js"),
            )),
        )
}

pub fn format_unix_timestamp(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "Unknown".to_string();
    }

    match Local.timestamp_opt(timestamp, 0).single() {
        Some(datetime) => datetime.format("%Y-%m-%d %H:%M").to_string(),
        None => "Unknown".to_string(),
    }
}

#[derive(Debug)]
struct UnusedAction {
    id: Uuid,
    name: String,
}

async fn run_action_gc_scheduler(db: SqlitePool) {
    let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
    interval.tick().await;

    loop {
        interval.tick().await;
        run_action_gc(&db).await;
    }
}

async fn run_action_gc(db: &SqlitePool) {
    match collect_and_delete_unused_actions(db).await {
        Ok(unused_actions) if unused_actions.is_empty() => {
            println!("Action GC: no unused actions found.");
        }
        Ok(unused_actions) => {
            let action_labels = unused_actions
                .iter()
                .map(|action| format!("{} ({})", action.name, action.id))
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "Action GC: deleted {} unused action(s): {}",
                unused_actions.len(),
                action_labels
            );
        }
        Err(err) => {
            eprintln!("Action GC failed: {}", err);
        }
    }
}

async fn collect_and_delete_unused_actions(db: &SqlitePool) -> anyhow::Result<Vec<UnusedAction>> {
    let mut tx = db.begin().await?;

    let unused_actions = sqlx::query!(
        r#"
        SELECT
            actions.id as "id: uuid::Uuid",
            actions.name
        FROM actions
        WHERE NOT EXISTS (
            SELECT 1
            FROM action_items
            WHERE action_items.action = actions.id
        )
        AND NOT EXISTS (
            SELECT 1
            FROM action_item_executions
            WHERE action_item_executions.action = actions.id
        )
        "#
    )
    .fetch_all(&mut *tx)
    .await?;

    for action in &unused_actions {
        sqlx::query!("DELETE FROM actions WHERE id = $1", action.id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;

    Ok(unused_actions
        .into_iter()
        .map(|action| UnusedAction {
            id: action.id,
            name: action.name,
        })
        .collect())
}
