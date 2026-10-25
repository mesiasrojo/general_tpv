//! Bulk physical-count import from the CSV worksheet exported by the stock tab.
//!
//! Parsing is pure and independent from storage. Every row is validated before
//! the service resolves its SKU and the adapter applies the whole batch inside
//! one transaction, so a partially valid file can never adjust stock.

use std::collections::HashSet;
use std::str::FromStr;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tpv_domain::Sku;

use crate::error::{ApplicationError, Result};

const MAX_ROWS: usize = 10_000;
const DEFAULT_REASON: &str = "Conteo físico importado";
const SKU_HEADERS: &[&str] = &[
    "codigo",
    "sku",
    "codigo_articulo",
    "referencia",
    "ref",
    "articulo_id",
];
const COUNT_HEADERS: &[&str] = &[
    "conteo_fisico",
    "conteo",
    "cantidad_contada",
    "cantidad_contada_fisica",
    "stock_contado",
    "contado",
];
const REASON_HEADERS: &[&str] = &["motivo", "motivo_conteo", "observaciones", "comentario"];

/// One validated counted row, still without a resolved product identifier.
#[derive(Debug, Clone, PartialEq)]
pub struct StockCountImportRow {
    /// Physical line in the source file, used in error reports.
    pub line: usize,
    pub sku: Sku,
    pub counted_quantity: Decimal,
    pub reason: String,
}

/// A row that cannot be imported, either because it is malformed or because
/// its SKU does not exist in the catalogue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StockCountImportError {
    pub line: usize,
    pub sku: Option<String>,
    pub message: String,
}

/// Fully parsed batch. The service enriches it with catalogue lookups.
#[derive(Debug, Clone)]
pub struct StockCountImportPlan {
    pub total_rows: usize,
    pub skipped_rows: usize,
    pub rows: Vec<StockCountImportRow>,
    pub errors: Vec<StockCountImportError>,
}

/// Stable report shared by the preview and the committed response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StockCountImportReport {
    pub total_rows: usize,
    /// Rows with an empty count; they are deliberately ignored so a downloaded
    /// worksheet can be filled progressively.
    pub skipped_rows: usize,
    /// Rows whose SKU exists and whose count is valid.
    pub valid_rows: usize,
    pub invalid_rows: usize,
    /// Valid rows whose counted quantity differs from the recorded level.
    pub adjusted: usize,
    /// Valid rows whose counted quantity matches the recorded level.
    pub unchanged: usize,
    pub committed: bool,
    pub errors: Vec<StockCountImportError>,
}

impl StockCountImportReport {
    /// Invalid files are never partially applied.
    #[must_use]
    pub fn can_commit(&self) -> bool {
        !self.committed && self.invalid_rows == 0 && self.valid_rows > 0
    }
}

/// Parses the stock-count worksheet.
///
/// Required columns are a SKU column (`Codigo`, `SKU`, ...) and a count column
/// (`Conteo fisico`, `Conteo`, `Cantidad contada`, ...). The optional `Motivo`
/// column is copied to the movement reason. Empty count cells are skipped.
pub fn parse_stock_count_import_csv(content: &str) -> Result<StockCountImportPlan> {
    let content = content.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    if content.trim().is_empty() {
        return Err(ApplicationError::Validation(
            "el fichero de conteo está vacío".into(),
        ));
    }

    let delimiter = detect_delimiter(&content);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content.as_bytes());
    let headers = reader
        .headers()
        .map_err(|error| csv_error(&error))?
        .iter()
        .map(normalize_header)
        .collect::<Vec<_>>();
    let sku_idx = required_column(&headers, SKU_HEADERS, "Codigo")?;
    let count_idx = required_column(&headers, COUNT_HEADERS, "Conteo fisico")?;
    let reason_idx = find_column(&headers, REASON_HEADERS);

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut skipped_rows = 0_usize;
    let mut total_rows = 0_usize;
    let mut seen_skus = HashSet::new();

    for record in reader.records() {
        total_rows += 1;
        if total_rows > MAX_ROWS {
            return Err(ApplicationError::Validation(format!(
                "el fichero no puede superar {MAX_ROWS} filas"
            )));
        }
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                let line = error
                    .position()
                    .map_or(total_rows + 1, |position| position.line() as usize);
                errors.push(StockCountImportError {
                    line,
                    sku: None,
                    message: format!("fila CSV inválida: {error}"),
                });
                continue;
            }
        };
        let line = record
            .position()
            .map_or(total_rows + 1, |position| position.line() as usize);
        let sku_raw = value(&record, sku_idx);
        let count_raw = value(&record, count_idx);
        if count_raw.is_empty() {
            skipped_rows += 1;
            continue;
        }
        let sku = match Sku::new(sku_raw) {
            Ok(sku) => sku,
            Err(error) => {
                errors.push(StockCountImportError {
                    line,
                    sku: (!sku_raw.is_empty()).then(|| sku_raw.to_string()),
                    message: format!("código inválido: {error}"),
                });
                continue;
            }
        };
        if !seen_skus.insert(sku.as_str().to_lowercase()) {
            errors.push(StockCountImportError {
                line,
                sku: Some(sku.to_string()),
                message: "código duplicado en el fichero".into(),
            });
            continue;
        }
        let counted_quantity = match parse_count(count_raw) {
            Ok(count) => count,
            Err(message) => {
                errors.push(StockCountImportError {
                    line,
                    sku: Some(sku.to_string()),
                    message,
                });
                continue;
            }
        };
        let reason = reason_idx
            .map(|index| value(&record, index).trim())
            .filter(|value| !value.is_empty())
            .unwrap_or(DEFAULT_REASON)
            .chars()
            .take(200)
            .collect::<String>();
        rows.push(StockCountImportRow {
            line,
            sku,
            counted_quantity,
            reason,
        });
    }

    if total_rows == 0 {
        return Err(ApplicationError::Validation(
            "el CSV no contiene filas de datos".into(),
        ));
    }
    if rows.is_empty() && errors.is_empty() {
        return Err(ApplicationError::Validation(
            "el CSV no contiene conteos físicos informados".into(),
        ));
    }

    Ok(StockCountImportPlan {
        total_rows,
        skipped_rows,
        rows,
        errors,
    })
}

fn parse_count(raw: &str) -> std::result::Result<Decimal, String> {
    let normalized = raw.trim().replace(',', ".");
    let counted = Decimal::from_str(&normalized)
        .map_err(|_| format!("cantidad contada no numérica: {raw}"))?
        .normalize();
    if counted < Decimal::ZERO {
        return Err(format!("la cantidad contada no puede ser negativa: {raw}"));
    }
    if counted.scale() > 3 {
        return Err(format!(
            "la cantidad contada admite como máximo tres decimales: {raw}"
        ));
    }
    Ok(counted)
}

fn value(record: &csv::StringRecord, index: usize) -> &str {
    record.get(index).unwrap_or_default().trim()
}

fn find_column(headers: &[String], aliases: &[&str]) -> Option<usize> {
    headers
        .iter()
        .position(|header| aliases.iter().any(|alias| header == alias))
}

fn required_column(headers: &[String], aliases: &[&str], label: &str) -> Result<usize> {
    find_column(headers, aliases).ok_or_else(|| {
        ApplicationError::Validation(format!("falta la columna obligatoria {label}"))
    })
}

fn normalize_header(value: &str) -> String {
    let mut normalized = String::new();
    for character in value.trim().to_lowercase().chars() {
        let character = match character {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            other => other,
        };
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
        } else {
            normalized.push('_');
        }
    }
    normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

fn detect_delimiter(content: &str) -> u8 {
    let line = content
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    b";,\t|"
        .iter()
        .copied()
        .map(|delimiter| (delimiter, line.matches(delimiter as char).count()))
        .max_by_key(|(_, count)| *count)
        .filter(|(_, count)| *count > 0)
        .map_or(b',', |(delimiter, _)| delimiter)
}

fn csv_error(error: &csv::Error) -> ApplicationError {
    ApplicationError::Validation(format!("CSV inválido: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exported_worksheet_and_skips_blank_counts() {
        let csv = concat!(
            "\u{feff}Codigo;Articulo;Existencia registrada;Minimo;A reponer;Bajo minimo;Conteo fisico;Diferencia;Motivo\r\n",
            "A-1;Café;3;4;1;SI;2,5;;Recuento mensual\r\n",
            "A-2;Agua;10;0;0;NO;;;\r\n",
        );
        let plan = parse_stock_count_import_csv(csv).unwrap();
        assert_eq!(plan.total_rows, 2);
        assert_eq!(plan.skipped_rows, 1);
        assert_eq!(plan.rows.len(), 1);
        assert!(plan.errors.is_empty());
        assert_eq!(plan.rows[0].line, 2);
        assert_eq!(plan.rows[0].sku.as_str(), "A-1");
        assert_eq!(plan.rows[0].counted_quantity, Decimal::new(25, 1));
        assert_eq!(plan.rows[0].reason, "Recuento mensual");
    }

    #[test]
    fn accepts_zero_and_uses_default_reason() {
        let plan = parse_stock_count_import_csv("codigo,conteo\nA-1,0\n").unwrap();
        assert_eq!(plan.rows.len(), 1);
        assert_eq!(plan.rows[0].counted_quantity, Decimal::ZERO);
        assert_eq!(plan.rows[0].reason, DEFAULT_REASON);
    }

    #[test]
    fn reports_each_invalid_count_row() {
        let csv = "codigo;conteo\n\
                   A-1;-1\n\
                   A-2;1,2345\n\
                   A-3;abc\n\
                   A-1;2\n\
                   ;3\n";
        let plan = parse_stock_count_import_csv(csv).unwrap();
        assert!(plan.rows.is_empty());
        assert_eq!(plan.errors.len(), 5);
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("negativa"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("decimales"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("no numérica"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("duplicado"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("inválido"))
        );
    }

    #[test]
    fn rejects_files_without_count_column_or_without_counts() {
        let missing = parse_stock_count_import_csv("codigo;articulo\nA-1;Café\n").unwrap_err();
        assert!(missing.to_string().contains("Conteo fisico"));
        let empty = parse_stock_count_import_csv("codigo;conteo\nA-1;\n").unwrap_err();
        assert!(empty.to_string().contains("no contiene conteos"));
    }
}
