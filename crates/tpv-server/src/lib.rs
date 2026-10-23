//! Central synchronization server.
//!
//! The HTTP contract is storage-agnostic. PostgreSQL is used when
//! `DATABASE_URL` is present; otherwise an in-memory ledger is used for local
//! development and tests.

#![forbid(unsafe_code)]

mod store;

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;
use tpv_application::PasswordHasher;
use tpv_auth::Argon2PasswordHasher;
use tpv_domain::ids::{DeviceId, StoreId, TenantId, UserId};
use tpv_domain::master_data::{MasterDataChange, MasterDataConflict};
use tpv_domain::people::{Role, UserAccount, UserProvision};
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

pub use store::{
    CentralSessionRecord, CentralStore, CentralUserRecord, DeviceRecord, DeviceStatus,
    EnrollDevice, EventPage, EventQuery, MemoryStore, NewCentralSession, NewCentralUser,
    PostgresStore, PushStoreResult, StoreError, StoreHealth, StoredEvent, UpdateCentralUser,
};

#[derive(Clone)]
pub struct AppState {
    token: Option<String>,
    store: Arc<dyn CentralStore>,
    require_auth: bool,
}

#[derive(Debug, Clone)]
struct AuthorizedPrincipal {
    username: String,
    display_name: String,
    role: Role,
    tenant_id: Option<Uuid>,
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
pub struct EventQueryParams {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub store_id: Option<Uuid>,
    pub device_id: Option<Uuid>,
    pub event_type: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EventPageResponse {
    pub items: Vec<StoredEvent>,
    pub total: u64,
    pub limit: u32,
    pub offset: u32,
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

#[derive(Debug, Deserialize)]
pub struct EnrollRequest {
    pub tenant_id: Uuid,
    pub store_id: Uuid,
    pub device_id: Uuid,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EnrollResponse {
    pub device: DeviceRecord,
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCentralUserRequest {
    pub tenant_id: Uuid,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<Uuid>,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCentralUserRequest {
    pub display_name: String,
    pub role: Role,
    pub store_ids: Vec<Uuid>,
    pub active: bool,
}

#[derive(Debug, Deserialize)]
pub struct ResetCentralUserPasswordRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub tenant_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct BootstrapRequest {
    pub tenant_id: Option<Uuid>,
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthStatusResponse {
    pub has_users: bool,
    pub suggested_tenant_id: Option<Uuid>,
}

#[derive(Debug, Serialize)]
pub struct AuthUserResponse {
    pub id: Option<Uuid>,
    pub tenant_id: Option<Uuid>,
    pub username: String,
    pub display_name: String,
    pub role: Role,
}

#[derive(Debug, Serialize)]
pub struct AuthSessionResponse {
    pub token: String,
    pub expires_at: time::OffsetDateTime,
    pub user: AuthUserResponse,
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

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
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
        Self::with_auth(token, store, false)
    }

    #[must_use]
    pub fn with_auth(
        token: Option<String>,
        store: Arc<dyn CentralStore>,
        require_auth: bool,
    ) -> Self {
        Self {
            token,
            store,
            require_auth,
        }
    }

    async fn authorize(&self, headers: &HeaderMap) -> Result<AuthorizedPrincipal, ApiError> {
        let provided = bearer_token(headers);
        if let Some(expected) = self.token.as_deref() {
            if provided == Some(expected) {
                return Ok(AuthorizedPrincipal {
                    username: "central-token".into(),
                    display_name: "Token central".into(),
                    role: Role::Admin,
                    tenant_id: None,
                });
            }
            if provided.is_none() {
                return Err(ApiError::unauthorized());
            }
        }
        if let Some(token) = provided {
            let session = self
                .store
                .central_session_by_token_hash(&hash_secret(token))
                .await
                .map_err(|error| ApiError::internal(error.to_string()))?;
            if let Some(session) = session {
                return Ok(AuthorizedPrincipal {
                    username: session.username,
                    display_name: session.display_name,
                    role: session.role,
                    tenant_id: Some(session.tenant_id),
                });
            }
        }
        if self.require_auth {
            return Err(ApiError::unauthorized());
        }
        Ok(AuthorizedPrincipal {
            username: "local-dev".into(),
            display_name: "Modo local".into(),
            role: Role::Admin,
            tenant_id: None,
        })
    }

    async fn authorize_admin(&self, headers: &HeaderMap) -> Result<AuthorizedPrincipal, ApiError> {
        let principal = self.authorize(headers).await?;
        if principal.role != Role::Admin {
            return Err(ApiError::forbidden(
                "esta operación requiere rol de administrador",
            ));
        }
        Ok(principal)
    }

    /// Accepts the global admin token (unrestricted) or a valid device token
    /// (scoped to that device).
    async fn authenticate_device(
        &self,
        headers: &HeaderMap,
    ) -> Result<Option<DeviceRecord>, ApiError> {
        let provided = bearer_token(headers).ok_or_else(ApiError::unauthorized)?;
        if self.token.as_deref() == Some(provided) {
            return Ok(None);
        }
        let token_hash = hash_device_token(provided);
        let device = self
            .store
            .device_by_token_hash(&token_hash)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
        match device {
            Some(device) if device.status == DeviceStatus::Active => Ok(Some(device)),
            Some(_) => Err(ApiError::forbidden("device revoked")),
            None => Err(ApiError::unauthorized()),
        }
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

fn generate_device_token() -> String {
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    format!("tpvdev_{}", hex::encode(bytes))
}

fn hash_device_token(token: &str) -> String {
    hash_secret(token)
}

fn hash_secret(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn generate_session_token() -> String {
    let mut bytes = [0_u8; 32];
    bytes[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    bytes[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    format!("tpvadm_{}", hex::encode(bytes))
}

async fn auth_status(State(state): State<AppState>) -> Result<Json<AuthStatusResponse>, ApiError> {
    let users = state
        .store
        .list_central_users()
        .await
        .map_err(store_error)?;
    let devices = state.store.list_devices().await.map_err(store_error)?;
    Ok(Json(AuthStatusResponse {
        has_users: !users.is_empty(),
        suggested_tenant_id: devices.first().map(|device| device.tenant_id),
    }))
}

async fn auth_bootstrap(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<BootstrapRequest>,
) -> Result<(StatusCode, Json<AuthSessionResponse>), ApiError> {
    if state.token.is_some() {
        state.authorize_admin(&headers).await?;
    }
    let existing = state
        .store
        .list_central_users()
        .await
        .map_err(store_error)?;
    if !existing.is_empty() {
        return Err(ApiError::conflict(
            "the central already has users; use the login endpoint",
        ));
    }
    let username = request.username.trim().to_lowercase();
    let display_name = request.display_name.trim().to_string();
    if username.is_empty() || display_name.is_empty() {
        return Err(ApiError::bad_request(
            "username and display_name are required",
        ));
    }
    validate_password(&request.password)?;
    let password_hash = hash_password(&request.password)?;
    let tenant_id = request.tenant_id.unwrap_or_else(Uuid::now_v7);
    let user = state
        .store
        .create_central_user(NewCentralUser {
            tenant_id,
            username,
            display_name,
            role: Role::Admin,
            store_ids: Vec::new(),
            password_hash,
        })
        .await
        .map_err(store_error)?;
    provision_user(&state, &user).await?;
    let response = issue_session(&state, &user).await?;
    Ok((StatusCode::CREATED, Json(response)))
}

async fn auth_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Json<AuthSessionResponse>, ApiError> {
    let username = request.username.trim().to_lowercase();
    if username.is_empty() || request.password.is_empty() {
        return Err(ApiError::bad_request("username and password are required"));
    }
    let mut candidates = state
        .store
        .find_central_users_by_username(&username)
        .await
        .map_err(store_error)?;
    if let Some(tenant_id) = request.tenant_id {
        candidates.retain(|user| user.tenant_id == tenant_id);
    }
    candidates.retain(|user| user.active);
    let user = match candidates.len() {
        0 => return Err(ApiError::unauthorized()),
        1 => candidates.remove(0),
        _ => {
            return Err(ApiError::bad_request(
                "username exists in several tenants; tenant_id is required",
            ));
        }
    };
    if user.role == Role::Cashier {
        return Err(ApiError::forbidden(
            "el rol de cajero no tiene acceso al backoffice",
        ));
    }
    let valid = Argon2PasswordHasher
        .verify(&request.password, &user.password_hash)
        .map_err(|error| ApiError::internal(error.to_string()))?;
    if !valid {
        return Err(ApiError::unauthorized());
    }
    Ok(Json(issue_session(&state, &user).await?))
}

async fn auth_me(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthUserResponse>, ApiError> {
    let principal = state.authorize(&headers).await?;
    Ok(Json(AuthUserResponse {
        id: None,
        tenant_id: principal.tenant_id,
        username: principal.username,
        display_name: principal.display_name,
        role: principal.role,
    }))
}

async fn auth_logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    if let Some(token) = bearer_token(&headers) {
        state
            .store
            .delete_central_session(&hash_secret(token))
            .await
            .map_err(store_error)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn issue_session(
    state: &AppState,
    user: &CentralUserRecord,
) -> Result<AuthSessionResponse, ApiError> {
    let token = generate_session_token();
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::hours(12);
    state
        .store
        .create_central_session(NewCentralSession {
            token_hash: hash_secret(&token),
            user_id: user.id,
            tenant_id: user.tenant_id,
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role,
            expires_at,
        })
        .await
        .map_err(store_error)?;
    Ok(AuthSessionResponse {
        token,
        expires_at,
        user: AuthUserResponse {
            id: Some(user.id),
            tenant_id: Some(user.tenant_id),
            username: user.username.clone(),
            display_name: user.display_name.clone(),
            role: user.role,
        },
    })
}

pub fn app(state: AppState) -> Router {
    let backoffice_dir =
        std::env::var("TPV_BACKOFFICE_DIR").unwrap_or_else(|_| "backoffice/build".into());
    let index = format!("{backoffice_dir}/index.html");
    Router::new()
        .route("/health", get(health))
        .route("/api/v1/auth/status", get(auth_status))
        .route("/api/v1/auth/bootstrap", post(auth_bootstrap))
        .route("/api/v1/auth/login", post(auth_login))
        .route("/api/v1/auth/logout", post(auth_logout))
        .route("/api/v1/auth/me", get(auth_me))
        .route("/api/v1/sync/push", post(push))
        .route("/api/v1/sync/pull", get(pull))
        .route("/api/v1/events", get(list_events))
        .route("/api/v1/sales", get(list_sales))
        .route(
            "/api/v1/master-data/conflicts",
            get(list_master_data_conflicts),
        )
        .route("/api/v1/devices", get(list_devices))
        .route("/api/v1/devices/enroll", post(enroll_device))
        .route("/api/v1/devices/{device_id}/revoke", post(revoke_device))
        .route(
            "/api/v1/users",
            get(list_central_users).post(create_central_user),
        )
        .route("/api/v1/users/{user_id}", put(update_central_user))
        .route(
            "/api/v1/users/{user_id}/password",
            post(reset_central_user_password),
        )
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
    let device = state.authenticate_device(&headers).await?;
    if let Some(device) = &device {
        if request.device_id != device.device_id {
            return Err(ApiError::forbidden(
                "device token does not match the pushed device",
            ));
        }
        if request.events.iter().any(|event| {
            event.store_id.as_uuid() != device.store_id
                || event.tenant_id.as_uuid() != device.tenant_id
        }) {
            return Err(ApiError::forbidden(
                "event belongs to another tenant or store",
            ));
        }
    }
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
        if !event.verify_hash() {
            rejected.push(RejectedEvent {
                event_id: event.event_id,
                reason: "invalid envelope hash".into(),
            });
            continue;
        }
        if event.aggregate_type != "master_data" {
            valid.push(event);
            continue;
        }
        let change: MasterDataChange = match serde_json::from_value(event.payload.clone()) {
            Ok(change) => change,
            Err(error) => {
                rejected.push(RejectedEvent {
                    event_id: event.event_id,
                    reason: format!("invalid master data payload: {error}"),
                });
                continue;
            }
        };
        let expected_deleted = event.event_type == "master_data.deleted";
        let valid_event_type =
            event.event_type == "master_data.upsert" || event.event_type == "master_data.deleted";
        if !valid_event_type
            || change.device_id.as_uuid() != event.device_id.as_uuid()
            || change.entity_id != event.aggregate_id
            || change.deleted != expected_deleted
        {
            rejected.push(RejectedEvent {
                event_id: event.event_id,
                reason: "master data envelope does not match its payload".into(),
            });
            continue;
        }
        let outcome = state
            .store
            .apply_master_data_change(event.tenant_id.as_uuid(), event.store_id.as_uuid(), &change)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
        if outcome.applied || outcome.conflict.is_none() {
            valid.push(event);
        } else if let Some(conflict) = outcome.conflict {
            rejected.push(RejectedEvent {
                event_id: event.event_id,
                reason: format!(
                    "master data conflict ({}), local version {} is authoritative",
                    conflict.reason.as_str(),
                    conflict.local_version
                ),
            });
        }
    }
    let result: PushStoreResult = state
        .store
        .push(valid)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    if let Some(device) = device {
        state
            .store
            .mark_device_seen(device.device_id)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
    }
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
    let device = state.authenticate_device(&headers).await?;
    let store_id = match &device {
        Some(device) => {
            if query
                .store_id
                .is_some_and(|store_id| store_id != device.store_id)
            {
                return Err(ApiError::forbidden(
                    "device token cannot pull another store",
                ));
            }
            Some(device.store_id)
        }
        None => query.store_id,
    };
    let cursor = query.cursor.unwrap_or(0);
    let events = state
        .store
        .pull(cursor, store_id, 1000)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    let next_cursor = events.last().map_or(cursor, |stored| stored.seq);
    if let Some(device) = device {
        state
            .store
            .mark_device_seen(device.device_id)
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
    }
    Ok(Json(PullResponse {
        events,
        next_cursor,
    }))
}

async fn enroll_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<EnrollRequest>,
) -> Result<Json<EnrollResponse>, ApiError> {
    state.authorize_admin(&headers).await?;
    let name = request.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("device name is required"));
    }
    let token = generate_device_token();
    let device = state
        .store
        .enroll_device(EnrollDevice {
            device_id: request.device_id,
            tenant_id: request.tenant_id,
            store_id: request.store_id,
            name: name.to_string(),
            token_hash: hash_device_token(&token),
        })
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(EnrollResponse { device, token }))
}

async fn list_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<DeviceRecord>>, ApiError> {
    state.authorize(&headers).await?;
    let devices = state
        .store
        .list_devices()
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(devices))
}

async fn revoke_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(device_id): Path<Uuid>,
) -> Result<Json<DeviceRecord>, ApiError> {
    state.authorize_admin(&headers).await?;
    let device = state
        .store
        .revoke_device(device_id)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?
        .ok_or_else(|| ApiError::not_found("device not found"))?;
    Ok(Json(device))
}

async fn list_master_data_conflicts(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<MasterDataConflict>>, ApiError> {
    state.authorize(&headers).await?;
    let conflicts = state
        .store
        .list_master_data_conflicts(200)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(conflicts))
}

async fn list_central_users(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<CentralUserRecord>>, ApiError> {
    state.authorize(&headers).await?;
    let users = state
        .store
        .list_central_users()
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(users))
}

async fn create_central_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateCentralUserRequest>,
) -> Result<(StatusCode, Json<CentralUserRecord>), ApiError> {
    state.authorize_admin(&headers).await?;
    if request.tenant_id.is_nil() {
        return Err(ApiError::bad_request("tenant_id is required"));
    }
    let (username, display_name) =
        normalize_user_fields(&request.username, &request.display_name, &request.store_ids)?;
    validate_password(&request.password)?;
    let password_hash = hash_password(&request.password)?;
    let user = state
        .store
        .create_central_user(NewCentralUser {
            tenant_id: request.tenant_id,
            username,
            display_name,
            role: request.role,
            store_ids: request.store_ids,
            password_hash,
        })
        .await
        .map_err(store_error)?;
    provision_user(&state, &user).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

async fn update_central_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(request): Json<UpdateCentralUserRequest>,
) -> Result<Json<CentralUserRecord>, ApiError> {
    state.authorize_admin(&headers).await?;
    let (_, display_name) =
        normalize_user_fields("unchanged", &request.display_name, &request.store_ids)?;
    let user = state
        .store
        .update_central_user(
            user_id,
            UpdateCentralUser {
                display_name,
                role: request.role,
                store_ids: request.store_ids,
                active: request.active,
                password_hash: None,
            },
        )
        .await
        .map_err(store_error)?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    state
        .store
        .delete_central_user_sessions(user.id)
        .await
        .map_err(store_error)?;
    provision_user(&state, &user).await?;
    Ok(Json(user))
}

async fn reset_central_user_password(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    Json(request): Json<ResetCentralUserPasswordRequest>,
) -> Result<Json<CentralUserRecord>, ApiError> {
    state.authorize_admin(&headers).await?;
    validate_password(&request.password)?;
    let password_hash = hash_password(&request.password)?;
    let user = state
        .store
        .set_central_user_password(user_id, &password_hash)
        .await
        .map_err(store_error)?
        .ok_or_else(|| ApiError::not_found("user not found"))?;
    state
        .store
        .delete_central_user_sessions(user.id)
        .await
        .map_err(store_error)?;
    provision_user(&state, &user).await?;
    Ok(Json(user))
}

fn normalize_user_fields(
    username: &str,
    display_name: &str,
    store_ids: &[Uuid],
) -> Result<(String, String), ApiError> {
    let username = username.trim().to_lowercase();
    let display_name = display_name.trim().to_string();
    if username.is_empty() || display_name.is_empty() {
        return Err(ApiError::bad_request(
            "username and display_name are required",
        ));
    }
    if store_ids.is_empty() || store_ids.iter().any(Uuid::is_nil) {
        return Err(ApiError::bad_request(
            "at least one valid store_id is required",
        ));
    }
    Ok((username, display_name))
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.chars().count() < 8 {
        return Err(ApiError::bad_request(
            "password must have at least 8 characters",
        ));
    }
    Ok(())
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    Argon2PasswordHasher
        .hash(password)
        .map_err(|error| ApiError::internal(error.to_string()))
}

fn store_error(error: StoreError) -> ApiError {
    match error {
        StoreError::Conflict(message) => ApiError::conflict(message),
        other => ApiError::internal(other.to_string()),
    }
}

async fn provision_user(state: &AppState, user: &CentralUserRecord) -> Result<(), ApiError> {
    let account = UserAccount {
        id: UserId::from_uuid(user.id),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        role: user.role,
        store_ids: user
            .store_ids
            .iter()
            .map(|store_id| StoreId::from_uuid(*store_id))
            .collect(),
        active: user.active,
    };
    let provision = UserProvision {
        user: account,
        password_hash: user.password_hash.clone(),
        revision: user.version,
    };
    let payload =
        serde_json::to_value(&provision).map_err(|error| ApiError::internal(error.to_string()))?;
    let now = time::OffsetDateTime::now_utc();
    let sequence = u64::try_from(now.unix_timestamp_nanos()).unwrap_or_default();
    for store_id in &user.store_ids {
        let event = SyncEvent::new(
            TenantId::from_uuid(user.tenant_id),
            StoreId::from_uuid(*store_id),
            DeviceId::from_uuid(Uuid::nil()),
            sequence,
            "user",
            user.id,
            "user.provisioned",
            1,
            now,
            payload.clone(),
        )
        .map_err(|error| ApiError::internal(error.to_string()))?;
        state
            .store
            .push(vec![event])
            .await
            .map_err(|error| ApiError::internal(error.to_string()))?;
    }
    Ok(())
}

async fn list_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<EventPageResponse>, ApiError> {
    state.authorize(&headers).await?;
    let query = event_query(params, false)?;
    let page = state
        .store
        .list_events(&query)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(event_page_response(page, &query)))
}

async fn list_sales(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<EventQueryParams>,
) -> Result<Json<EventPageResponse>, ApiError> {
    state.authorize(&headers).await?;
    let query = event_query(params, true)?;
    let page = state
        .store
        .list_sales(&query)
        .await
        .map_err(|error| ApiError::internal(error.to_string()))?;
    Ok(Json(event_page_response(page, &query)))
}

fn event_query(params: EventQueryParams, sales_only: bool) -> Result<EventQuery, ApiError> {
    let parse_time = |value: Option<&str>,
                      field: &str|
     -> Result<Option<time::OffsetDateTime>, ApiError> {
        value
            .map(|value| {
                time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
                    .map_err(|_| ApiError::bad_request(format!("invalid {field}; use RFC3339")))
            })
            .transpose()
    };
    Ok(EventQuery {
        limit: params.limit.unwrap_or(50).clamp(1, 200),
        offset: params.offset.unwrap_or(0).min(1_000_000),
        store_id: params.store_id,
        device_id: params.device_id,
        event_type: params.event_type.filter(|value| !value.trim().is_empty()),
        from: parse_time(params.from.as_deref(), "from")?,
        to: parse_time(params.to.as_deref(), "to")?,
        sales_only,
    })
}

fn event_page_response(page: EventPage, query: &EventQuery) -> EventPageResponse {
    EventPageResponse {
        items: page.items,
        total: page.total,
        limit: query.limit,
        offset: query.offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;
    use tpv_domain::master_data::{ConflictReason, MasterDataKind};

    fn event(device_id: tpv_domain::ids::DeviceId, tenant_id: Uuid, store_id: Uuid) -> SyncEvent {
        SyncEvent::new(
            tpv_domain::ids::TenantId::from_uuid(tenant_id),
            tpv_domain::ids::StoreId::from_uuid(store_id),
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

    fn master_event(
        device_id: Uuid,
        tenant_id: Uuid,
        store_id: Uuid,
        change: &MasterDataChange,
    ) -> SyncEvent {
        let sequence = u64::try_from(change.occurred_at.unix_timestamp_nanos()).unwrap_or_default();
        let event_type = if change.deleted {
            "master_data.deleted"
        } else {
            "master_data.upsert"
        };
        SyncEvent::new(
            tpv_domain::ids::TenantId::from_uuid(tenant_id),
            tpv_domain::ids::StoreId::from_uuid(store_id),
            tpv_domain::ids::DeviceId::from_uuid(device_id),
            sequence,
            "master_data",
            change.entity_id,
            event_type,
            1,
            change.occurred_at,
            serde_json::to_value(change).unwrap(),
        )
        .unwrap()
    }

    async fn push_master(
        app: &Router,
        device_id: Uuid,
        token: &str,
        event: SyncEvent,
    ) -> (StatusCode, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(token),
                Some(json!({"device_id": device_id, "events": [event]})),
            ))
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn request(
        method: &str,
        uri: &str,
        token: Option<&str>,
        body: Option<serde_json::Value>,
    ) -> Request<Body> {
        let mut builder = Request::builder().method(method).uri(uri);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        match body {
            Some(body) => builder
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
            None => builder.body(Body::empty()).unwrap(),
        }
    }

    async fn enroll(
        app: &Router,
        device_id: Uuid,
        store_id: Uuid,
        admin_token: Option<&str>,
    ) -> EnrollResponse {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/devices/enroll",
                admin_token,
                Some(json!({
                    "tenant_id": Uuid::now_v7(),
                    "store_id": store_id,
                    "device_id": device_id,
                    "name": "Caja test"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn push_deduplicates_events() {
        let app = app(AppState::new(None));
        let device_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let enrollment = enroll(&app, device_id, store_id, None).await;
        let event = event(
            tpv_domain::ids::DeviceId::from_uuid(device_id),
            enrollment.device.tenant_id,
            store_id,
        );
        let payload = json!({"device_id": device_id, "events": [event]});
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(&enrollment.token),
                Some(payload.clone()),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(&enrollment.token),
                Some(payload),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .oneshot(request("GET", "/api/v1/devices", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let devices: Vec<DeviceRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(devices.len(), 1);
        assert!(devices[0].last_seen_at.is_some());
    }

    #[tokio::test]
    async fn revoked_device_cannot_push() {
        let app = app(AppState::new(None));
        let device_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let enrollment = enroll(&app, device_id, store_id, None).await;

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/devices/{device_id}/revoke"),
                None,
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let event = event(
            tpv_domain::ids::DeviceId::from_uuid(device_id),
            enrollment.device.tenant_id,
            store_id,
        );
        let response = app
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(&enrollment.token),
                Some(json!({"device_id": device_id, "events": [event]})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn device_cannot_push_events_from_another_store() {
        let app = app(AppState::new(None));
        let device_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let enrollment = enroll(&app, device_id, store_id, None).await;
        let event = event(
            tpv_domain::ids::DeviceId::from_uuid(device_id),
            enrollment.device.tenant_id,
            Uuid::now_v7(),
        );
        let response = app
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(&enrollment.token),
                Some(json!({"device_id": device_id, "events": [event]})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn device_management_requires_admin_token_when_configured() {
        let app = app(AppState::new(Some("secret".into())));
        let device_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/devices/enroll",
                None,
                Some(json!({
                    "tenant_id": Uuid::now_v7(),
                    "store_id": store_id,
                    "device_id": device_id,
                    "name": "Caja test"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let enrollment = enroll(&app, device_id, store_id, Some("secret")).await;
        assert_eq!(enrollment.device.device_id, device_id);
    }

    #[tokio::test]
    async fn master_data_conflicts_are_detected_and_recorded() {
        let app = app(AppState::new(None));
        let device_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let enrollment = enroll(&app, device_id, store_id, None).await;
        let tenant_id = enrollment.device.tenant_id;
        let entity_id = Uuid::now_v7();
        let base = time::OffsetDateTime::now_utc();
        let device = tpv_domain::ids::DeviceId::from_uuid(device_id);

        let first = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            1,
            device,
            base,
            json!({"name": "Producto A"}),
            false,
        )
        .unwrap();
        let (status, body) = push_master(
            &app,
            device_id,
            &enrollment.token,
            master_event(device_id, tenant_id, store_id, &first),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert!(body["rejected"].as_array().unwrap().is_empty());

        let divergent = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            1,
            device,
            base + time::Duration::seconds(1),
            json!({"name": "Producto B"}),
            false,
        )
        .unwrap();
        let (status, _) = push_master(
            &app,
            device_id,
            &enrollment.token,
            master_event(device_id, tenant_id, store_id, &divergent),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let newer = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            2,
            device,
            base + time::Duration::seconds(2),
            json!({"name": "Producto C"}),
            false,
        )
        .unwrap();
        let (status, _) = push_master(
            &app,
            device_id,
            &enrollment.token,
            master_event(device_id, tenant_id, store_id, &newer),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let stale = MasterDataChange::new(
            MasterDataKind::Product,
            entity_id,
            1,
            device,
            base + time::Duration::seconds(3),
            json!({"name": "Producto D"}),
            false,
        )
        .unwrap();
        let (status, body) = push_master(
            &app,
            device_id,
            &enrollment.token,
            master_event(device_id, tenant_id, store_id, &stale),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let rejected = body["rejected"].as_array().unwrap();
        assert!(rejected.is_empty());

        let response = app
            .oneshot(request("GET", "/api/v1/master-data/conflicts", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let conflicts: Vec<MasterDataConflict> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert!(
            conflicts
                .iter()
                .any(|conflict| conflict.reason == ConflictReason::SameVersionDiverged)
        );
    }

    #[tokio::test]
    async fn central_user_crud_provisions_sync_events() {
        let app = app(AppState::new(Some("secret".into())));
        let tenant_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/users",
                Some("secret"),
                Some(json!({
                    "tenant_id": tenant_id,
                    "username": "Cajero.Central",
                    "display_name": "Cajero Central",
                    "role": "cashier",
                    "store_ids": [store_id],
                    "password": "central123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let user: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(user["username"], "cajero.central");
        assert_eq!(user["version"], 1);
        assert!(user.get("password_hash").is_none());
        let user_id = Uuid::parse_str(user["id"].as_str().unwrap()).unwrap();

        let response = app
            .clone()
            .oneshot(request("GET", "/api/v1/users", Some("secret"), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let users: Vec<CentralUserRecord> = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(users.len(), 1);
        assert!(users[0].password_hash.is_empty());

        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/v1/sync/pull?cursor=0&store_id={store_id}"),
                Some("secret"),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let pull: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let provisioned = pull["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stored| stored["event"]["event_type"] == "user.provisioned")
            .expect("provision event");
        let provision: UserProvision =
            serde_json::from_value(provisioned["event"]["payload"].clone()).unwrap();
        assert_eq!(provision.user.id.as_uuid(), user_id);
        assert_eq!(provision.user.username, "cajero.central");
        assert_eq!(provision.revision, 1);
        assert!(provision.password_hash.starts_with("$argon2"));

        let response = app
            .clone()
            .oneshot(request(
                "PUT",
                &format!("/api/v1/users/{user_id}"),
                Some("secret"),
                Some(json!({
                    "display_name": "Cajero Actualizado",
                    "role": "manager",
                    "store_ids": [store_id],
                    "active": true
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let updated: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(updated["version"], 2);
        assert_eq!(updated["display_name"], "Cajero Actualizado");

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/v1/users/{user_id}/password"),
                Some("secret"),
                Some(json!({"password": "nuevaClave123"})),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let reset: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reset["version"], 3);

        let duplicate = app
            .oneshot(request(
                "POST",
                "/api/v1/users",
                Some("secret"),
                Some(json!({
                    "tenant_id": tenant_id,
                    "username": "cajero.central",
                    "display_name": "Duplicado",
                    "role": "cashier",
                    "store_ids": [store_id],
                    "password": "central123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(duplicate.status(), StatusCode::CONFLICT);
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

    fn typed_event(
        device_id: Uuid,
        tenant_id: Uuid,
        store_id: Uuid,
        device_seq: u64,
        aggregate_type: &str,
        event_type: &str,
        occurred_at: time::OffsetDateTime,
    ) -> SyncEvent {
        SyncEvent::new(
            tpv_domain::ids::TenantId::from_uuid(tenant_id),
            tpv_domain::ids::StoreId::from_uuid(store_id),
            tpv_domain::ids::DeviceId::from_uuid(device_id),
            device_seq,
            aggregate_type,
            Uuid::now_v7(),
            event_type,
            1,
            occurred_at,
            json!({ "total_cents": 100 * i64::try_from(device_seq).unwrap_or_default() }),
        )
        .unwrap()
    }

    async fn push_events(app: &Router, device_id: Uuid, token: &str, events: &[SyncEvent]) {
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/sync/push",
                Some(token),
                Some(json!({ "device_id": device_id, "events": events })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    async fn get_json(app: &Router, uri: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(request("GET", uri, None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn events_support_pagination_and_filters() {
        let app = app(AppState::new(None));
        let device_a = Uuid::now_v7();
        let device_b = Uuid::now_v7();
        let store_a = Uuid::now_v7();
        let store_b = Uuid::now_v7();
        let enrollment_a = enroll(&app, device_a, store_a, None).await;
        let enrollment_b = enroll(&app, device_b, store_b, None).await;
        let tenant_a = enrollment_a.device.tenant_id;
        let tenant_b = enrollment_b.device.tenant_id;
        let base = time::OffsetDateTime::now_utc() - time::Duration::hours(1);

        push_events(
            &app,
            device_a,
            &enrollment_a.token,
            &[
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    1,
                    "sale",
                    "sale.confirmed",
                    base,
                ),
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    2,
                    "sale",
                    "sale.confirmed",
                    base + time::Duration::seconds(10),
                ),
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    3,
                    "invoice",
                    "invoice.issued",
                    base + time::Duration::seconds(20),
                ),
            ],
        )
        .await;
        push_events(
            &app,
            device_b,
            &enrollment_b.token,
            &[typed_event(
                device_b,
                tenant_b,
                store_b,
                1,
                "sale",
                "sale.confirmed",
                base + time::Duration::seconds(30),
            )],
        )
        .await;

        let page = get_json(&app, "/api/v1/events?limit=2").await;
        assert_eq!(page["total"], 4);
        assert_eq!(page["limit"], 2);
        assert_eq!(page["offset"], 0);
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["event"]["store_id"], store_b.to_string());
        assert_eq!(items[1]["event"]["event_type"], "invoice.issued");

        let second = get_json(&app, "/api/v1/events?limit=2&offset=2").await;
        assert_eq!(second["total"], 4);
        assert_eq!(second["items"].as_array().unwrap().len(), 2);

        let beyond = get_json(&app, "/api/v1/events?offset=100").await;
        assert_eq!(beyond["total"], 4);
        assert!(beyond["items"].as_array().unwrap().is_empty());

        let by_type = get_json(&app, "/api/v1/events?event_type=invoice.issued").await;
        assert_eq!(by_type["total"], 1);
        assert_eq!(by_type["items"][0]["event"]["event_type"], "invoice.issued");

        let by_store = get_json(&app, &format!("/api/v1/events?store_id={store_a}")).await;
        assert_eq!(by_store["total"], 3);

        let by_device = get_json(&app, &format!("/api/v1/events?device_id={device_b}")).await;
        assert_eq!(by_device["total"], 1);

        let from = (base + time::Duration::seconds(15))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        let by_date = get_json(&app, &format!("/api/v1/events?from={from}")).await;
        assert_eq!(by_date["total"], 2);

        let clamped = get_json(&app, "/api/v1/events?limit=9999").await;
        assert_eq!(clamped["limit"], 200);
    }

    #[tokio::test]
    async fn sales_endpoint_only_returns_sales_and_paginates() {
        let app = app(AppState::new(None));
        let device_a = Uuid::now_v7();
        let device_b = Uuid::now_v7();
        let store_a = Uuid::now_v7();
        let store_b = Uuid::now_v7();
        let enrollment_a = enroll(&app, device_a, store_a, None).await;
        let enrollment_b = enroll(&app, device_b, store_b, None).await;
        let tenant_a = enrollment_a.device.tenant_id;
        let tenant_b = enrollment_b.device.tenant_id;
        let base = time::OffsetDateTime::now_utc() - time::Duration::hours(1);

        push_events(
            &app,
            device_a,
            &enrollment_a.token,
            &[
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    1,
                    "sale",
                    "sale.confirmed",
                    base,
                ),
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    2,
                    "invoice",
                    "invoice.issued",
                    base + time::Duration::seconds(10),
                ),
                typed_event(
                    device_a,
                    tenant_a,
                    store_a,
                    3,
                    "sale",
                    "sale.confirmed",
                    base + time::Duration::seconds(20),
                ),
            ],
        )
        .await;
        push_events(
            &app,
            device_b,
            &enrollment_b.token,
            &[typed_event(
                device_b,
                tenant_b,
                store_b,
                1,
                "sale",
                "sale.confirmed",
                base + time::Duration::seconds(30),
            )],
        )
        .await;

        let first = get_json(&app, "/api/v1/sales?limit=2").await;
        assert_eq!(first["total"], 3);
        let items = first["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .all(|item| item["event"]["aggregate_type"] == "sale")
        );

        let second = get_json(&app, "/api/v1/sales?limit=2&offset=2").await;
        assert_eq!(second["total"], 3);
        assert_eq!(second["items"].as_array().unwrap().len(), 1);

        let by_store = get_json(&app, &format!("/api/v1/sales?store_id={store_b}")).await;
        assert_eq!(by_store["total"], 1);
        assert_eq!(
            by_store["items"][0]["event"]["store_id"],
            store_b.to_string()
        );
    }

    #[tokio::test]
    async fn bootstrap_login_session_and_rbac() {
        let state = AppState::with_auth(None, Arc::new(MemoryStore::new()), true);
        let app = app(state);
        let tenant_id = Uuid::now_v7();
        let store_id = Uuid::now_v7();

        let response = app
            .clone()
            .oneshot(request("GET", "/api/v1/users", None, None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/auth/bootstrap",
                None,
                Some(json!({
                    "tenant_id": tenant_id,
                    "username": "owner",
                    "display_name": "Owner",
                    "password": "owner123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let bootstrap: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let admin_token = bootstrap["token"].as_str().unwrap().to_string();
        assert_eq!(bootstrap["user"]["role"], "admin");

        let response = app
            .clone()
            .oneshot(request("GET", "/api/v1/auth/me", Some(&admin_token), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/users",
                Some(&admin_token),
                Some(json!({
                    "tenant_id": tenant_id,
                    "username": "manager",
                    "display_name": "Manager",
                    "role": "manager",
                    "store_ids": [store_id],
                    "password": "manager123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/auth/login",
                None,
                Some(json!({
                    "username": "manager",
                    "password": "manager123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let login: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let manager_token = login["token"].as_str().unwrap().to_string();
        assert_eq!(login["user"]["role"], "manager");

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/users",
                Some(&manager_token),
                Some(json!({
                    "tenant_id": tenant_id,
                    "username": "cashier",
                    "display_name": "Cashier",
                    "role": "cashier",
                    "store_ids": [store_id],
                    "password": "cashier123"
                })),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/v1/auth/logout",
                Some(&admin_token),
                None,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        let response = app
            .oneshot(request("GET", "/api/v1/users", Some(&admin_token), None))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
