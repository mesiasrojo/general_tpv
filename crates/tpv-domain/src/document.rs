use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::ids::{SeriesId, StoreId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentType {
    Ticket,
    Invoice,
    Rectificative,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSeries {
    pub id: SeriesId,
    pub store_id: StoreId,
    pub document_type: DocumentType,
    pub code: String,
    pub year: u16,
    pub next_number: u64,
}

impl DocumentSeries {
    pub fn new(
        store_id: StoreId,
        document_type: DocumentType,
        code: impl Into<String>,
        year: u16,
        next_number: u64,
    ) -> Result<Self> {
        let code = code.into().trim().to_uppercase();
        if code.is_empty() {
            return Err(DomainError::Validation(
                "document series code is required".into(),
            ));
        }
        if !(2000..=2100).contains(&year) {
            return Err(DomainError::Validation("invalid document year".into()));
        }
        if next_number == 0 {
            return Err(DomainError::Validation(
                "document sequence must start at one".into(),
            ));
        }
        Ok(Self {
            id: SeriesId::new(),
            store_id,
            document_type,
            code,
            year,
            next_number,
        })
    }

    pub fn allocate(&mut self) -> Result<u64> {
        let number = self.next_number;
        self.next_number = self
            .next_number
            .checked_add(1)
            .ok_or(DomainError::SeriesExhausted)?;
        Ok(number)
    }

    #[must_use]
    pub fn peek(&self) -> u64 {
        self.next_number
    }

    #[must_use]
    pub fn format_number(&self, number: u64) -> String {
        format!("{}-{}-{number:06}", self.code, self.year)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentNumber {
    pub series: String,
    pub number: u64,
}

impl DocumentNumber {
    #[must_use]
    pub fn format(&self) -> String {
        format!("{}-{:06}", self.series, self.number)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_sequential_numbers_without_reuse() {
        let mut series =
            DocumentSeries::new(StoreId::new(), DocumentType::Invoice, "FAC", 2026, 1).unwrap();
        assert_eq!(series.allocate().unwrap(), 1);
        assert_eq!(series.allocate().unwrap(), 2);
        assert_eq!(series.peek(), 3);
        assert_eq!(series.format_number(7), "FAC-2026-000007");
    }

    #[test]
    fn rejects_invalid_series() {
        assert!(DocumentSeries::new(StoreId::new(), DocumentType::Ticket, "", 2026, 1).is_err());
        assert!(DocumentSeries::new(StoreId::new(), DocumentType::Ticket, "A", 1999, 1).is_err());
        assert!(DocumentSeries::new(StoreId::new(), DocumentType::Ticket, "A", 2026, 0).is_err());
    }
}
