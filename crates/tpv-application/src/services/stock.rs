use std::str::FromStr;
use std::sync::Arc;

use rust_decimal::Decimal;
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::ids::{ProductId, StockMovementId, StoreId, UserId};
use tpv_domain::money::Quantity;
use tpv_domain::stock::{StockLevel, StockMovement};

use crate::dto::{
    QueryStockMovementsRequest, RecordStockMovementRequest, SetStockCountRequest,
    SetStockMinimumRequest, StockCountCommand, StockCountResult, StockMovementPage,
    StockMovementView,
};
use crate::error::{ApplicationError, Result};
use crate::ports::{CatalogRepository, Clock, StockMovementFilter, StockRepository};

/// Coordinates stock levels and their auditable movement ledger.
#[derive(Clone)]
pub struct StockService {
    stock: Arc<dyn StockRepository>,
    catalog: Arc<dyn CatalogRepository>,
    clock: Arc<dyn Clock>,
}

impl StockService {
    pub fn new(
        stock: Arc<dyn StockRepository>,
        catalog: Arc<dyn CatalogRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            stock,
            catalog,
            clock,
        }
    }

    pub async fn list_levels(&self, store_id: StoreId) -> Result<Vec<StockLevel>> {
        self.stock.list_levels(store_id).await
    }

    pub async fn count_low_stock(&self, store_id: StoreId) -> Result<u64> {
        self.stock.count_low_stock(store_id).await
    }

    pub async fn levels_for_products(
        &self,
        store_id: StoreId,
        product_ids: &[ProductId],
    ) -> Result<Vec<StockLevel>> {
        let capped = &product_ids[..product_ids.len().min(100)];
        self.stock.list_levels_for_products(store_id, capped).await
    }

    pub async fn get_level(
        &self,
        store_id: StoreId,
        product_id: ProductId,
    ) -> Result<Option<StockLevel>> {
        self.stock.get_level(store_id, product_id).await
    }

    pub async fn list_movements(
        &self,
        store_id: StoreId,
        product_id: ProductId,
        limit: u32,
    ) -> Result<Vec<StockMovement>> {
        self.stock
            .list_movements(store_id, product_id, limit.clamp(1, 500))
            .await
    }

    /// Full ledger used by manager CSV exports. The on-screen history keeps a
    /// smaller limit, while an audit export must not silently drop old rows.
    pub async fn list_movements_for_export(
        &self,
        store_id: StoreId,
        product_id: ProductId,
    ) -> Result<Vec<StockMovement>> {
        self.stock
            .list_movements(store_id, product_id, 50_000)
            .await
    }

    /// Store-wide paginated ledger for the audit screen.
    pub async fn query_movements(
        &self,
        store_id: StoreId,
        request: QueryStockMovementsRequest,
    ) -> Result<StockMovementPage> {
        self.query_movements_page(store_id, request, 1, 200).await
    }

    /// Same filters as the on-screen ledger, without pagination, for exports.
    pub async fn movements_for_ledger_export(
        &self,
        store_id: StoreId,
        request: QueryStockMovementsRequest,
    ) -> Result<Vec<StockMovementView>> {
        let page = self
            .query_movements_page(store_id, request, 50_000, 50_000)
            .await?;
        Ok(page.items)
    }

    async fn query_movements_page(
        &self,
        store_id: StoreId,
        request: QueryStockMovementsRequest,
        min_limit: u32,
        max_limit: u32,
    ) -> Result<StockMovementPage> {
        let search = request
            .search
            .unwrap_or_default()
            .trim()
            .chars()
            .take(100)
            .collect::<String>();
        let limit = if request.limit == 0 {
            min_limit.max(100).min(max_limit)
        } else {
            request.limit.clamp(min_limit, max_limit)
        };
        let filter = StockMovementFilter {
            store_id,
            search: (!search.is_empty()).then_some(search),
            product_id: request.product_id,
            kind: request.kind,
            limit,
            offset: request.offset,
        };
        self.stock.query_movements(&filter).await
    }

    /// Sets the stock level to a physically counted quantity. A positive or
    /// negative adjustment movement is created only when the count differs,
    /// while every count remains auditable.
    pub async fn set_count(
        &self,
        store_id: StoreId,
        actor_id: Option<UserId>,
        request: SetStockCountRequest,
    ) -> Result<StockCountResult> {
        let product = self.find_product(request.product_id).await?;
        let counted = parse_decimal(&request.counted_quantity, "counted quantity")?;
        if counted < Decimal::ZERO {
            return Err(ApplicationError::Validation(
                "counted stock cannot be negative".into(),
            ));
        }
        if counted.scale() > 3 {
            return Err(ApplicationError::Validation(
                "counted stock supports at most three decimals".into(),
            ));
        }
        let reason = request.reason.trim().to_string();
        if reason.is_empty() {
            return Err(ApplicationError::Validation(
                "stock count reason is required".into(),
            ));
        }
        let level = self.get_level(store_id, product.id).await?.ok_or_else(|| {
            ApplicationError::NotFound(format!("stock level {product_id}", product_id = product.id))
        })?;
        let previous = level.quantity;
        let counted = counted.normalize();
        let difference = counted - previous;
        let now = self.clock.now();
        let command = StockCountCommand {
            store_id,
            product_id: product.id,
            counted_quantity: counted,
            movement_id: StockMovementId::new(),
            reason: reason.clone(),
            occurred_at: now,
        };
        let audit = AuditEvent::new(
            now,
            actor_id,
            Some(store_id),
            AuditAction::StockCount,
            "stock_level",
            product.id.to_string(),
            serde_json::json!({
                "product_id": product.id,
                "sku": product.sku.as_str(),
                "previous_quantity": previous.normalize().to_string(),
                "counted_quantity": counted.to_string(),
                "difference": difference.normalize().to_string(),
                "reason": reason,
            }),
        )?;
        self.stock.set_count(&command, &audit).await
    }

    pub async fn record_movement(
        &self,
        store_id: StoreId,
        actor_id: Option<UserId>,
        request: RecordStockMovementRequest,
    ) -> Result<StockMovement> {
        if !request.kind.is_manual() {
            return Err(ApplicationError::Validation(
                "sales and returns are recorded by the sales flow".into(),
            ));
        }
        let product = self.find_product(request.product_id).await?;
        let quantity = Quantity::new(parse_decimal(&request.quantity, "quantity")?)?;
        let now = self.clock.now();
        let movement = StockMovement::new(
            StockMovementId::new(),
            store_id,
            product.id,
            request.kind,
            quantity,
            request.reason,
            None,
            None,
            now,
        )?;
        let audit = AuditEvent::new(
            now,
            actor_id,
            Some(store_id),
            AuditAction::StockMovement,
            "stock_movement",
            movement.id.to_string(),
            serde_json::json!({
                "product_id": product.id,
                "sku": product.sku.as_str(),
                "kind": movement.kind,
                "quantity": movement.quantity.to_string(),
                "signed_quantity": movement.signed_quantity().to_string(),
                "reason": movement.reason,
            }),
        )?;
        self.stock.record_movement(&movement, &audit).await?;
        Ok(movement)
    }

    pub async fn set_minimum(
        &self,
        store_id: StoreId,
        actor_id: Option<UserId>,
        request: SetStockMinimumRequest,
    ) -> Result<StockLevel> {
        let product = self.find_product(request.product_id).await?;
        let minimum = parse_decimal(&request.min_quantity, "minimum stock")?;
        if minimum < Decimal::ZERO {
            return Err(ApplicationError::Validation(
                "minimum stock cannot be negative".into(),
            ));
        }
        if minimum.scale() > 3 {
            return Err(ApplicationError::Validation(
                "minimum stock supports at most three decimals".into(),
            ));
        }
        let now = self.clock.now();
        let audit = AuditEvent::new(
            now,
            actor_id,
            Some(store_id),
            AuditAction::StockMinimumChanged,
            "stock_level",
            product.id.to_string(),
            serde_json::json!({
                "product_id": product.id,
                "sku": product.sku.as_str(),
                "min_quantity": minimum.normalize().to_string(),
            }),
        )?;
        self.stock
            .set_minimum(store_id, product.id, minimum.normalize(), &audit)
            .await
    }

    async fn find_product(&self, product_id: ProductId) -> Result<tpv_domain::catalog::Product> {
        self.catalog
            .find_product_by_id(product_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("product {product_id}")))
    }
}

fn parse_decimal(value: &str, field: &str) -> Result<Decimal> {
    let normalized = value.trim().replace(',', ".");
    if normalized.is_empty() {
        return Err(ApplicationError::Validation(format!("{field} is required")));
    }
    Decimal::from_str(&normalized)
        .map_err(|_| ApplicationError::Validation(format!("invalid {field}: {value}")))
}
