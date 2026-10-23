use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Neg;

use crate::error::{DomainError, Result};

/// Monetary amount in euro cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    #[must_use]
    pub const fn cents(self) -> i64 {
        self.0
    }

    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    #[must_use]
    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    #[must_use]
    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn abs(self) -> Result<Self> {
        self.0
            .checked_abs()
            .map(Self)
            .ok_or(DomainError::MoneyOverflow)
    }

    pub fn checked_add(self, other: Self) -> Result<Self> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::MoneyOverflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(DomainError::MoneyOverflow)
    }

    /// Rounds a decimal euro amount to cents using half-up (away from zero
    /// at the midpoint), which is the documented policy for this product.
    pub fn from_decimal_round(value: Decimal) -> Result<Self> {
        let rounded = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
        let cents = rounded * Decimal::ONE_HUNDRED;
        cents.to_i64().map(Self).ok_or(DomainError::MoneyOverflow)
    }

    #[must_use]
    pub fn to_decimal(self) -> Decimal {
        Decimal::new(self.0, 2)
    }

    #[must_use]
    pub fn format_eur(self) -> String {
        format!("{:.2} €", self.to_decimal())
    }
}

impl Neg for Money {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_decimal().fmt(f)
    }
}

impl std::iter::Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, item| {
            acc.checked_add(item).unwrap_or(Self::ZERO)
        })
    }
}

/// Positive quantity. Fractional quantities are supported for weighted goods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Quantity(Decimal);

impl Quantity {
    pub fn new(value: Decimal) -> Result<Self> {
        if value <= Decimal::ZERO {
            return Err(DomainError::InvalidQuantity);
        }
        if value.scale() > 3 {
            return Err(DomainError::Validation(
                "quantity supports at most three decimals".into(),
            ));
        }
        Ok(Self(value.normalize()))
    }

    pub fn one() -> Self {
        Self(Decimal::ONE)
    }

    #[must_use]
    pub fn as_decimal(self) -> Decimal {
        self.0
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<Decimal> for Quantity {
    type Error = DomainError;

    fn try_from(value: Decimal) -> Result<Self> {
        Self::new(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_rounds_half_up() {
        assert_eq!(
            Money::from_decimal_round(Decimal::new(1005, 3)).unwrap(),
            Money::from_cents(101)
        );
        assert_eq!(
            Money::from_decimal_round(Decimal::new(1004, 3)).unwrap(),
            Money::from_cents(100)
        );
        assert_eq!(
            Money::from_decimal_round(Decimal::new(-1005, 3)).unwrap(),
            Money::from_cents(-101)
        );
    }

    #[test]
    fn money_arithmetic_checks_overflow() {
        assert_eq!(
            Money::from_cents(100)
                .checked_add(Money::from_cents(50))
                .unwrap(),
            Money::from_cents(150)
        );
        assert!(
            Money::from_cents(i64::MAX)
                .checked_add(Money::from_cents(1))
                .is_err()
        );
    }

    #[test]
    fn quantity_rejects_zero_and_negative() {
        assert!(Quantity::new(Decimal::ZERO).is_err());
        assert!(Quantity::new(Decimal::NEGATIVE_ONE).is_err());
        assert_eq!(
            Quantity::new(Decimal::new(1500, 3)).unwrap().as_decimal(),
            Decimal::new(15, 1)
        );
    }
}
