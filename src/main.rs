use std::{path::Path, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};

mod action_plan;

const DB_PATH: &str = "./db/db.sqlite";

#[derive(Debug, Clone)]
struct AppState {
    db: SqlitePool,
    jinja: Arc<minijinja::Environment<'static>>,
}

#[derive(Debug)]
struct AppError(anyhow::Error);
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
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
        .route("/action_plan/new", get(action_plan::new_get))
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
