use std::sync::Arc;

use anyhow::Result;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::store::Store;

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<Store>,
}

pub async fn run(state: AppState, host: String, port: u16) -> Result<()> {
    let app = router(state);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    tracing::info!("Web inbox listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/messages", get(list_messages).delete(clear_messages))
        .route("/api/messages/{id}", get(get_message).delete(delete_message))
        .route("/api/messages/{id}/raw", get(get_raw))
        .route("/api/messages/{id}/attachments/{att_id}", get(get_attachment))
        .with_state(state)
}

const INDEX_HTML: &str = include_str!("ui/index.html");

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn health(State(state): State<AppState>) -> Json<serde_json::Value> {
    let count = state.store.count().unwrap_or(0);
    Json(serde_json::json!({ "status": "ok", "messages": count }))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    q: Option<String>,
    page: Option<u32>,
    limit: Option<u32>,
}

#[derive(Serialize)]
struct ListResponse {
    total: usize,
    messages: Vec<crate::store::MessageSummary>,
}

async fn list_messages(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, AppError> {
    let q = query.q.unwrap_or_default();
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = (page - 1) as usize * limit as usize;

    let all = state.store.list(&q, 10000, 0)?;
    let total = all.len();
    let page_items: Vec<_> = all
        .iter()
        .skip(offset)
        .take(limit as usize)
        .map(|m| crate::store::MessageSummary {
            snippet: snippet_for(&m, &q),
            ..m.clone()
        })
        .collect();

    Ok(Json(ListResponse {
        total,
        messages: page_items,
    }))
}

fn snippet_for(m: &crate::store::MessageSummary, q: &str) -> String {
    if m.subject.to_lowercase().contains(&q.to_lowercase()) && !q.is_empty() {
        format!("Subject: {}", m.subject)
    } else {
        String::new()
    }
}

async fn get_message(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::store::Message>, AppError> {
    state
        .store
        .get_message(&id)?
        .map(Json)
        .ok_or(AppError::not_found("message not found"))
}

async fn get_raw(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let msg = state
        .store
        .get_message(&id)?
        .ok_or(AppError::not_found("message not found"))?;
    let body = Body::from(msg.raw);
    let res = Response::builder()
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(body)
        .unwrap();
    Ok(res)
}

async fn get_attachment(
    State(state): State<AppState>,
    Path((id, att_id)): Path<(String, i64)>,
) -> Result<impl IntoResponse, AppError> {
    let att = state
        .store
        .get_attachment(&id, att_id)?
        .ok_or(AppError::not_found("attachment not found"))?;
    let body = Body::from(att.data);
    let res = Response::builder()
        .header(header::CONTENT_TYPE, att.content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", sanitize_filename(&att.filename)),
        )
        .body(body)
        .unwrap();
    Ok(res)
}

async fn delete_message(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    state.store.delete_message(&id)?;
    Ok(Json(serde_json::json!({ "deleted": id })))
}

async fn clear_messages(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    state.store.delete_all()?;
    Ok(Json(serde_json::json!({ "cleared": true })))
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c == '"' || c == '\\' || c == '\n' || c == '\r' { '_' } else { c })
        .collect();
    cleaned
}

struct AppError {
    status: StatusCode,
    message: String,
}

impl AppError {
    fn not_found(msg: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: msg.to_string(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("handler error: {e:#}");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: e.to_string(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.message })),
        )
            .into_response()
    }
}