use std::{path::Path, sync::Arc};

use axum::{
    Router,
    extract::State,
    http::{HeaderValue, header},
    response::Html,
    routing::get,
};
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};

const DB_PATH: &str = "./db/db.sqlite";

#[derive(Debug, Clone)]
struct AppState {
    db: SqlitePool,
    jinja: Arc<minijinja::Environment<'static>>,
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
    println!("Starting contact injector on: {}", addr);
    axum::serve(listener, app).await.unwrap();
}

fn router() -> Router<AppState> {
    Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
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
}

async fn root(State(state): State<AppState>) -> Html<String> {
    let template = state
        .jinja
        .get_template("home.html")
        .expect("template is loaded");
    let rendered = template.render(()).unwrap();
    Html(rendered)
}
