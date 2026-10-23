use std::collections::HashMap;
use std::sync::Arc;

use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::ids::{StoreId, UserId};
use tpv_domain::timeclock::TimeEntry;

use crate::dto::{TimeClockStatus, TimeEntryView};
use crate::error::{ApplicationError, Result};
use crate::ports::{Clock, TimeClockRepository, UserRepository};

#[derive(Clone)]
pub struct TimeClockService {
    entries: Arc<dyn TimeClockRepository>,
    users: Arc<dyn UserRepository>,
    clock: Arc<dyn Clock>,
}

impl TimeClockService {
    pub fn new(
        entries: Arc<dyn TimeClockRepository>,
        users: Arc<dyn UserRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            entries,
            users,
            clock,
        }
    }

    pub async fn status(
        &self,
        store_id: StoreId,
        viewer_id: UserId,
        all_users: bool,
        limit: u32,
    ) -> Result<TimeClockStatus> {
        let limit = limit.clamp(1, 200);
        let filter = if all_users { None } else { Some(viewer_id) };
        let active = self.entries.find_open(viewer_id, store_id).await?;
        let recent = self.entries.list_recent(store_id, filter, limit).await?;
        let names = self.user_names().await?;
        let to_view = |entry: TimeEntry| TimeEntryView {
            user_name: names
                .get(&entry.user_id)
                .cloned()
                .unwrap_or_else(|| "Usuario desconocido".to_string()),
            entry,
        };
        Ok(TimeClockStatus {
            active: active.map(to_view),
            recent: recent.into_iter().map(to_view).collect(),
        })
    }

    pub async fn clock_in(&self, user_id: UserId, store_id: StoreId) -> Result<TimeEntry> {
        if self.entries.find_open(user_id, store_id).await?.is_some() {
            return Err(ApplicationError::Conflict(
                "ya existe un fichaje abierto para este usuario".into(),
            ));
        }
        let now = self.clock.now();
        let entry = TimeEntry::clock_in(user_id, store_id, now);
        let audit = AuditEvent::new(
            now,
            Some(user_id),
            Some(store_id),
            AuditAction::TimeClockIn,
            "time_entry",
            entry.id.to_string(),
            serde_json::json!({
                "user_id": user_id,
                "clock_in": entry.clock_in,
            }),
        )?;
        self.entries.save(&entry, &audit).await?;
        Ok(entry)
    }

    pub async fn clock_out(&self, user_id: UserId, store_id: StoreId) -> Result<TimeEntry> {
        let mut entry = self
            .entries
            .find_open(user_id, store_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound("no hay un fichaje abierto".into()))?;
        let now = self.clock.now();
        entry.clock_out(now)?;
        let duration_seconds = entry
            .duration()
            .map(|duration| duration.whole_seconds())
            .unwrap_or_default();
        let audit = AuditEvent::new(
            now,
            Some(user_id),
            Some(store_id),
            AuditAction::TimeClockOut,
            "time_entry",
            entry.id.to_string(),
            serde_json::json!({
                "user_id": user_id,
                "clock_in": entry.clock_in,
                "clock_out": entry.clock_out,
                "duration_seconds": duration_seconds,
            }),
        )?;
        self.entries.save(&entry, &audit).await?;
        Ok(entry)
    }

    async fn user_names(&self) -> Result<HashMap<UserId, String>> {
        Ok(self
            .users
            .list_users()
            .await?
            .into_iter()
            .map(|user| (user.id, user.display_name))
            .collect())
    }
}
