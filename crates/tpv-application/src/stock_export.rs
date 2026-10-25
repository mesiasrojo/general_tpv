//! CSV exports for stock levels and their movement ledger.

use time::format_description::well_known::Rfc3339;
use tpv_domain::stock::{StockLevel, StockMovement, StockMovementKind};

use crate::dto::StockMovementView;

/// Builds a semicolon-separated CSV suitable for Spanish Excel. Quantities use
/// a decimal comma and the payload starts with a UTF-8 BOM so accents survive
/// a double click in Excel.
pub fn stock_levels_csv(levels: &[StockLevel]) -> String {
    let mut output = String::from('\u{feff}');
    output.push_str("Codigo;Articulo;Cantidad;Minimo;Bajo minimo;Actualizado\r\n");
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
        output.push_str(if level.is_low() { "SI" } else { "NO" });
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
            | StockMovementKind::AdjustmentIn => movement.quantity,
            StockMovementKind::Sale | StockMovementKind::AdjustmentOut => -movement.quantity,
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
    }
}

fn decimal_cell(value: rust_decimal::Decimal) -> String {
    value.to_string().replace('.', ",")
}

fn csv_cell(value: &str) -> String {
    if value.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
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
        assert!(csv.contains("Codigo;Articulo;Cantidad;Minimo;Bajo minimo;Actualizado\r\n"));
        assert!(csv.contains("\"A;1\";\"Café \"\"especial\"\"\";3,5;5;SI;\r\n"));
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
        assert!(csv.contains("A-1;Producto;5;3,5;NO;\r\n"));
    }
}
