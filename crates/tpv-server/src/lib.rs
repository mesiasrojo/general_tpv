//! Central synchronization server.
//!
//! The first slice keeps the event ledger in memory. The HTTP contract is
//! intentionally storage-agnostic so it can be backed by PostgreSQL without
//! changing clients.

#![forbid(unsafe_code)]

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    token: Option<String>,
    inner: Arc<RwLock<CentralLedger>>,
}

#[derive(Default)]
struct CentralLedger {
    events: Vec<StoredEvent>,
    next_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredEvent {
    pub seq: u64,
    pub event: SyncEvent,
    pub received_at: time::OffsetDateTime,
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
    pub events: usize,
    pub cursor: u64,
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
        Self {
            token,
            inner: Arc::new(RwLock::new(CentralLedger::default())),
        }
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

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    let ledger = state.inner.read().await;
    Json(HealthResponse {
        status: "ok",
        events: ledger.events.len(),
        cursor: ledger.next_seq,
    })
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
    let mut ledger = state.inner.write().await;
    let mut accepted = Vec::new();
    let mut duplicates = Vec::new();
    let mut rejected = Vec::new();
    for event in request.events {
        if !event.verify_hash() {
            rejected.push(RejectedEvent {
                event_id: event.event_id,
                reason: "invalid envelope hash".into(),
            });
            continue;
        }
        if ledger
            .events
            .iter()
            .any(|stored| stored.event.event_id == event.event_id)
        {
            duplicates.push(event.event_id);
            continue;
        }
        ledger.next_seq += 1;
        let seq = ledger.next_seq;
        accepted.push(event.event_id);
        ledger.events.push(StoredEvent {
            seq,
            event,
            received_at: time::OffsetDateTime::now_utc(),
        });
    }
    Ok(Json(PushResponse {
        accepted,
        duplicates,
        rejected,
        cursor: ledger.next_seq,
    }))
}

async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<PullQuery>,
) -> Result<Json<PullResponse>, ApiError> {
    state.authorize(&headers)?;
    let cursor = query.cursor.unwrap_or(0);
    let ledger = state.inner.read().await;
    let events = ledger
        .events
        .iter()
        .filter(|stored| stored.seq > cursor)
        .filter(|stored| {
            query
                .store_id
                .is_none_or(|store_id| stored.event.store_id.as_uuid() == store_id)
        })
        .cloned()
        .collect::<Vec<_>>();
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
    let ledger = state.inner.read().await;
    Ok(Json(ledger.events.clone()))
}

async fn list_sales(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<StoredEvent>>, ApiError> {
    state.authorize(&headers)?;
    let ledger = state.inner.read().await;
    Ok(Json(
        ledger
            .events
            .iter()
            .filter(|stored| stored.event.aggregate_type == "sale")
            .cloned()
            .collect(),
    ))
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
