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
                    // A per-event rejection is deterministic: retrying the
                    // same envelope will not succeed. Quarantine it so the
                    // queue can keep moving and an operator can inspect it.
                    for rejected in &response.rejected {
                        outbox
                            .mark_events_failed(&[rejected.event_id], &rejected.reason)
                            .await?;
                    }
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
                Err(error) => {
                    // Exhausted retries or a non-retryable failure. Keep the
                    // batch in quarantine instead of blocking the outbox
                    // forever; an operator can requeue it from the console.
                    let event_ids = events
                        .iter()
                        .map(|event| event.event_id)
                        .collect::<Vec<_>>();
                    outbox
                        .mark_events_failed(&event_ids, &error.to_string())
                        .await?;
                    return Err(error);
                }
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

#[cfg(test)]
mod worker_tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tpv_application::{QuarantinedEvent, SyncOutboxRepository, SyncStatus};
    use tpv_domain::ids::{StoreId, TenantId};
    use tpv_domain::sync::SyncEvent;

    #[derive(Default)]
    struct FakeOutbox {
        events: Vec<SyncEvent>,
        sent: Mutex<Vec<Uuid>>,
        failed: Mutex<Vec<(Uuid, String)>>,
    }

    impl FakeOutbox {
        fn with_events(events: Vec<SyncEvent>) -> Self {
            Self {
                events,
                ..Self::default()
            }
        }

        fn failed_entries(&self) -> Vec<(Uuid, String)> {
            self.failed.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl SyncOutboxRepository for FakeOutbox {
        async fn pending_events(&self, limit: u32) -> tpv_application::Result<Vec<SyncEvent>> {
            let sent = self.sent.lock().unwrap();
            let failed = self.failed.lock().unwrap();
            Ok(self
                .events
                .iter()
                .filter(|event| {
                    !sent.contains(&event.event_id)
                        && !failed.iter().any(|(id, _)| *id == event.event_id)
                })
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn mark_events_sent(&self, event_ids: &[Uuid]) -> tpv_application::Result<()> {
            self.sent.lock().unwrap().extend_from_slice(event_ids);
            Ok(())
        }

        async fn mark_events_failed(
            &self,
            event_ids: &[Uuid],
            error: &str,
        ) -> tpv_application::Result<()> {
            self.failed
                .lock()
                .unwrap()
                .extend(event_ids.iter().map(|id| (*id, error.to_string())));
            Ok(())
        }

        async fn sync_status(&self) -> tpv_application::Result<SyncStatus> {
            Ok(SyncStatus {
                pending: self.events.len() as u32,
                ..SyncStatus::default()
            })
        }

        async fn list_failed(&self, _limit: u32) -> tpv_application::Result<Vec<QuarantinedEvent>> {
            Ok(Vec::new())
        }

        async fn requeue_failed(&self, _event_ids: &[Uuid]) -> tpv_application::Result<u64> {
            Ok(0)
        }
    }

    fn sample_event(device_id: DeviceId) -> SyncEvent {
        SyncEvent::new(
            TenantId::new(),
            StoreId::new(),
            device_id,
            1,
            "sale",
            Uuid::now_v7(),
            "sale.confirmed",
            1,
            time::OffsetDateTime::now_utc(),
            serde_json::json!({"total": 1210}),
        )
        .unwrap()
    }

    async fn spawn_response(status: &'static str, reason: &'static str, body: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 16 * 1024];
            let _ = socket.read(&mut buffer).await;
            let response = format!(
                "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
            socket.shutdown().await.unwrap();
        });
        format!("http://{address}")
    }

    #[tokio::test]
    async fn quarantines_rejected_events_with_their_reason() {
        let device_id = DeviceId::new();
        let event = sample_event(device_id);
        let body = serde_json::json!({
            "accepted": [],
            "duplicates": [],
            "rejected": [{
                "event_id": event.event_id,
                "reason": "hash mismatch"
            }],
            "cursor": 1
        })
        .to_string();
        let url = spawn_response("200", "OK", body).await;
        let outbox = FakeOutbox::with_events(vec![event.clone()]);
        let worker = SyncWorker::new(SyncClient::new(url, None), RetryPolicy::default());

        let report = worker.push_pending(&outbox, device_id, 100).await.unwrap();

        assert_eq!(report.rejected, 1);
        assert_eq!(report.accepted, 0);
        assert_eq!(
            outbox.failed_entries(),
            vec![(event.event_id, "hash mismatch".into())]
        );
        assert!(outbox.sent.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn quarantines_batch_after_terminal_transport_error() {
        let device_id = DeviceId::new();
        let event = sample_event(device_id);
        let url = spawn_response("400", "Bad Request", "invalid batch".into()).await;
        let outbox = FakeOutbox::with_events(vec![event.clone()]);
        let worker = SyncWorker::new(
            SyncClient::new(url, None),
            RetryPolicy {
                max_attempts: 3,
                base_delay: std::time::Duration::from_millis(1),
                max_delay: std::time::Duration::from_millis(1),
            },
        );

        let result = worker.push_pending(&outbox, device_id, 100).await;

        assert!(result.is_err());
        let failed = outbox.failed_entries();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, event.event_id);
        assert!(failed[0].1.contains("400"));
    }
}
