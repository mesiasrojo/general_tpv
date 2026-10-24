use rust_decimal::Decimal;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_domain::ids::{PriceListId, ProductId};
use tpv_domain::pricing::{PriceList, PriceListItem, PriceListSnapshot};

use crate::error::StorageResult;
use crate::helpers::{parse_decimal, parse_uuid};
use crate::store::SqliteStore;

fn row_to_price_list(row: &SqliteRow) -> StorageResult<PriceList> {
    Ok(PriceList {
        id: PriceListId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        code: row.try_get("code")?,
        name: row.try_get("name")?,
        active: row.try_get::<i64, _>("active")? != 0,
    })
}

impl SqliteStore {
    pub(crate) async fn all_price_lists(&self) -> StorageResult<Vec<PriceListSnapshot>> {
        let rows = sqlx::query("SELECT * FROM price_lists ORDER BY name")
            .fetch_all(self.pool())
            .await?;
        let mut snapshots = Vec::with_capacity(rows.len());
        for row in rows {
            let price_list = row_to_price_list(&row)?;
            let items = self.load_price_list_items(price_list.id).await?;
            snapshots.push(PriceListSnapshot { price_list, items });
        }
        Ok(snapshots)
    }

    pub(crate) async fn get_price_list(
        &self,
        id: PriceListId,
    ) -> StorageResult<Option<PriceListSnapshot>> {
        let row = sqlx::query("SELECT * FROM price_lists WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let price_list = row_to_price_list(&row)?;
        let items = self.load_price_list_items(price_list.id).await?;
        Ok(Some(PriceListSnapshot { price_list, items }))
    }

    pub(crate) async fn get_price_list_by_code(
        &self,
        code: &str,
    ) -> StorageResult<Option<PriceList>> {
        let row = sqlx::query("SELECT * FROM price_lists WHERE code = ?")
            .bind(code)
            .fetch_optional(self.pool())
            .await?;
        row.as_ref().map(row_to_price_list).transpose()
    }

    async fn load_price_list_items(
        &self,
        price_list_id: PriceListId,
    ) -> StorageResult<Vec<PriceListItem>> {
        let rows = sqlx::query(
            "SELECT product_id, unit_price
             FROM price_list_items
             WHERE price_list_id = ?
             ORDER BY product_id",
        )
        .bind(price_list_id.to_string())
        .fetch_all(self.pool())
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(PriceListItem {
                    product_id: ProductId::from_uuid(parse_uuid(
                        &row.try_get::<String, _>("product_id")?,
                    )?),
                    unit_price: parse_decimal(&row.try_get::<String, _>("unit_price")?)?,
                })
            })
            .collect()
    }

    pub(crate) async fn persist_price_list(
        &self,
        snapshot: &PriceListSnapshot,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO price_lists (id, code, name, active)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                code = excluded.code,
                name = excluded.name,
                active = excluded.active",
        )
        .bind(snapshot.price_list.id.to_string())
        .bind(&snapshot.price_list.code)
        .bind(&snapshot.price_list.name)
        .bind(i64::from(snapshot.price_list.active))
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM price_list_items WHERE price_list_id = ?")
            .bind(snapshot.price_list.id.to_string())
            .execute(&mut *tx)
            .await?;
        for item in &snapshot.items {
            sqlx::query(
                "INSERT INTO price_list_items (price_list_id, product_id, unit_price)
                 VALUES (?, ?, ?)",
            )
            .bind(snapshot.price_list.id.to_string())
            .bind(item.product_id.to_string())
            .bind(item.unit_price.to_string())
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn get_price_list_price(
        &self,
        price_list_id: PriceListId,
        product_id: ProductId,
    ) -> StorageResult<Option<Decimal>> {
        let row = sqlx::query(
            "SELECT unit_price FROM price_list_items
             WHERE price_list_id = ? AND product_id = ?",
        )
        .bind(price_list_id.to_string())
        .bind(product_id.to_string())
        .fetch_optional(self.pool())
        .await?;
        row.map(|row| {
            let value = row.try_get::<String, _>("unit_price")?;
            parse_decimal(&value)
        })
        .transpose()
    }
}
