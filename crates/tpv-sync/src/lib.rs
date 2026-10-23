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
    #[error(transparent)]
    Application(#[from] tpv_application::ApplicationError),

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

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub base_delay: std::time::Duration,
    pub max_delay: std::time::Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay: std::time::Duration::from_millis(500),
            max_delay: std::time::Duration::from_secs(30),
        }
    }
}

impl RetryPolicy {
    #[must_use]
    pub fn delay_for(&self, attempt: u32) -> std::time::Duration {
        let exponent = attempt.saturating_sub(1).min(10);
        let multiplier = 1u32 << exponent;
        self.base_delay
            .saturating_mul(multiplier)
            .min(self.max_delay)
    }
}

#[derive(Debug, Clone)]
pub struct PushReport {
    pub accepted: usize,
    pub duplicates: usize,
    pub rejected: usize,
    pub attempts: u32,
}

pub struct SyncWorker {
    client: SyncClient,
    policy: RetryPolicy,
}

impl SyncWorker {
    #[must_use]
    pub fn new(client: SyncClient, policy: RetryPolicy) -> Self {
        Self { client, policy }
    }

    pub async fn push_pending(
        &self,
        outbox: &dyn tpv_application::SyncOutboxRepository,
        device_id: DeviceId,
        batch_size: u32,
    ) -> Result<PushReport, SyncError> {
        let events = outbox.pending_events(batch_size).await?;
        if events.is_empty() {
            return Ok(PushReport {
                accepted: 0,
                duplicates: 0,
                rejected: 0,
                attempts: 0,
            });
        }
        let mut attempt = 1;
        loop {
            match self.client.push(device_id, events.clone()).await {
                Ok(response) => {
                    let mut acknowledged = response.accepted.clone();
                    acknowledged.extend(response.duplicates.iter().copied());
                    outbox.mark_events_sent(&acknowledged).await?;
                    return Ok(PushReport {
                        accepted: response.accepted.len(),
                        duplicates: response.duplicates.len(),
                        rejected: response.rejected.len(),
                        attempts: attempt,
                    });
                }
                Err(error) if attempt < self.policy.max_attempts && is_retryable(&error) => {
                    tokio::time::sleep(self.policy.delay_for(attempt)).await;
                    attempt += 1;
                }
                Err(error) => return Err(error),
            }
        }
    }
}

fn is_retryable(error: &SyncError) -> bool {
    match error {
        SyncError::Http(error) => {
            error.is_timeout()
                || error.is_connect()
                || error.status().is_none_or(|status| status.is_server_error())
        }
        SyncError::Rejected { status, .. } => *status >= 500,
        SyncError::InvalidResponse(_) => false,
        SyncError::Application(_) => false,
    }
}

#[cfg(test)]
mod retry_tests {
    use super::*;

    #[test]
    fn retry_delay_grows_and_is_capped() {
        let policy = RetryPolicy {
            max_attempts: 6,
            base_delay: std::time::Duration::from_millis(100),
            max_delay: std::time::Duration::from_millis(350),
        };
        assert_eq!(policy.delay_for(1), std::time::Duration::from_millis(100));
        assert_eq!(policy.delay_for(2), std::time::Duration::from_millis(200));
        assert_eq!(policy.delay_for(3), std::time::Duration::from_millis(350));
        assert_eq!(policy.delay_for(10), std::time::Duration::from_millis(350));
    }
}
