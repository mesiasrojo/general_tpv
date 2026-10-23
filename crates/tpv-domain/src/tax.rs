use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::TaxId;
use crate::money::Money;

/// Spanish VAT rate expressed in basis points (2100 = 21 %).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaxRate {
    pub code: String,
    pub percent_bp: u32,
}

impl TaxRate {
    pub fn new(code: impl Into<String>, percent_bp: u32) -> Result<Self> {
        let code = code.into().trim().to_string();
        if code.is_empty() {
            return Err(DomainError::Validation("tax code must not be empty".into()));
        }
        if percent_bp > 10_000 {
            return Err(DomainError::InvalidTaxRate(percent_bp));
        }
        Ok(Self { code, percent_bp })
    }

    pub fn exempt() -> Self {
        Self {
            code: "EXEMPT".into(),
            percent_bp: 0,
        }
    }

    pub fn standard() -> Self {
        Self {
            code: "IVA21".into(),
            percent_bp: 2100,
        }
    }

    pub fn reduced() -> Self {
        Self {
            code: "IVA10".into(),
            percent_bp: 1000,
        }
    }

    pub fn super_reduced() -> Self {
        Self {
            code: "IVA4".into(),
            percent_bp: 400,
        }
    }

    #[must_use]
    pub fn factor(&self) -> Decimal {
        Decimal::ONE + Decimal::new(i64::from(self.percent_bp), 4)
    }

    /// Extracts the VAT contained in a tax-inclusive gross amount.
    pub fn tax_from_gross(&self, gross: Money) -> Result<Money> {
        if self.percent_bp == 0 {
            return Ok(Money::ZERO);
        }
        let gross_decimal = gross.to_decimal();
        let tax = gross_decimal - (gross_decimal / self.factor());
        Money::from_decimal_round(tax)
    }

    /// Net amount (base imponible) from a tax-inclusive gross amount.
    pub fn net_from_gross(&self, gross: Money) -> Result<Money> {
        gross.checked_sub(self.tax_from_gross(gross)?)
    }
}

/// Editable VAT master record used by the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tax {
    pub id: TaxId,
    pub code: String,
    pub name: String,
    pub percent_bp: u32,
    pub active: bool,
}

impl Tax {
    pub fn new(
        id: TaxId,
        code: impl Into<String>,
        name: impl Into<String>,
        percent_bp: u32,
    ) -> Result<Self> {
        let code = code.into().trim().to_uppercase();
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(DomainError::Validation("tax name is required".into()));
        }
        if code.len() > 16 {
            return Err(DomainError::Validation(
                "tax code supports at most 16 characters".into(),
            ));
        }
        if name.len() > 120 {
            return Err(DomainError::Validation(
                "tax name supports at most 120 characters".into(),
            ));
        }
        TaxRate::new(code.clone(), percent_bp)?;
        Ok(Self {
            id,
            code,
            name,
            percent_bp,
            active: true,
        })
    }

    #[must_use]
    pub fn rate(&self) -> TaxRate {
        TaxRate {
            code: self.code.clone(),
            percent_bp: self.percent_bp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxBreakdown {
    pub code: String,
    pub percent_bp: u32,
    pub gross: Money,
    pub net: Money,
    pub tax: Money,
}

impl TaxBreakdown {
    pub fn from_gross(rate: &TaxRate, gross: Money) -> Result<Self> {
        let tax = rate.tax_from_gross(gross)?;
        let net = gross.checked_sub(tax)?;
        Ok(Self {
            code: rate.code.clone(),
            percent_bp: rate.percent_bp,
            gross,
            net,
            tax,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_21_percent_vat_from_gross() {
        let rate = TaxRate::standard();
        let gross = Money::from_cents(1210);
        let tax = rate.tax_from_gross(gross).unwrap();
        let net = rate.net_from_gross(gross).unwrap();
        assert_eq!(tax, Money::from_cents(210));
        assert_eq!(net, Money::from_cents(1000));
    }

    #[test]
    fn handles_exempt_rate() {
        let rate = TaxRate::exempt();
        assert_eq!(
            rate.tax_from_gross(Money::from_cents(1000)).unwrap(),
            Money::ZERO
        );
    }

    #[test]
    fn tax_master_validates_identity_and_exposes_its_rate() {
        let tax = Tax::new(TaxId::new(), "  iva21  ", "IVA general", 2100).unwrap();
        assert_eq!(tax.code, "IVA21");
        assert_eq!(tax.name, "IVA general");
        assert_eq!(tax.rate(), TaxRate::new("IVA21", 2100).unwrap());
        assert!(tax.active);
        assert!(Tax::new(TaxId::new(), "IVA21", "   ", 2100).is_err());
        assert!(Tax::new(TaxId::new(), "IVA21", "IVA", 10_001).is_err());
    }

    #[test]
    fn rejects_invalid_rates() {
        assert!(TaxRate::new("BAD", 10_001).is_err());
    }
}
