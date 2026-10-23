use std::sync::Arc;

use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::cash::{CashCloseRecord, CashMovementKind, CashSession};
use tpv_domain::ids::{CashSessionId, RegisterId, StoreId};
use tpv_domain::money::Money;
use tpv_domain::sync::SyncEvent;

use crate::dto::{
    CashExpenseRequest, CashMovementRequest, CloseCashRequest, OpenCashRequest, ReopenCashRequest,
    SyncContext,
};
use crate::error::{ApplicationError, Result};
use crate::ports::{CashRepository, Clock, SaleRepository};

#[derive(Clone)]
pub struct CashService {
    cash_repo: Arc<dyn CashRepository>,
    sale_repo: Arc<dyn SaleRepository>,
    clock: Arc<dyn Clock>,
    sync: SyncContext,
}

impl CashService {
    pub fn new(
        cash_repo: Arc<dyn CashRepository>,
        sale_repo: Arc<dyn SaleRepository>,
        clock: Arc<dyn Clock>,
        sync: SyncContext,
    ) -> Self {
        Self {
            cash_repo,
            sale_repo,
            clock,
            sync,
        }
    }

    pub async fn open(&self, request: OpenCashRequest) -> Result<CashSession> {
        let now = self.clock.now();
        let session = CashSession::open(
            request.store_id,
            request.register_id,
            request.opened_by,
            Money::from_cents(request.opening_float_cents),
            now,
        )?;
        let event = self.build_event(
            session.store_id,
            "cash_session",
            session.id.as_uuid(),
            "cash.opened",
            now,
            serde_json::to_value(&session)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            Some(request.opened_by),
            Some(session.store_id),
            AuditAction::CashOpened,
            "cash_session",
            session.id.to_string(),
            serde_json::json!({
                "register_id": session.register_id,
                "opening_float_cents": session.opening_float.cents(),
            }),
        )?;
        self.cash_repo.save(&session, Some(&event), &audit).await?;
        Ok(session)
    }

    pub async fn expense(&self, request: CashExpenseRequest) -> Result<CashSession> {
        self.movement(CashMovementRequest {
            session_id: request.session_id,
            kind: CashMovementKind::Expense,
            amount_cents: request.amount_cents,
            reason: request.reason,
        })
        .await
    }

    pub async fn movement(&self, request: CashMovementRequest) -> Result<CashSession> {
        let mut session = self.load(request.session_id).await?;
        let now = self.clock.now();
        let amount = Money::from_cents(request.amount_cents);
        match request.kind {
            CashMovementKind::Expense => session.expense(amount, request.reason.clone(), now)?,
            CashMovementKind::Withdrawal => {
                session.withdrawal(amount, request.reason.clone(), now)?;
            }
            CashMovementKind::Deposit => session.deposit(amount, request.reason.clone(), now)?,
        }
        let movement = match request.kind {
            CashMovementKind::Expense => "expense",
            CashMovementKind::Withdrawal => "withdrawal",
            CashMovementKind::Deposit => "deposit",
        };
        let event = self.build_event(
            session.store_id,
            "cash_session",
            session.id.as_uuid(),
            "cash.movement",
            now,
            serde_json::to_value(&session)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            None,
            Some(session.store_id),
            AuditAction::CashMovement,
            "cash_session",
            session.id.to_string(),
            serde_json::json!({
                "movement": movement,
                "amount_cents": request.amount_cents,
                "reason": request.reason,
            }),
        )?;
        self.cash_repo.save(&session, Some(&event), &audit).await?;
        Ok(session)
    }

    pub async fn close(&self, request: CloseCashRequest) -> Result<CashCloseRecord> {
        let mut session = self.load(request.session_id).await?;
        let now = self.clock.now();
        let sales = self
            .sale_repo
            .payment_totals_since(session.store_id, session.register_id, session.opened_at)
            .await?;
        let record = session.close(
            now,
            request.closed_by,
            sales.cash,
            sales.card,
            sales.voucher,
            request.counted,
        )?;
        let event = self.build_event(
            session.store_id,
            "cash_session",
            session.id.as_uuid(),
            "cash.closed",
            now,
            serde_json::to_value(&session)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            Some(request.closed_by),
            Some(session.store_id),
            AuditAction::CashClosed,
            "cash_session",
            session.id.to_string(),
            serde_json::json!({
                "expected_cash_cents": record.expected.cash.cents(),
                "counted_cash_cents": record.counted.cash.cents(),
                "difference_cash_cents": record.differences.cash.cents(),
                "version": record.version,
            }),
        )?;
        self.cash_repo.save(&session, Some(&event), &audit).await?;
        Ok(record)
    }

    pub async fn reopen(&self, request: ReopenCashRequest) -> Result<CashSession> {
        let mut session = self.load(request.session_id).await?;
        let now = self.clock.now();
        session.reopen(now, request.reopened_by, request.reason.clone())?;
        let event = self.build_event(
            session.store_id,
            "cash_session",
            session.id.as_uuid(),
            "cash.reopened",
            now,
            serde_json::to_value(&session)
                .map_err(|error| ApplicationError::Serialization(error.to_string()))?,
        )?;
        let audit = AuditEvent::new(
            now,
            Some(request.reopened_by),
            Some(session.store_id),
            AuditAction::CashReopened,
            "cash_session",
            session.id.to_string(),
            serde_json::json!({
                "reason": request.reason,
                "version": session.version,
            }),
        )?;
        self.cash_repo.save(&session, Some(&event), &audit).await?;
        Ok(session)
    }

    pub async fn find_open(&self, register_id: RegisterId) -> Result<Option<CashSession>> {
        self.cash_repo.find_open(register_id).await
    }

    pub async fn list_recent(&self, store_id: StoreId, limit: u32) -> Result<Vec<CashSession>> {
        self.cash_repo.list_recent(store_id, limit).await
    }

    async fn load(&self, id: CashSessionId) -> Result<CashSession> {
        self.cash_repo
            .find(id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("cash session {id}")))
    }

    fn build_event(
        &self,
        store_id: StoreId,
        aggregate_type: &str,
        aggregate_id: uuid::Uuid,
        event_type: &str,
        now: time::OffsetDateTime,
        payload: serde_json::Value,
    ) -> Result<SyncEvent> {
        let sequence = u64::try_from(now.unix_timestamp_nanos()).unwrap_or_default();
        Ok(SyncEvent::new(
            self.sync.tenant_id,
            store_id,
            self.sync.device_id,
            sequence,
            aggregate_type,
            aggregate_id,
            event_type,
            1,
            now,
            payload,
        )?)
    }
}
