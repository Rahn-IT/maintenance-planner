use std::collections::HashSet;

use axum::{
    Json,
    extract::{Path, Query, State},
    response::{Html, Redirect},
};
use axum_extra::extract::Form;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::prelude::FromRow;
use uuid::Uuid;

use crate::{AppError, AppState, CurrentUser};

#[derive(Debug, Clone, Serialize)]
pub struct TagBadge {
    pub id: Uuid,
    pub name: String,
    pub color_style: String,
}

#[derive(Serialize)]
struct TagsPageView {
    tags: Vec<TagBadge>,
    is_admin: bool,
}

#[derive(Serialize)]
struct DeleteTagConfirmView {
    id: Uuid,
    name: String,
    usage_count: i64,
    is_admin: bool,
}

#[derive(FromRow)]
struct TagRow {
    id: Uuid,
    name: String,
}

#[derive(Deserialize)]
pub struct CreateTagForm {
    name: String,
}

#[derive(Deserialize)]
pub struct UpdateTagForm {
    name: String,
}

#[derive(Deserialize)]
pub struct TagSearchQuery {
    q: Option<String>,
}

pub async fn index(
    State(state): State<AppState>,
    current_user: CurrentUser,
) -> Result<Html<String>, AppError> {
    let tags = fetch_all_badges(&state.db).await?;
    let template = state
        .jinja
        .get_template("tags.html")
        .expect("template is loaded");
    let rendered = template.render(TagsPageView {
        tags,
        is_admin: current_user.is_admin,
    })?;

    Ok(Html(rendered))
}

pub async fn create_post(
    State(state): State<AppState>,
    Form(form): Form<CreateTagForm>,
) -> Result<Redirect, AppError> {
    let name = normalize_tag_name(&form.name)?;
    ensure_name_available(&state.db, &name, None).await?;
    let tag_id = Uuid::new_v4();

    sqlx::query!("INSERT INTO tags (id, name) VALUES ($1, $2)", tag_id, name)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/tags"))
}

pub async fn edit_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Form(form): Form<UpdateTagForm>,
) -> Result<Redirect, AppError> {
    let name = normalize_tag_name(&form.name)?;
    ensure_name_available(&state.db, &name, Some(id)).await?;

    let result = sqlx::query!("UPDATE tags SET name = $1 WHERE id = $2", name, id)
        .execute(&state.db)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found_for(
            "Tag",
            format!("No tag exists for id: {}", id),
        ));
    }

    Ok(Redirect::to("/tags"))
}

pub async fn delete_get(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, AppError> {
    let tag = sqlx::query!(
        r#"
        SELECT
            tags.id as "id: uuid::Uuid",
            tags.name,
            COUNT(action_plan_tags.action_plan) as "usage_count!: i64"
        FROM tags
        LEFT JOIN action_plan_tags ON action_plan_tags.tag = tags.id
        WHERE tags.id = $1
        GROUP BY tags.id, tags.name
        LIMIT 1
        "#,
        id
    )
    .fetch_optional(&state.db)
    .await?;

    let Some(tag) = tag else {
        return Err(AppError::not_found_for(
            "Tag",
            format!("No tag exists for id: {}", id),
        ));
    };

    let template = state
        .jinja
        .get_template("tag_delete_confirm.html")
        .expect("template is loaded");
    let rendered = template.render(DeleteTagConfirmView {
        id: tag.id,
        name: tag.name,
        usage_count: tag.usage_count,
        is_admin: current_user.is_admin,
    })?;

    Ok(Html(rendered))
}

pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Redirect, AppError> {
    let mut tx = state.db.begin().await?;

    sqlx::query!("DELETE FROM action_plan_tags WHERE tag = $1", id)
        .execute(&mut *tx)
        .await?;

    let result = sqlx::query!("DELETE FROM tags WHERE id = $1", id)
        .execute(&mut *tx)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found_for(
            "Tag",
            format!("No tag exists for id: {}", id),
        ));
    }

    tx.commit().await?;

    Ok(Redirect::to("/tags"))
}

pub async fn search(
    State(state): State<AppState>,
    Query(query): Query<TagSearchQuery>,
) -> Result<Json<Vec<TagBadge>>, AppError> {
    let q = query.q.unwrap_or_default().trim().to_string();

    let rows: Vec<TagRow> = if q.is_empty() {
        sqlx::query_as!(
            TagRow,
            r#"
            SELECT
                id as "id: uuid::Uuid",
                name
            FROM tags
            ORDER BY name COLLATE NOCASE ASC
            LIMIT 10
            "#
        )
        .fetch_all(&state.db)
        .await?
    } else {
        let pattern = format!("%{}%", q);
        sqlx::query_as!(
            TagRow,
            r#"
            SELECT
                id as "id: uuid::Uuid",
                name
            FROM tags
            WHERE LOWER(name) LIKE LOWER($1)
            ORDER BY name COLLATE NOCASE ASC
            LIMIT 10
            "#,
            pattern
        )
        .fetch_all(&state.db)
        .await?
    };

    Ok(Json(
        rows.into_iter()
            .map(|row| TagBadge {
                id: row.id,
                name: row.name.clone(),
                color_style: tag_color_style(&row.name),
            })
            .collect(),
    ))
}

pub async fn fetch_all_badges(db: &SqlitePool) -> Result<Vec<TagBadge>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT
            id as "id: uuid::Uuid",
            name
        FROM tags
        ORDER BY name COLLATE NOCASE ASC
        "#
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TagBadge {
            id: row.id,
            name: row.name.clone(),
            color_style: tag_color_style(&row.name),
        })
        .collect())
}

pub async fn fetch_badges_for_plan(
    db: &SqlitePool,
    plan_id: Uuid,
) -> Result<Vec<TagBadge>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT
            tags.id as "id: uuid::Uuid",
            tags.name
        FROM action_plan_tags
        INNER JOIN tags ON tags.id = action_plan_tags.tag
        WHERE action_plan_tags.action_plan = $1
        ORDER BY tags.name COLLATE NOCASE ASC
        "#,
        plan_id
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TagBadge {
            id: row.id,
            name: row.name.clone(),
            color_style: tag_color_style(&row.name),
        })
        .collect())
}

pub async fn fetch_selected_tag_ids(
    db: &SqlitePool,
    plan_id: Uuid,
) -> Result<HashSet<Uuid>, AppError> {
    let rows = sqlx::query!(
        r#"
        SELECT tag as "tag: uuid::Uuid"
        FROM action_plan_tags
        WHERE action_plan = $1
        "#,
        plan_id
    )
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|row| row.tag).collect())
}

pub async fn fetch_badge_by_id(
    db: &SqlitePool,
    tag_id: Uuid,
) -> Result<Option<TagBadge>, AppError> {
    let row = sqlx::query!(
        r#"
        SELECT
            id as "id: uuid::Uuid",
            name
        FROM tags
        WHERE id = $1
        LIMIT 1
        "#,
        tag_id
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| TagBadge {
        id: row.id,
        name: row.name.clone(),
        color_style: tag_color_style(&row.name),
    }))
}

pub fn tag_color_style(name: &str) -> String {
    let hash = fnv1a_hash(name.trim().to_lowercase().as_bytes());
    let hue = (hash % 360) as f32;
    let saturation = 52.0 + ((hash >> 9) % 18) as f32;
    let value = 82.0 + ((hash >> 17) % 12) as f32;
    let (r, g, b) = hsv_to_rgb(hue, saturation / 100.0, value / 100.0);
    let text = if perceived_luminance(r, g, b) > 0.62 {
        "#172033"
    } else {
        "#ffffff"
    };

    format!(
        "background-color: rgb({r}, {g}, {b}); color: {text}; border-color: rgba({r}, {g}, {b}, 0.65);"
    )
}

fn normalize_tag_name(name: &str) -> Result<String, AppError> {
    let normalized = name.trim();
    if normalized.is_empty() {
        return Err(AppError::conflict("Tag name cannot be empty."));
    }

    Ok(normalized.to_string())
}

async fn ensure_name_available(
    db: &SqlitePool,
    name: &str,
    existing_id: Option<Uuid>,
) -> Result<(), AppError> {
    let existing = sqlx::query!(
        r#"
        SELECT id as "id: uuid::Uuid"
        FROM tags
        WHERE LOWER(name) = LOWER($1)
        "#,
        name
    )
    .fetch_optional(db)
    .await?;

    if let Some(tag) = existing {
        if Some(tag.id) != existing_id {
            return Err(AppError::conflict(format!(
                "A tag named \"{}\" already exists.",
                name
            )));
        }
    }

    Ok(())
}

fn fnv1a_hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (u8, u8, u8) {
    let chroma = value * saturation;
    let segment = (hue / 60.0) % 6.0;
    let x = chroma * (1.0 - ((segment % 2.0) - 1.0).abs());
    let (r1, g1, b1) = match hue {
        h if (0.0..60.0).contains(&h) => (chroma, x, 0.0),
        h if (60.0..120.0).contains(&h) => (x, chroma, 0.0),
        h if (120.0..180.0).contains(&h) => (0.0, chroma, x),
        h if (180.0..240.0).contains(&h) => (0.0, x, chroma),
        h if (240.0..300.0).contains(&h) => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = value - chroma;

    (
        ((r1 + m) * 255.0).round() as u8,
        ((g1 + m) * 255.0).round() as u8,
        ((b1 + m) * 255.0).round() as u8,
    )
}

fn perceived_luminance(r: u8, g: u8, b: u8) -> f32 {
    (0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)) / 255.0
}
