use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::{ProductId, Sku};
use crate::tax::TaxRate;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Family {
    pub code: String,
    pub name: String,
    pub active: bool,
}

impl Family {
    pub fn new(code: impl Into<String>, name: impl Into<String>) -> Result<Self> {
        let code = code.into().trim().to_uppercase();
        let name = name.into().trim().to_string();
        if code.is_empty() || name.is_empty() {
            return Err(DomainError::Validation(
                "family code and name are required".into(),
            ));
        }
        Ok(Self {
            code,
            name,
            active: true,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Product {
    pub id: ProductId,
    pub sku: Sku,
    pub barcode: Option<String>,
    pub name: String,
    pub family_code: Option<String>,
    /// Tax-inclusive unit price.
    pub unit_price: Decimal,
    pub tax_rate: TaxRate,
    pub active: bool,
}

impl Product {
    pub fn new(
        id: ProductId,
        sku: Sku,
        name: impl Into<String>,
        unit_price: Decimal,
        tax_rate: TaxRate,
    ) -> Result<Self> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(DomainError::Validation("product name is required".into()));
        }
        if unit_price < Decimal::ZERO {
            return Err(DomainError::Validation(
                "unit price cannot be negative".into(),
            ));
        }
        if unit_price.scale() > 4 {
            return Err(DomainError::Validation(
                "unit price supports at most four decimals".into(),
            ));
        }
        Ok(Self {
            id,
            sku,
            barcode: None,
            name,
            family_code: None,
            unit_price,
            tax_rate,
            active: true,
        })
    }

    pub fn set_barcode(&mut self, barcode: impl Into<String>) -> Result<()> {
        let barcode = barcode.into().trim().to_string();
        if barcode.is_empty() {
            return Err(DomainError::Validation("barcode must not be empty".into()));
        }
        if !barcode.chars().all(|c| c.is_ascii_digit()) {
            return Err(DomainError::Validation(
                "barcode must contain digits only".into(),
            ));
        }
        self.barcode = Some(barcode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_rejects_negative_price() {
        let sku = Sku::new("A-1").unwrap();
        let err = Product::new(
            ProductId::new(),
            sku,
            "Test",
            Decimal::new(-1, 0),
            TaxRate::standard(),
        );
        assert!(err.is_err());
    }

    #[test]
    fn barcode_must_be_numeric() {
        let mut product = Product::new(
            ProductId::new(),
            Sku::new("A-1").unwrap(),
            "Test",
            Decimal::new(100, 2),
            TaxRate::standard(),
        )
        .unwrap();
        assert!(product.set_barcode("ABC").is_err());
        assert!(product.set_barcode("8412345678901").is_ok());
    }
}
