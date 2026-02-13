use std::{path::Path, sync::Arc};

use axum::{
    Router,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};

mod action_plan;
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
}

impl AppError {
    fn internal<E>(err: E) -> Self
    where
        E: Into<anyhow::Error>,
    {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: err.into().to_string(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
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
            let (title, button_label, button_href) = if self.status == StatusCode::NOT_FOUND {
                ("Action Plan Not Found", "Back Home", "/")
            } else {
                ("Cannot Save Changes", "Back Home", "/")
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

    let mut jinja = minijinja::Environment::new();
    minijinja_embed::load_templates!(&mut jinja);

    let state = AppState {
        db,
        jinja: Arc::new(jinja),
    };

    // build our application with a route
    let app = router().with_state(state);

    // run our app with hyper, listening globally on port 3000
    let addr = "0.0.0.0:4040";
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Starting webserver on: http://{}", addr);
    axum::serve(listener, app).await.unwrap();
}

fn router() -> Router<AppState> {
    Router::new()
        // `GET /` goes to `root`
        .route("/", get(action_plan::index))
        .route("/executions", get(executions::index))
        .route("/executions/{id}", get(executions::show))
        .route("/action_plan/{id}", get(action_plan::show_action_plan))
        .route("/action_plan/{id}/execute", post(executions::create_post))
        .route("/action_plan/new", get(action_plan::new_get))
        .route("/action_plan/new", post(action_plan::new_post))
        .route("/action_plan/{id}/edit", get(action_plan::edit_get))
        .route("/action_plan/{id}/edit", post(action_plan::edit_post))
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
