//! Product catalogue migration from CSV files.
//!
//! Parsing is pure and deterministic: the adapter reads the file, the
//! application validates every row and the storage port applies the batch
//! atomically. Rows with errors never reach the commit phase.

use std::collections::HashSet;
use std::str::FromStr;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tpv_domain::catalog::Product;
use tpv_domain::ids::Sku;
use tpv_domain::tax::TaxRate;

use crate::error::{ApplicationError, Result};

/// One validated CSV data row.
#[derive(Debug, Clone)]
pub struct ProductImportRow {
    /// Physical line in the source file, used in error reports.
    pub line: usize,
    pub source_id: String,
    pub product: Product,
}

/// A row that could not be imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductImportError {
    pub line: usize,
    pub source_id: Option<String>,
    pub message: String,
}

/// Fully parsed batch, ready to preview or commit.
#[derive(Debug, Clone)]
pub struct ProductImportPlan {
    pub source_system: String,
    pub batch_id: String,
    pub total_rows: usize,
    pub rows: Vec<ProductImportRow>,
    pub errors: Vec<ProductImportError>,
}

impl ProductImportPlan {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty() && !self.rows.is_empty()
    }
}

/// Classification returned by the storage adapter.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProductImportPreview {
    pub already_imported: bool,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
}

/// Stable, human-readable report shared by the CLI and the desktop UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductImportReport {
    pub source_system: String,
    pub batch_id: String,
    pub dry_run: bool,
    pub committed: bool,
    pub already_imported: bool,
    pub total_rows: usize,
    pub valid_rows: usize,
    pub invalid_rows: usize,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub errors: Vec<ProductImportError>,
}

impl ProductImportReport {
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.invalid_rows > 0
    }

    /// True when every valid row has been inserted, updated or confirmed as
    /// unchanged. This is the reconciliation invariant requested by the plan.
    #[must_use]
    pub fn reconciled(&self) -> bool {
        self.inserted + self.updated + self.unchanged == self.valid_rows
    }
}

const SOURCE_ID_HEADERS: &[&str] = &["source_id", "id_origen", "idorigen", "id"];
const SKU_HEADERS: &[&str] = &[
    "sku",
    "codigo",
    "codigo_articulo",
    "referencia",
    "ref",
    "articulo_id",
];
const BARCODE_HEADERS: &[&str] = &["barcode", "codigo_barras", "codigobarras", "ean", "ean13"];
const NAME_HEADERS: &[&str] = &[
    "name",
    "nombre",
    "descripcion",
    "denominacion",
    "articulo",
    "nombre_articulo",
];
const FAMILY_HEADERS: &[&str] = &[
    "family_code",
    "familia",
    "codigo_familia",
    "familia_codigo",
    "cod_familia",
];
const PRICE_HEADERS: &[&str] = &[
    "unit_price",
    "precio",
    "precio_venta",
    "pvp",
    "importe",
    "precio_unitario",
];
const TAX_CODE_HEADERS: &[&str] = &["tax_code", "codigo_iva", "iva_codigo", "codigo_fiscal"];
const TAX_BP_HEADERS: &[&str] = &[
    "tax_percent_bp",
    "iva_bp",
    "tipo_iva_bp",
    "porcentaje_iva_bp",
];
const TAX_PERCENT_HEADERS: &[&str] = &[
    "tax_percent",
    "iva",
    "tipo_iva",
    "porcentaje_iva",
    "tipo_iva_porcentaje",
];
const ACTIVE_HEADERS: &[&str] = &["active", "activo", "estado", "alta", "baja"];

/// Parses and validates a product CSV.
///
/// Accepted formats:
/// - UTF-8, optional BOM, comma/semicolon/tab/pipe delimiter.
/// - `unit_price` may use a decimal comma (`12,50`).
/// - Tax can be supplied as basis points (`2100`) or percent (`21`).
/// - Required columns: source id, SKU, name, price and one tax column.
pub fn parse_product_import_csv(
    source_system: &str,
    batch_id: Option<&str>,
    content: &str,
) -> Result<ProductImportPlan> {
    let source_system = source_system.trim();
    if source_system.is_empty() {
        return Err(ApplicationError::Validation(
            "el sistema de origen es obligatorio".into(),
        ));
    }
    if source_system.len() > 64 {
        return Err(ApplicationError::Validation(
            "el sistema de origen no puede superar 64 caracteres".into(),
        ));
    }
    let content = content.trim_start_matches('\u{feff}');
    let batch_id = match batch_id.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value.len() > 128 => {
            return Err(ApplicationError::Validation(
                "el identificador de lote no puede superar 128 caracteres".into(),
            ));
        }
        Some(value) => value.to_string(),
        None => default_batch_id(content),
    };

    if content.trim().is_empty() {
        return Err(ApplicationError::Validation(
            "el fichero CSV está vacío".into(),
        ));
    }

    let delimiter = detect_delimiter(content);
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

    let source_id_idx = required_column(&headers, SOURCE_ID_HEADERS, "source_id")?;
    let sku_idx = required_column(&headers, SKU_HEADERS, "sku")?;
    let name_idx = required_column(&headers, NAME_HEADERS, "nombre")?;
    let price_idx = required_column(&headers, PRICE_HEADERS, "precio")?;
    let tax_bp_idx = find_column(&headers, TAX_BP_HEADERS);
    let tax_percent_idx = find_column(&headers, TAX_PERCENT_HEADERS);
    if tax_bp_idx.is_none() && tax_percent_idx.is_none() {
        return Err(ApplicationError::Validation(
            "falta la columna de IVA (tax_percent_bp o tax_percent)".into(),
        ));
    }
    let barcode_idx = find_column(&headers, BARCODE_HEADERS);
    let family_idx = find_column(&headers, FAMILY_HEADERS);
    let tax_code_idx = find_column(&headers, TAX_CODE_HEADERS);
    let active_idx = find_column(&headers, ACTIVE_HEADERS);

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    let mut total_rows = 0_usize;
    let mut seen_source_ids = HashSet::new();
    let mut seen_skus = HashSet::new();

    for record in reader.records() {
        total_rows += 1;
        let record = match record {
            Ok(record) => record,
            Err(error) => {
                let line = error
                    .position()
                    .map_or(total_rows + 1, |position| position.line() as usize);
                errors.push(ProductImportError {
                    line,
                    source_id: None,
                    message: format!("fila CSV inválida: {error}"),
                });
                continue;
            }
        };
        let line = record
            .position()
            .map_or(total_rows + 1, |position| position.line() as usize);
        let source_id = value(&record, source_id_idx).to_string();
        match parse_row(
            line,
            &source_id,
            &record,
            sku_idx,
            name_idx,
            price_idx,
            tax_bp_idx,
            tax_percent_idx,
            tax_code_idx,
            barcode_idx,
            family_idx,
            active_idx,
        ) {
            Ok(row) => {
                if !seen_source_ids.insert(row.source_id.clone()) {
                    errors.push(ProductImportError {
                        line,
                        source_id: Some(row.source_id),
                        message: "source_id duplicado en el fichero".into(),
                    });
                    continue;
                }
                if !seen_skus.insert(row.product.sku.as_str().to_string()) {
                    errors.push(ProductImportError {
                        line,
                        source_id: Some(row.source_id),
                        message: format!("SKU duplicado en el fichero: {}", row.product.sku),
                    });
                    continue;
                }
                rows.push(row);
            }
            Err(message) => errors.push(ProductImportError {
                line,
                source_id: (!source_id.trim().is_empty()).then(|| source_id.trim().to_string()),
                message,
            }),
        }
    }

    if total_rows == 0 {
        return Err(ApplicationError::Validation(
            "el CSV no contiene filas de datos".into(),
        ));
    }

    Ok(ProductImportPlan {
        source_system: source_system.to_string(),
        batch_id,
        total_rows,
        rows,
        errors,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_row(
    line: usize,
    source_id: &str,
    record: &csv::StringRecord,
    sku_idx: usize,
    name_idx: usize,
    price_idx: usize,
    tax_bp_idx: Option<usize>,
    tax_percent_idx: Option<usize>,
    tax_code_idx: Option<usize>,
    barcode_idx: Option<usize>,
    family_idx: Option<usize>,
    active_idx: Option<usize>,
) -> std::result::Result<ProductImportRow, String> {
    let source_id = source_id.trim();
    if source_id.is_empty() {
        return Err("source_id es obligatorio".into());
    }
    if source_id.len() > 128 {
        return Err("source_id no puede superar 128 caracteres".into());
    }
    let sku = Sku::new(value(record, sku_idx)).map_err(|error| format!("SKU inválido: {error}"))?;
    let name = value(record, name_idx).trim();
    if name.is_empty() {
        return Err("el nombre es obligatorio".into());
    }
    let unit_price = parse_decimal(value(record, price_idx))
        .map_err(|error| format!("precio inválido: {error}"))?;
    let percent_bp = parse_tax_bp(
        tax_bp_idx.map(|index| value(record, index)),
        tax_percent_idx.map(|index| value(record, index)),
    )?;
    let tax_code = tax_code_idx
        .map(|index| value(record, index).trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("IVA");
    let tax_rate =
        TaxRate::new(tax_code, percent_bp).map_err(|error| format!("IVA inválido: {error}"))?;
    let mut product = Product::new(
        tpv_domain::ids::ProductId::new(),
        sku,
        name,
        unit_price,
        tax_rate,
    )
    .map_err(|error| format!("producto inválido: {error}"))?;
    if let Some(index) = barcode_idx {
        let barcode = value(record, index).trim();
        if !barcode.is_empty() {
            product
                .set_barcode(barcode)
                .map_err(|error| format!("código de barras inválido: {error}"))?;
        }
    }
    if let Some(index) = family_idx {
        let family = value(record, index).trim();
        if !family.is_empty() {
            product.family_code = Some(family.to_string());
        }
    }
    if let Some(index) = active_idx {
        product.active = parse_active(value(record, index))?;
    }
    Ok(ProductImportRow {
        line,
        source_id: source_id.to_string(),
        product,
    })
}

fn parse_decimal(raw: &str) -> std::result::Result<Decimal, String> {
    let normalized = raw.trim().replace(',', ".");
    if normalized.is_empty() {
        return Err("valor vacío".into());
    }
    Decimal::from_str(&normalized).map_err(|_| format!("valor no numérico: {raw}"))
}

fn parse_tax_bp(
    basis_points: Option<&str>,
    percent: Option<&str>,
) -> std::result::Result<u32, String> {
    if let Some(raw) = basis_points
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let decimal = parse_decimal(raw)?;
        if decimal < Decimal::ZERO || decimal.trunc() != decimal {
            return Err(format!(
                "tax_percent_bp debe ser un entero no negativo: {raw}"
            ));
        }
        return decimal
            .to_i64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| format!("tax_percent_bp fuera de rango: {raw}"));
    }
    let Some(raw) = percent.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err("falta el IVA (tax_percent_bp o tax_percent)".into());
    };
    let decimal = parse_decimal(raw)?;
    if decimal < Decimal::ZERO {
        return Err(format!("el IVA no puede ser negativo: {raw}"));
    }
    // Values up to 100 are read as percentages; larger values are assumed to
    // already be basis points. This keeps `21` and `2100` equivalent.
    let basis_points = if decimal <= Decimal::from(100) {
        decimal * Decimal::from(100)
    } else {
        decimal
    };
    if basis_points.trunc() != basis_points {
        return Err(format!(
            "el IVA debe ser un entero en puntos básicos: {raw}"
        ));
    }
    basis_points
        .to_i64()
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| format!("IVA fuera de rango: {raw}"))
}

fn parse_active(raw: &str) -> std::result::Result<bool, String> {
    match raw.trim().to_lowercase().as_str() {
        "" | "1" | "true" | "si" | "sí" | "yes" | "activo" | "alta" => Ok(true),
        "0" | "false" | "no" | "inactivo" | "baja" => Ok(false),
        other => Err(format!("valor de activo no reconocido: {other}")),
    }
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

fn default_batch_id(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    hex::encode(&digest[..8])
}

fn csv_error(error: &csv::Error) -> ApplicationError {
    ApplicationError::Validation(format!("CSV inválido: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_semicolon_csv_with_bom_and_decimal_comma() {
        let csv = "\u{feff}id_origen;codigo;nombre;precio;iva;familia;activo\n\
                   A-1;SKU-1;Café solo;1,20;21;BEBIDAS;1\n\
                   A-2;SKU-2;Agua;0,50;1000;;0\n";
        let plan = parse_product_import_csv("micro_server", None, csv).unwrap();
        assert_eq!(plan.total_rows, 2);
        assert!(plan.errors.is_empty());
        assert_eq!(plan.rows.len(), 2);
        assert_eq!(plan.batch_id.len(), 16);
        let first = &plan.rows[0];
        assert_eq!(first.source_id, "A-1");
        assert_eq!(first.product.sku.as_str(), "SKU-1");
        assert_eq!(first.product.name, "Café solo");
        assert_eq!(first.product.unit_price, Decimal::new(120, 2));
        assert_eq!(first.product.tax_rate.percent_bp, 2100);
        assert_eq!(first.product.family_code.as_deref(), Some("BEBIDAS"));
        assert!(first.product.active);
        assert!(!plan.rows[1].product.active);
        assert_eq!(plan.rows[1].product.tax_rate.percent_bp, 1000);
        assert_eq!(plan.rows.len() + plan.errors.len(), plan.total_rows);
    }

    #[test]
    fn reports_each_invalid_row_and_keeps_valid_ones() {
        let csv = "source_id,sku,name,unit_price,tax_percent_bp\n\
                   A-1,SKU-1,Producto,10.00,2100\n\
                   ,SKU-2,Sin origen,10.00,2100\n\
                   A-3,SKU-3,,\n\
                   A-4,SKU-4,Producto,abc,2100\n\
                   A-5,SKU-5,Producto,10.00,10001\n\
                   A-6,SKU-1,Duplicado,10.00,2100\n";
        let plan = parse_product_import_csv("micro_server", Some("lote-1"), csv).unwrap();
        assert_eq!(plan.batch_id, "lote-1");
        assert_eq!(plan.rows.len(), 1);
        assert_eq!(plan.errors.len(), 5);
        assert_eq!(plan.total_rows, 6);
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("source_id"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("nombre"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("precio"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("IVA"))
        );
        assert!(
            plan.errors
                .iter()
                .any(|error| error.message.contains("duplicado"))
        );
        assert!(!plan.is_valid());
    }

    #[test]
    fn rejects_missing_required_headers() {
        let error =
            parse_product_import_csv("micro_server", None, "sku,name\nA,Producto\n").unwrap_err();
        assert!(error.to_string().contains("source_id"));
    }

    #[test]
    fn accepts_tax_as_basis_points_or_percent() {
        let csv = "source_id,sku,name,unit_price,tax_percent\n\
                   A-1,SKU-1,Producto,10.00,21\n\
                   A-2,SKU-2,Producto,10.00,2100\n";
        let plan = parse_product_import_csv("micro_server", None, csv).unwrap();
        assert!(plan.errors.is_empty());
        assert!(
            plan.rows
                .iter()
                .all(|row| row.product.tax_rate.percent_bp == 2100)
        );
    }

    #[test]
    fn batch_id_is_deterministic_for_identical_content() {
        let csv = "source_id,sku,name,unit_price,tax_percent_bp\nA,SKU,Producto,1.00,2100\n";
        let first = parse_product_import_csv("micro_server", None, csv).unwrap();
        let second = parse_product_import_csv("micro_server", None, csv).unwrap();
        assert_eq!(first.batch_id, second.batch_id);
        let changed =
            parse_product_import_csv("micro_server", None, &csv.replace("1.00", "1.01")).unwrap();
        assert_ne!(first.batch_id, changed.batch_id);
    }
}
