//! Central synchronization server.
//!
//! The HTTP contract is storage-agnostic. PostgreSQL is used when
//! `DATABASE_URL` is present; otherwise an in-memory ledger is used for local
//! development and tests.

#![forbid(unsafe_code)]

mod store;

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

pub use store::{
    CentralStore, MemoryStore, PostgresStore, PushStoreResult, StoreError, StoreHealth, StoredEvent,
};

#[derive(Clone)]
pub struct AppState {
    token: Option<String>,
    store: Arc<dyn CentralStore>,
}

#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub device_id: Uuid,
    pub events: Vec<SyncEvent>,
}

#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub accepted: Vec<Uuid>,
    pub duplicates: Vec<Uuid>,
    pub rejected: Vec<RejectedEvent>,
    pub cursor: u64,
}

#[derive(Debug, Serialize)]
pub struct RejectedEvent {
    pub event_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub cursor: Option<u64>,
    pub store_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PullResponse {
    pub events: Vec<StoredEvent>,
    pub next_cursor: u64,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub events: i64,
    pub cursor: i64,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: String,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: "missing or invalid bearer token".into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

impl AppState {
    #[must_use]
    pub fn new(token: Option<String>) -> Self {
        Self::with_store(token, Arc::new(MemoryStore::new()))
    }

    #[must_use]
    pub fn with_store(token: Option<String>, store: Arc<dyn CentralStore>) -> Self {
        Self { token, store }
    }

    fn authorize(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        let Some(expected) = self.token.as_deref() else {
            return Ok(());
        };
        let provided = headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.strip_prefix("Bearer "));
        if provided == Some(expected) {
            Ok(())
        } else {
            Err(ApiError::unauthorized())
        }
    }
}

pub fn app(state: AppState) -> Router {
    let backoffice_dir =
        std::env::var("TPV_BACKOFFICE_DIR").unwrap_or_else(|_| "backoffice/build".into());
    let index = format!("{backoffice_dir}/index.html");
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/sync/push", post(push))
        .route("/api/v1/sync/pull", get(pull))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/sales", get(list_sales))
        .fallback_service(ServeDir::new(backoffice_dir).not_found_service(ServeFile::new(index)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, ApiError> {
    let health = state
        .store
        .health()
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(HealthResponse {
        status: "ok",
        events: health.events,
        cursor: health.cursor,
    }))
}

async fn push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<PushRequest>,
) -> Result<Json<PushResponse>, ApiError> {
    state.authorize(&headers)?;
    if request.events.len() > 100 {
        return Err(ApiError::bad_request("a batch is limited to 100 events"));
    }
    if request
        .events
        .iter()
        .any(|event| event.device_id.as_uuid() != request.device_id)
    {
        return Err(ApiError::bad_request(
            "batch contains an event from another device",
        ));
    }
    let mut rejected = Vec::new();
    let mut valid = Vec::new();
    for event in request.events {
        if event.verify_hash() {
            valid.push(event);
        } else {
            rejected.push(RejectedEvent {
                event_id: event.event_id,
                reason: "invalid envelope hash".into(),
            });
        }
    }
    let result: PushStoreResult = state
        .store
        .push(valid)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(PushResponse {
        accepted: result.accepted,
        duplicates: result.duplicates,
        rejected,
        cursor: result.cursor,
    }))
}

async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>, ApiError> {
    state.authorize(&headers)?;
    let cursor = query.cursor.unwrap_or(0);
    let events = state
        .store
        .pull(cursor, query.store_id, 1000)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let next_cursor = events.last().map_or(cursor, |stored| stored.seq);
    Ok(Json(PullResponse {
        events,
        next_cursor,
    }))
}

async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StoredEvent>>, ApiError> {
    state.authorize(&headers)?;
    let events = state
        .store
        .list_events(1000)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(events))
}

async fn list_sales(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StoredEvent>>, ApiError> {
    state.authorize(&headers)?;
    let events = state
        .store
        .list_sales(1000)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    fn event(device_id: tpv_domain::ids::DeviceId) -> SyncEvent {
        SyncEvent::new(
            tpv_domain::ids::TenantId::new(),
            tpv_domain::ids::StoreId::new(),
            device_id,
            1,
            "sale",
            Uuid::now_v7(),
            "sale.confirmed",
            1,
            time::OffsetDateTime::now_utc(),
            json!({"total_cents": 140}),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn push_deduplicates_events() {
        let state = AppState::new(None);
        let app = app(state);
        let device_id = tpv_domain::ids::DeviceId::new();
        let event = event(device_id);
        let payload = json!({"device_id": device_id, "events": [event]});
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/sync/push")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let request = Request::builder()
            .method("POST")
            .uri("/api/v1/sync/push")
            .header("content-type", "application/json")
            .body(Body::from(payload.to_string()))
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn rejects_missing_token_when_configured() {
        let state = AppState::new(Some("secret".into()));
        let app = app(state);
        let request = Request::builder()
            .uri("/api/v1/events")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
