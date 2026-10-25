//! CSV exports for stock levels and their movement ledger.

use time::format_description::well_known::Rfc3339;
use tpv_domain::stock::{StockLevel, StockMovement, StockMovementKind};

use crate::dto::StockMovementView;

/// Builds a semicolon-separated CSV suitable for Spanish Excel. Quantities use
/// a decimal comma and the payload starts with a UTF-8 BOM so accents survive
/// a double click in Excel.
pub fn stock_levels_csv(levels: &[StockLevel]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str("Codigo;Articulo;Cantidad;Minimo;A reponer;Bajo minimo;Actualizado\r\n");
    for level in levels {
        let updated_at = level
            .updated_at
            .and_then(|value| value.format(&Rfc3339).ok())
            .unwrap_or_default();
        output.push_str(&csv_cell(level.sku.as_str()));
        output.push(';');
        output.push_str(&csv_cell(&level.name));
        output.push(';');
        output.push_str(&decimal_cell(level.quantity));
        output.push(';');
        output.push_str(&decimal_cell(level.min_quantity));
        output.push(';');
        output.push_str(&decimal_cell(level.restock_quantity()));
        output.push(';');
        output.push_str(if level.is_low() { "SI" } else { "NO" });
        output.push(';');
        output.push_str(&csv_cell(&updated_at));
        output.push_str("\r\n");
    }
    output
}

/// Printable worksheet for a physical inventory count. Registered quantities
/// are included as a reference, while the count, difference and reason columns
/// stay empty so staff can fill them during the count.
pub fn stock_count_sheet_csv(levels: &[StockLevel]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str(
        "Codigo;Articulo;Existencia registrada;Minimo;A reponer;Bajo minimo;Conteo fisico;Diferencia;Motivo\r\n",
    );
    for level in levels {
        output.push_str(&csv_cell(level.sku.as_str()));
        output.push(';');
        output.push_str(&csv_cell(&level.name));
        output.push(';');
        output.push_str(&decimal_cell(level.quantity));
        output.push(';');
        output.push_str(&decimal_cell(level.min_quantity));
        output.push(';');
        output.push_str(&decimal_cell(level.restock_quantity()));
        output.push(';');
        output.push_str(if level.is_low() { "SI" } else { "NO" });
        output.push(';');
        output.push_str(";;\r\n");
    }
    output
}

/// Builds a purchase order proposal with only the products that need stock.
/// Rows are defensive: a level without a pending restock is omitted even if a
/// caller passes a wider collection.
pub fn stock_replenishment_csv(levels: &[StockLevel]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str("Codigo;Articulo;Existencia;Minimo;Cantidad a pedir;Actualizado\r\n");
    for level in levels {
        let quantity = level.restock_quantity();
        if quantity <= rust_decimal::Decimal::ZERO {
            continue;
        }
        let updated_at = level
            .updated_at
            .and_then(|value| value.format(&Rfc3339).ok())
            .unwrap_or_default();
        output.push_str(&csv_cell(level.sku.as_str()));
        output.push(';');
        output.push_str(&csv_cell(&level.name));
        output.push(';');
        output.push_str(&decimal_cell(level.quantity));
        output.push(';');
        output.push_str(&decimal_cell(level.min_quantity));
        output.push(';');
        output.push_str(&decimal_cell(quantity));
        output.push(';');
        output.push_str(&csv_cell(&updated_at));
        output.push_str("\r\n");
    }
    output
}

/// Builds the auditable movement history of one product. The quantity is the
/// positive amount recorded in the ledger and `Efecto` carries its signed
/// effect on the level.
pub fn stock_movements_csv(level: &StockLevel, movements: &[StockMovement]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str("Codigo;Articulo;Tipo;Cantidad;Efecto;Motivo;Venta;Linea;Fecha\r\n");
    for movement in movements {
        let occurred_at = movement.occurred_at.format(&Rfc3339).unwrap_or_default();
        output.push_str(&csv_cell(level.sku.as_str()));
        output.push(';');
        output.push_str(&csv_cell(&level.name));
        output.push(';');
        output.push_str(movement_kind_label(movement.kind));
        output.push(';');
        output.push_str(&decimal_cell(movement.quantity.as_decimal()));
        output.push(';');
        output.push_str(&decimal_cell(movement.signed_quantity()));
        output.push(';');
        output.push_str(&csv_cell(movement.reason.as_deref().unwrap_or_default()));
        output.push(';');
        output.push_str(
            &movement
                .sale_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
        output.push(';');
        output.push_str(
            &movement
                .sale_line_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
        output.push(';');
        output.push_str(&csv_cell(&occurred_at));
        output.push_str("\r\n");
    }
    output
}

/// Builds the store-wide audit ledger, already enriched with SKU and name.
pub fn stock_movement_ledger_csv(records: &[StockMovementView]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str("Codigo;Articulo;Tipo;Cantidad;Efecto;Motivo;Venta;Linea;Fecha\r\n");
    for movement in records {
        let occurred_at = movement.occurred_at.format(&Rfc3339).unwrap_or_default();
        let signed = match movement.kind {
            StockMovementKind::Initial
            | StockMovementKind::Purchase
            | StockMovementKind::Return
            | StockMovementKind::AdjustmentIn
            | StockMovementKind::TransferIn => movement.quantity,
            StockMovementKind::Sale
            | StockMovementKind::AdjustmentOut
            | StockMovementKind::TransferOut => -movement.quantity,
        };
        output.push_str(&csv_cell(&movement.sku));
        output.push(';');
        output.push_str(&csv_cell(&movement.name));
        output.push(';');
        output.push_str(movement_kind_label(movement.kind));
        output.push(';');
        output.push_str(&decimal_cell(movement.quantity));
        output.push(';');
        output.push_str(&decimal_cell(signed));
        output.push(';');
        output.push_str(&csv_cell(movement.reason.as_deref().unwrap_or_default()));
        output.push(';');
        output.push_str(
            &movement
                .sale_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
        output.push(';');
        output.push_str(
            &movement
                .sale_line_id
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );
        output.push(';');
        output.push_str(&csv_cell(&occurred_at));
        output.push_str("\r\n");
    }
    output
}

fn movement_kind_label(kind: StockMovementKind) -> &'static str {
    match kind {
        StockMovementKind::Initial => "Stock inicial",
        StockMovementKind::Purchase => "Entrada",
        StockMovementKind::Sale => "Venta",
        StockMovementKind::Return => "Devolución",
        StockMovementKind::AdjustmentIn => "Ajuste positivo",
        StockMovementKind::AdjustmentOut => "Ajuste negativo",
        StockMovementKind::TransferOut => "Transferencia enviada",
        StockMovementKind::TransferIn => "Transferencia recibida",
    }
}

fn decimal_cell(value: rust_decimal::Decimal) -> String {
    value.to_string().replace('.', ",")
}

fn csv_cell(value: &str) -> String {
    // Spreadsheet applications may evaluate text cells beginning with these
    // characters as formulas. A leading apostrophe keeps the content literal.
    let first = value.trim_start().chars().next();
    let guarded = if matches!(first, Some('=' | '+' | '-' | '@' | '\t' | '\r')) {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if guarded.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;
    use time::OffsetDateTime;
    use tpv_domain::Sku;
    use tpv_domain::ids::{ProductId, StockMovementId, StoreId};
    use tpv_domain::money::Quantity;

    use super::*;

    #[test]
    fn exports_spanish_csv_and_escapes_cells() {
        let level = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A;1").unwrap(),
            name: "Café \"especial\"".into(),
            quantity: Decimal::new(35, 1),
            min_quantity: Decimal::new(5, 0),
            updated_at: None,
        };
        let csv = stock_levels_csv(&[level]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(
            csv.contains("Codigo;Articulo;Cantidad;Minimo;A reponer;Bajo minimo;Actualizado\r\n")
        );
        assert!(csv.contains("\"A;1\";\"Café \"\"especial\"\"\";3,5;5;1,5;SI;\r\n"));
    }

    #[test]
    fn exports_movement_history_with_signed_effect() {
        let level = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A-1").unwrap(),
            name: "Producto".into(),
            quantity: Decimal::new(25, 1),
            min_quantity: Decimal::ZERO,
            updated_at: None,
        };
        let movement = StockMovement::new(
            StockMovementId::new(),
            level.store_id,
            level.product_id,
            StockMovementKind::Purchase,
            Quantity::new(Decimal::new(25, 1)).unwrap(),
            Some("Compra; proveedor".into()),
            None,
            None,
            OffsetDateTime::UNIX_EPOCH,
        )
        .unwrap();
        let csv = stock_movements_csv(&level, &[movement]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Codigo;Articulo;Tipo;Cantidad;Efecto;Motivo;Venta;Linea;Fecha\r\n"));
        assert!(csv.contains(
            "A-1;Producto;Entrada;2,5;2,5;\"Compra; proveedor\";;;1970-01-01T00:00:00Z\r\n"
        ));
    }

    #[test]
    fn exports_the_store_wide_ledger_with_product_data() {
        let record = StockMovementView {
            id: StockMovementId::new(),
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: "A-1".into(),
            name: "Producto".into(),
            kind: StockMovementKind::AdjustmentOut,
            quantity: Decimal::new(25, 1),
            reason: Some("Merma".into()),
            sale_id: None,
            sale_line_id: None,
            occurred_at: OffsetDateTime::UNIX_EPOCH,
        };
        let csv = stock_movement_ledger_csv(&[record]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Codigo;Articulo;Tipo;Cantidad;Efecto;Motivo;Venta;Linea;Fecha\r\n"));
        assert!(
            csv.contains("A-1;Producto;Ajuste negativo;2,5;-2,5;Merma;;;1970-01-01T00:00:00Z\r\n")
        );
    }

    #[test]
    fn exports_no_when_stock_is_above_the_minimum() {
        let level = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A-1").unwrap(),
            name: "Producto".into(),
            quantity: Decimal::new(5, 0),
            min_quantity: Decimal::new(35, 1),
            updated_at: None,
        };
        let csv = stock_levels_csv(&[level]);
        assert!(csv.contains("A-1;Producto;5;3,5;0;NO;\r\n"));
    }

    #[test]
    fn csv_cells_cannot_inject_spreadsheet_formulas() {
        assert_eq!(csv_cell("=SUM(A1:A2)"), "'=SUM(A1:A2)");
        assert_eq!(csv_cell("@cmd"), "'@cmd");
        assert_eq!(csv_cell("-A-1"), "'-A-1");
        assert_eq!(csv_cell("  +cmd"), "'  +cmd");
        assert_eq!(csv_cell("A;B"), "\"A;B\"");
        assert_eq!(csv_cell("Producto normal"), "Producto normal");
    }

    #[test]
    fn purchase_order_contains_only_pending_products() {
        let pending = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A-1").unwrap(),
            name: "Producto".into(),
            quantity: Decimal::new(15, 1),
            min_quantity: Decimal::new(4, 0),
            updated_at: None,
        };
        let healthy = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A-2").unwrap(),
            name: "Sobrado".into(),
            quantity: Decimal::new(6, 0),
            min_quantity: Decimal::new(4, 0),
            updated_at: None,
        };
        let csv = stock_replenishment_csv(&[pending, healthy]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Codigo;Articulo;Existencia;Minimo;Cantidad a pedir;Actualizado\r\n"));
        assert!(csv.contains("A-1;Producto;1,5;4;2,5;\r\n"));
        assert!(!csv.contains("A-2"));
    }

    #[test]
    fn count_sheet_leaves_manual_columns_empty() {
        let level = StockLevel {
            store_id: StoreId::new(),
            product_id: ProductId::new(),
            sku: Sku::new("A-1").unwrap(),
            name: "Producto".into(),
            quantity: Decimal::new(15, 1),
            min_quantity: Decimal::new(4, 0),
            updated_at: None,
        };
        let csv = stock_count_sheet_csv(&[level]);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains(
            "Codigo;Articulo;Existencia registrada;Minimo;A reponer;Bajo minimo;Conteo fisico;Diferencia;Motivo\r\n"
        ));
        assert!(csv.contains("A-1;Producto;1,5;4;2,5;SI;;;\r\n"));
    }
}
