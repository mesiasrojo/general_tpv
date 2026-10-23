use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{DomainError, Result};
use crate::ids::{CashMovementId, CashSessionId, RegisterId, StoreId, UserId};
use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashMovementKind {
    Expense,
    Withdrawal,
    Deposit,
}

impl CashMovementKind {
    #[must_use]
    pub fn affects_cash(self) -> bool {
        true
    }

    #[must_use]
    pub fn sign(self) -> i64 {
        match self {
            Self::Expense | Self::Withdrawal => -1,
            Self::Deposit => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashMovement {
    pub id: CashMovementId,
    pub kind: CashMovementKind,
    pub amount: Money,
    pub reason: String,
    pub created_at: OffsetDateTime,
}

impl CashMovement {
    pub fn new(
        id: CashMovementId,
        kind: CashMovementKind,
        amount: Money,
        reason: impl Into<String>,
        created_at: OffsetDateTime,
    ) -> Result<Self> {
        if !amount.is_positive() {
            return Err(DomainError::Validation(
                "cash movement amount must be positive".into(),
            ));
        }
        let reason = reason.into().trim().to_string();
        if reason.is_empty() {
            return Err(DomainError::Validation(
                "cash movement reason is required".into(),
            ));
        }
        Ok(Self {
            id,
            kind,
            amount,
            reason,
            created_at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CashCount {
    pub cash: Money,
    pub card: Money,
    pub voucher: Money,
}

impl CashCount {
    #[must_use]
    pub fn new(cash: Money, card: Money, voucher: Money) -> Self {
        Self {
            cash,
            card,
            voucher,
        }
    }

    pub fn checked_sub(self, other: Self) -> Result<Self> {
        Ok(Self {
            cash: self.cash.checked_sub(other.cash)?,
            card: self.card.checked_sub(other.card)?,
            voucher: self.voucher.checked_sub(other.voucher)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CashSessionState {
    Open,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashCloseRecord {
    pub closed_at: OffsetDateTime,
    pub closed_by: UserId,
    pub opening_float: Money,
    pub cash_sales: Money,
    pub card_sales: Money,
    pub voucher_sales: Money,
    pub expenses: Money,
    pub withdrawals: Money,
    pub deposits: Money,
    pub expected: CashCount,
    pub counted: CashCount,
    pub differences: CashCount,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashReopenRecord {
    pub reopened_at: OffsetDateTime,
    pub reopened_by: UserId,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CashSession {
    pub id: CashSessionId,
    pub store_id: StoreId,
    pub register_id: RegisterId,
    pub opened_by: UserId,
    pub opened_at: OffsetDateTime,
    pub opening_float: Money,
    pub movements: Vec<CashMovement>,
    pub close_history: Vec<CashCloseRecord>,
    pub reopen_history: Vec<CashReopenRecord>,
    pub state: CashSessionState,
    pub version: u32,
}

impl CashSession {
    pub fn open(
        store_id: StoreId,
        register_id: RegisterId,
        opened_by: UserId,
        opening_float: Money,
        opened_at: OffsetDateTime,
    ) -> Result<Self> {
        if opening_float.is_negative() {
            return Err(DomainError::Validation(
                "opening float cannot be negative".into(),
            ));
        }
        Ok(Self {
            id: CashSessionId::new(),
            store_id,
            register_id,
            opened_by,
            opened_at,
            opening_float,
            movements: Vec::new(),
            close_history: Vec::new(),
            reopen_history: Vec::new(),
            state: CashSessionState::Open,
            version: 1,
        })
    }

    pub fn add_movement(&mut self, movement: CashMovement) -> Result<()> {
        if self.state != CashSessionState::Open {
            return Err(DomainError::InvalidState("cash session is closed".into()));
        }
        self.movements.push(movement);
        Ok(())
    }

    pub fn expense(
        &mut self,
        amount: Money,
        reason: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<()> {
        let movement = CashMovement::new(
            CashMovementId::new(),
            CashMovementKind::Expense,
            amount,
            reason,
            now,
        )?;
        self.add_movement(movement)
    }

    pub fn withdrawal(
        &mut self,
        amount: Money,
        reason: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<()> {
        let movement = CashMovement::new(
            CashMovementId::new(),
            CashMovementKind::Withdrawal,
            amount,
            reason,
            now,
        )?;
        self.add_movement(movement)
    }

    pub fn deposit(
        &mut self,
        amount: Money,
        reason: impl Into<String>,
        now: OffsetDateTime,
    ) -> Result<()> {
        let movement = CashMovement::new(
            CashMovementId::new(),
            CashMovementKind::Deposit,
            amount,
            reason,
            now,
        )?;
        self.add_movement(movement)
    }

    pub fn close(
        &mut self,
        now: OffsetDateTime,
        closed_by: UserId,
        cash_sales: Money,
        card_sales: Money,
        voucher_sales: Money,
        counted: CashCount,
    ) -> Result<CashCloseRecord> {
        if self.state != CashSessionState::Open {
            return Err(DomainError::InvalidState(
                "cash session is already closed".into(),
            ));
        }
        let expenses = self.total_for(CashMovementKind::Expense)?;
        let withdrawals = self.total_for(CashMovementKind::Withdrawal)?;
        let deposits = self.total_for(CashMovementKind::Deposit)?;
        let expected_cash = self
            .opening_float
            .checked_add(cash_sales)?
            .checked_add(deposits)?
            .checked_sub(expenses)?
            .checked_sub(withdrawals)?;
        let expected = CashCount::new(expected_cash, card_sales, voucher_sales);
        let differences = counted.checked_sub(expected)?;
        let record = CashCloseRecord {
            closed_at: now,
            closed_by,
            opening_float: self.opening_float,
            cash_sales,
            card_sales,
            voucher_sales,
            expenses,
            withdrawals,
            deposits,
            expected,
            counted,
            differences,
            version: self.version,
        };
        self.close_history.push(record.clone());
        self.state = CashSessionState::Closed;
        self.version = self.version.saturating_add(1);
        Ok(record)
    }

    pub fn reopen(
        &mut self,
        now: OffsetDateTime,
        reopened_by: UserId,
        reason: impl Into<String>,
    ) -> Result<()> {
        if self.state != CashSessionState::Closed {
            return Err(DomainError::InvalidState(
                "cash session is not closed".into(),
            ));
        }
        let reason = reason.into().trim().to_string();
        if reason.is_empty() {
            return Err(DomainError::Validation(
                "reopening a cash session requires a reason".into(),
            ));
        }
        self.reopen_history.push(CashReopenRecord {
            reopened_at: now,
            reopened_by,
            reason,
        });
        self.state = CashSessionState::Open;
        self.version = self.version.saturating_add(1);
        Ok(())
    }

    pub fn total_for(&self, kind: CashMovementKind) -> Result<Money> {
        let mut total = Money::ZERO;
        for movement in self.movements.iter().filter(|m| m.kind == kind) {
            total = total.checked_add(movement.amount)?;
        }
        Ok(total)
    }

    #[must_use]
    pub fn last_close(&self) -> Option<&CashCloseRecord> {
        self.close_history.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn now() -> OffsetDateTime {
        datetime!(2026-10-22 19:15:46 UTC)
    }

    #[test]
    fn closes_with_expected_cash_and_differences() {
        let mut session = CashSession::open(
            StoreId::new(),
            RegisterId::new(),
            UserId::new(),
            Money::from_cents(10_000),
            now(),
        )
        .unwrap();
        session
            .expense(Money::from_cents(2_000), "cleaning", now())
            .unwrap();
        let record = session
            .close(
                now(),
                UserId::new(),
                Money::from_cents(5_000),
                Money::from_cents(3_000),
                Money::ZERO,
                CashCount::new(
                    Money::from_cents(13_000),
                    Money::from_cents(3_000),
                    Money::ZERO,
                ),
            )
            .unwrap();
        assert_eq!(record.expected.cash, Money::from_cents(13_000));
        assert_eq!(record.differences.cash, Money::ZERO);
        assert_eq!(record.differences.card, Money::ZERO);
        assert_eq!(session.state, CashSessionState::Closed);
    }

    #[test]
    fn closed_session_rejects_new_movements() {
        let mut session = CashSession::open(
            StoreId::new(),
            RegisterId::new(),
            UserId::new(),
            Money::ZERO,
            now(),
        )
        .unwrap();
        session
            .close(
                now(),
                UserId::new(),
                Money::ZERO,
                Money::ZERO,
                Money::ZERO,
                CashCount::default(),
            )
            .unwrap();
        assert!(session.expense(Money::from_cents(1), "x", now()).is_err());
    }

    #[test]
    fn reopens_a_closed_session_with_a_reason() {
        let mut session = CashSession::open(
            StoreId::new(),
            RegisterId::new(),
            UserId::new(),
            Money::ZERO,
            now(),
        )
        .unwrap();
        session
            .close(
                now(),
                UserId::new(),
                Money::ZERO,
                Money::ZERO,
                Money::ZERO,
                CashCount::default(),
            )
            .unwrap();
        assert!(session.reopen(now(), UserId::new(), "").is_err());
        session
            .reopen(now(), UserId::new(), "counting mistake")
            .unwrap();
        assert_eq!(session.state, CashSessionState::Open);
        assert_eq!(session.version, 3);
    }
}
