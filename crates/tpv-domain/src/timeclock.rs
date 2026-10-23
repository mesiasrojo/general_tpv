use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};

use crate::error::{DomainError, Result};
use crate::ids::{StoreId, TimeEntryId, UserId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeEntry {
    pub id: TimeEntryId,
    pub user_id: UserId,
    pub store_id: StoreId,
    pub clock_in: OffsetDateTime,
    pub clock_out: Option<OffsetDateTime>,
    pub correction_reason: Option<String>,
}

impl TimeEntry {
    pub fn clock_in(user_id: UserId, store_id: StoreId, at: OffsetDateTime) -> Self {
        Self {
            id: TimeEntryId::new(),
            user_id,
            store_id,
            clock_in: at,
            clock_out: None,
            correction_reason: None,
        }
    }

    pub fn clock_out(&mut self, at: OffsetDateTime) -> Result<()> {
        if self.clock_out.is_some() {
            return Err(DomainError::InvalidState(
                "time entry is already closed".into(),
            ));
        }
        if at < self.clock_in {
            return Err(DomainError::Validation(
                "clock-out cannot be earlier than clock-in".into(),
            ));
        }
        self.clock_out = Some(at);
        Ok(())
    }

    #[must_use]
    pub fn duration(&self) -> Option<Duration> {
        self.clock_out.map(|end| end - self.clock_in)
    }

    pub fn correct(
        &mut self,
        clock_in: OffsetDateTime,
        clock_out: Option<OffsetDateTime>,
        reason: impl Into<String>,
    ) -> Result<()> {
        let reason = reason.into().trim().to_string();
        if reason.is_empty() {
            return Err(DomainError::Validation(
                "time correction requires a reason".into(),
            ));
        }
        if let Some(end) = clock_out {
            if end < clock_in {
                return Err(DomainError::Validation(
                    "clock-out cannot be earlier than clock-in".into(),
                ));
            }
        }
        self.clock_in = clock_in;
        self.clock_out = clock_out;
        self.correction_reason = Some(reason);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    #[test]
    fn computes_duration_and_rejects_double_close() {
        let mut entry = TimeEntry::clock_in(
            UserId::new(),
            StoreId::new(),
            datetime!(2026-10-22 08:00:00 UTC),
        );
        entry.clock_out(datetime!(2026-10-22 16:30:00 UTC)).unwrap();
        assert_eq!(
            entry.duration().unwrap(),
            Duration::hours(8) + Duration::minutes(30)
        );
        assert!(entry.clock_out(datetime!(2026-10-22 17:00:00 UTC)).is_err());
    }

    #[test]
    fn corrections_require_a_reason() {
        let mut entry = TimeEntry::clock_in(
            UserId::new(),
            StoreId::new(),
            datetime!(2026-10-22 08:00:00 UTC),
        );
        assert!(
            entry
                .correct(datetime!(2026-10-22 08:05:00 UTC), None, "")
                .is_err()
        );
    }
}
