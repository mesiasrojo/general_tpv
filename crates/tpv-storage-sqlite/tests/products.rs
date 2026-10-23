use std::sync::Arc;

use tpv_application::{
    CatalogRepository, MasterDataRepository, MasterDataService, ProductService,
    SyncOutboxRepository, UpsertProductRequest,
};
use tpv_domain::master_data::{MasterDataChange, MasterDataKind};
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

#[tokio::test]
async fn product_upsert_publishes_and_remote_change_updates_catalog() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let master_data_repo: Arc<dyn MasterDataRepository> = store.clone();
    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let master_data =
        MasterDataService::new(master_data_repo, Arc::new(tpv_application::SystemClock));
    let products = ProductService::new(catalog_repo.clone(), master_data.clone());
    let pending_before = sync_repo.sync_status().await.unwrap().pending;

    let created = products
        .upsert(
            &context,
            demo.store_id,
            UpsertProductRequest {
                product_id: None,
                sku: "TEST-UI-1".into(),
                barcode: Some("8412345678901".into()),
                name: "Producto UI".into(),
                family_code: Some("TEST".into()),
                unit_price: "12.50".into(),
                tax_code: "IVA21".into(),
                tax_percent_bp: 2100,
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(created.name, "Producto UI");
    assert_eq!(created.unit_price.to_string(), "12.50");
    assert_eq!(
        sync_repo.sync_status().await.unwrap().pending,
        pending_before + 1
    );
    assert!(
        catalog_repo
            .list_products()
            .await
            .unwrap()
            .iter()
            .any(|product| product.id == created.id)
    );

    let mut remote = created.clone();
    remote.name = "Producto remoto".into();
    remote.unit_price = "13.75".parse().unwrap();
    let change = MasterDataChange::new(
        MasterDataKind::Product,
        created.id.as_uuid(),
        2,
        tpv_domain::ids::DeviceId::new(),
        time::OffsetDateTime::now_utc(),
        serde_json::to_value(&remote).unwrap(),
        false,
    )
    .unwrap();
    let result = master_data
        .apply_change(context.tenant_id, demo.store_id, &change)
        .await
        .unwrap();
    assert!(result.applied);
    let stored = catalog_repo
        .list_products()
        .await
        .unwrap()
        .into_iter()
        .find(|product| product.id == created.id)
        .unwrap();
    assert_eq!(stored.name, "Producto remoto");
    assert_eq!(stored.unit_price.to_string(), "13.75");
}
