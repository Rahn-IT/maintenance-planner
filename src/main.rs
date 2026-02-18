use std::{path::Path, sync::Arc};

use axum::{
    Router,
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    http::{HeaderValue, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use axum_extra::extract::cookie::CookieJar;
use chrono::{Local, TimeZone};
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};
use tokio::{signal, time::Duration};
use uuid::Uuid;

mod action_plan;
mod backup;
mod executions;
mod users;

const DB_PATH: &str = "./db/db.sqlite";

#[derive(Debug, Clone)]
struct AppState {
    db: SqlitePool,
    jinja: Arc<minijinja::Environment<'static>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CurrentUser {
    pub(crate) id: Uuid,
    pub(crate) name: String,
    pub(crate) is_admin: bool,
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

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
            not_found_title: None,
        }
    }

    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
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
        if self.status == StatusCode::NOT_FOUND
            || self.status == StatusCode::CONFLICT
            || self.status == StatusCode::FORBIDDEN
            || self.status == StatusCode::UNAUTHORIZED
        {
            let (title, button_label, button_href): (String, &str, &str) =
                if self.status == StatusCode::NOT_FOUND {
                    (
                        format!(
                            "{} Not Found",
                            self.not_found_title.as_deref().unwrap_or("Site")
                        ),
                        "Back Home",
                        "/",
                    )
                } else if self.status == StatusCode::FORBIDDEN {
                    ("Forbidden".to_string(), "Back Home", "/")
                } else if self.status == StatusCode::UNAUTHORIZED {
                    ("Unauthorized".to_string(), "Login", "/login")
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
    run_session_gc(&db).await;
    tokio::spawn(run_action_gc_scheduler(db.clone()));
    tokio::spawn(run_session_gc_scheduler(db.clone()));

    let mut jinja = minijinja::Environment::new();
    minijinja_embed::load_templates!(&mut jinja);

    let state = AppState {
        db: db.clone(),
        jinja: Arc::new(jinja),
    };

    // build our application with a route
    let app = router()
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .with_state(state);

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
    let admin_routes = Router::new()
        .route("/backup", get(backup::index))
        .route("/backup/export.json", get(backup::export_json))
        .route("/backup/import", post(backup::import_json))
        .route("/users", get(users::index).post(users::create_post))
        .route("/users/{id}/delete", post(users::delete_post))
        .route_layer(middleware::from_extractor::<RequireAdmin>());

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
        .route("/action_plan/{id}/delete", post(action_plan::delete_post))
        .route(
            "/action_plan/{id}/undelete",
            post(action_plan::undelete_post),
        )
        .route("/action_plan/new", get(action_plan::new_get))
        .route("/action_plan/new", post(action_plan::new_post))
        .route("/action_plan/{id}/edit", get(action_plan::edit_get))
        .route("/action_plan/{id}/edit", post(action_plan::edit_post))
        .route("/actions/search", get(action_plan::search_actions))
        .route("/setup", get(users::setup_get).post(users::setup_post))
        .route("/login", get(users::login_get).post(users::login_post))
        .route("/logout", post(users::logout_post))
        .merge(admin_routes)
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
        .route(
            "/static/action_item_search.js",
            get((
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static(mime::APPLICATION_JAVASCRIPT_UTF_8.as_ref()),
                )],
                include_bytes!("../assets/static/action_item_search.js"),
            )),
        )
}

struct RequireAdmin;

impl<S> FromRequestParts<S> for RequireAdmin
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        let current_user = parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or_else(|| AppError::unauthorized("Authentication required."))?;

        if current_user.is_admin {
            Ok(Self)
        } else {
            Err(AppError::forbidden(
                "Only admin users can access this endpoint.",
            ))
        }
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or_else(|| AppError::unauthorized("Authentication required."))
    }
}

async fn auth_middleware(
    State(state): State<AppState>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if path.starts_with("/static/") {
        return next.run(request).await;
    }

    let has_users = match users::has_users(&state.db).await {
        Ok(value) => value,
        Err(err) => return err.into_response(),
    };

    if !has_users {
        if path == "/setup" {
            return next.run(request).await;
        }
        return axum::response::Redirect::to("/setup").into_response();
    }

    if path == "/setup" {
        return axum::response::Redirect::to("/login").into_response();
    }

    if path == "/login" {
        return next.run(request).await;
    }

    let session_id = match users::read_session_cookie(&jar) {
        Some(id) => id,
        None => return axum::response::Redirect::to("/login").into_response(),
    };

    let current_user = match users::resolve_current_user_from_session(&state.db, session_id).await {
        Ok(Some(user)) => user,
        Ok(None) => return axum::response::Redirect::to("/login").into_response(),
        Err(err) => return err.into_response(),
    };

    request.extensions_mut().insert(current_user);
    next.run(request).await
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

async fn run_session_gc_scheduler(db: SqlitePool) {
    let mut interval = tokio::time::interval(Duration::from_secs(60 * 60));
    interval.tick().await;

    loop {
        interval.tick().await;
        run_session_gc(&db).await;
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

async fn run_session_gc(db: &SqlitePool) {
    match users::cleanup_expired_sessions(db).await {
        Ok(0) => {
            println!("Session GC: no expired sessions found.");
        }
        Ok(count) => {
            println!("Session GC: deleted {} expired session(s).", count);
        }
        Err(err) => {
            eprintln!("Session GC failed: {}", err.message);
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
