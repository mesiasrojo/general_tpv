use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::{PriceListId, ProductId};

/// Named selling-price list. Products not included in the list keep their
/// catalogue base price.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceList {
    pub id: PriceListId,
    pub code: String,
    pub name: String,
    pub active: bool,
}

impl PriceList {
    pub fn new(id: PriceListId, code: impl Into<String>, name: impl Into<String>) -> Result<Self> {
        let code = code.into().trim().to_uppercase();
        let name = name.into().trim().to_string();
        if code.is_empty() || name.is_empty() {
            return Err(DomainError::Validation(
                "price list code and name are required".into(),
            ));
        }
        if code.len() > 32 {
            return Err(DomainError::Validation(
                "price list code supports at most 32 characters".into(),
            ));
        }
        if name.len() > 120 {
            return Err(DomainError::Validation(
                "price list name supports at most 120 characters".into(),
            ));
        }
        Ok(Self {
            id,
            code,
            name,
            active: true,
        })
    }
}

/// Explicit tax-inclusive price of a product inside a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceListItem {
    pub product_id: ProductId,
    /// Tax-inclusive unit price, same convention as `Product::unit_price`.
    pub unit_price: Decimal,
}

impl PriceListItem {
    pub fn new(product_id: ProductId, unit_price: Decimal) -> Result<Self> {
        if unit_price < Decimal::ZERO {
            return Err(DomainError::Validation(
                "price list unit price cannot be negative".into(),
            ));
        }
        if unit_price.scale() > 4 {
            return Err(DomainError::Validation(
                "price list unit price supports at most four decimals".into(),
            ));
        }
        Ok(Self {
            product_id,
            unit_price,
        })
    }
}

/// Complete versioned payload of a price list and its overrides. It is used
/// both by the UI and by the master-data outbox so remote devices can replace
/// the list atomically.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceListSnapshot {
    pub price_list: PriceList,
    pub items: Vec<PriceListItem>,
}

impl PriceListSnapshot {
    pub fn new(price_list: PriceList, mut items: Vec<PriceListItem>) -> Result<Self> {
        items.sort_by_key(|item| item.product_id.as_uuid());
        for pair in items.windows(2) {
            if pair[0].product_id == pair[1].product_id {
                return Err(DomainError::Validation(
                    "a product can only appear once in a price list".into(),
                ));
            }
        }
        Ok(Self { price_list, items })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_list_normalizes_code_and_rejects_duplicate_products() {
        let list = PriceList::new(PriceListId::new(), "  empleados  ", " Empleados ").unwrap();
        assert_eq!(list.code, "EMPLEADOS");
        assert_eq!(list.name, "Empleados");

        let product_id = ProductId::new();
        let item = PriceListItem::new(product_id, Decimal::new(125, 2)).unwrap();
        let snapshot = PriceListSnapshot::new(list.clone(), vec![item.clone()]).unwrap();
        assert_eq!(snapshot.items.len(), 1);

        let duplicate = PriceListSnapshot::new(list, vec![item.clone(), item]);
        assert!(duplicate.is_err());
        assert!(PriceListItem::new(product_id, Decimal::new(-1, 0)).is_err());
    }
}
