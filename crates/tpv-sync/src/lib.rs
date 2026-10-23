//! HTTP client for the central synchronization endpoint.
//!
//! The client is deliberately small: it pushes an outbox batch, pulls changes
//! after a cursor, and leaves retry/backoff policy to the caller.

#![forbid(unsafe_code)]

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tpv_domain::ids::DeviceId;
use tpv_domain::sync::SyncEvent;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("central rejected the request ({status}): {body}")]
    Rejected { status: u16, body: String },

    #[error("invalid sync response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct PushRequest {
    pub device_id: Uuid,
    pub events: Vec<SyncEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RejectedEvent {
    pub event_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PushResponse {
    pub accepted: Vec<Uuid>,
    pub duplicates: Vec<Uuid>,
    pub rejected: Vec<RejectedEvent>,
    pub cursor: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoredEvent {
    pub seq: u64,
    pub event: SyncEvent,
    pub received_at: time::OffsetDateTime,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PullResponse {
    pub events: Vec<StoredEvent>,
    pub next_cursor: u64,
}

#[derive(Clone)]
pub struct SyncClient {
    base_url: String,
    token: Option<String>,
    client: Client,
}

impl SyncClient {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            token,
            client: Client::new(),
        }
    }

    pub async fn push(
        &self,
        device_id: DeviceId,
        events: Vec<SyncEvent>,
    ) -> Result<PushResponse, SyncError> {
        if events.len() > 100 {
            return Err(SyncError::InvalidResponse(
                "a batch is limited to 100 events".into(),
            ));
        }
        let mut request = self
            .client
            .post(format!("{}/api/v1/sync/push", self.base_url))
            .json(&PushRequest {
                device_id: device_id.as_uuid(),
                events,
            });
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        decode(response).await
    }

    pub async fn pull(
        &self,
        cursor: u64,
        store_id: Option<Uuid>,
    ) -> Result<PullResponse, SyncError> {
        let mut request = self
            .client
            .get(format!("{}/api/v1/sync/pull", self.base_url))
            .query(&[("cursor", cursor.to_string())]);
        if let Some(store_id) = store_id {
            request = request.query(&[("store_id", store_id.to_string())]);
        }
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        decode(response).await
    }
}

async fn decode<T: for<'de> Deserialize<'de>>(response: reqwest::Response) -> Result<T, SyncError> {
    let status = response.status();
    let body = response.text().await?;
    if status != StatusCode::OK {
        return Err(SyncError::Rejected {
            status: status.as_u16(),
            body,
        });
    }
    serde_json::from_str(&body)
        .map_err(|error| SyncError::InvalidResponse(format!("{error}: {body}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_trailing_slashes() {
        let client = SyncClient::new("http://localhost:8080///", None);
        assert_eq!(client.base_url, "http://localhost:8080");
    }
}
