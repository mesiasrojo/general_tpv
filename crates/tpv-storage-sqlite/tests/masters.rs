use std::sync::Arc;

use tpv_application::{
    ApplicationError, CatalogRepository, FamilyService, MasterDataRepository, MasterDataService,
    SellerService, StoreService, SyncOutboxRepository, TaxService, UpsertFamilyRequest,
    UpsertSellerRequest, UpsertStoreRequest, UpsertTaxRequest,
};
use tpv_domain::master_data::{MasterDataChange, MasterDataKind};
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

#[tokio::test]
async fn family_and_tax_upserts_persist_and_publish_master_changes() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let master_data_repo: Arc<dyn MasterDataRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let master_data =
        MasterDataService::new(master_data_repo, Arc::new(tpv_application::SystemClock));
    let families = FamilyService::new(catalog_repo.clone(), master_data.clone());
    let taxes = TaxService::new(catalog_repo.clone(), master_data.clone());
    let pending_before = sync_repo.sync_status().await.unwrap().pending;

    let family = families
        .upsert(
            &context,
            demo.store_id,
            UpsertFamilyRequest {
                family_id: None,
                code: "  bebidas  ".into(),
                name: "Bebidas".into(),
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(family.code, "BEBIDAS");
    assert!(family.active);

    let tax = taxes
        .upsert(
            &context,
            demo.store_id,
            UpsertTaxRequest {
                tax_id: None,
                code: "iva5".into(),
                name: "IVA reducido".into(),
                percent_bp: 500,
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(tax.code, "IVA5");
    assert_eq!(tax.percent_bp, 500);

    assert_eq!(
        sync_repo.sync_status().await.unwrap().pending,
        pending_before + 2
    );
    assert!(
        catalog_repo
            .find_family_by_code("BEBIDAS")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        catalog_repo
            .find_tax_by_code("IVA5")
            .await
            .unwrap()
            .is_some()
    );

    let conflict = taxes
        .upsert(
            &context,
            demo.store_id,
            UpsertTaxRequest {
                tax_id: None,
                code: "IVA5".into(),
                name: "Duplicado".into(),
                percent_bp: 500,
                active: true,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(conflict, ApplicationError::Conflict(_)));

    let mut remote = family.clone();
    remote.name = "Bebidas remotas".into();
    let change = MasterDataChange::new(
        MasterDataKind::Family,
        family.id.as_uuid(),
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
        .find_family_by_code("BEBIDAS")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.name, "Bebidas remotas");

    let mut remote_tax = tax.clone();
    remote_tax.active = false;
    let change = MasterDataChange::new(
        MasterDataKind::Tax,
        tax.id.as_uuid(),
        2,
        tpv_domain::ids::DeviceId::new(),
        time::OffsetDateTime::now_utc(),
        serde_json::to_value(&remote_tax).unwrap(),
        false,
    )
    .unwrap();
    let result = master_data
        .apply_change(context.tenant_id, demo.store_id, &change)
        .await
        .unwrap();
    assert!(result.applied);
    let stored_tax = catalog_repo
        .find_tax_by_code("IVA5")
        .await
        .unwrap()
        .unwrap();
    assert!(!stored_tax.active);
}

#[tokio::test]
async fn seller_and_store_upserts_persist_and_publish_master_changes() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let catalog_repo: Arc<dyn CatalogRepository> = store.clone();
    let master_data_repo: Arc<dyn MasterDataRepository> = store.clone();
    let sync_repo: Arc<dyn SyncOutboxRepository> = store.clone();
    let master_data =
        MasterDataService::new(master_data_repo, Arc::new(tpv_application::SystemClock));
    let sellers = SellerService::new(catalog_repo.clone(), master_data.clone());
    let stores = StoreService::new(catalog_repo.clone(), master_data.clone());
    let pending_before = sync_repo.sync_status().await.unwrap().pending;

    let seller = sellers
        .upsert(
            &context,
            demo.store_id,
            UpsertSellerRequest {
                seller_id: None,
                code: "v2".into(),
                name: "  Vendedor Dos  ".into(),
                phone1: Some(" 600111222 ".into()),
                phone2: Some("   ".into()),
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(seller.code, "v2");
    assert_eq!(seller.name, "Vendedor Dos");
    assert_eq!(seller.phone1.as_deref(), Some("600111222"));
    assert_eq!(seller.phone2, None);

    let store_master = stores
        .upsert(
            &context,
            demo.store_id,
            UpsertStoreRequest {
                store_id: None,
                code: "S2".into(),
                name: "Tienda Dos".into(),
                tax_id: "b12345674".into(),
                phone: Some(" 952000000 ".into()),
                street: Some(" Calle Nueva ".into()),
                number: Some(" 22 ".into()),
                locality: Some(" Benalmádena ".into()),
                postal_code: Some(" 29630 ".into()),
                province: Some(" Málaga ".into()),
                active: true,
            },
        )
        .await
        .unwrap();
    assert_eq!(store_master.code, "S2");
    assert_eq!(store_master.tax_id, "B12345674");
    assert_eq!(store_master.phone.as_deref(), Some("952000000"));
    assert_eq!(store_master.address.street, "Calle Nueva");

    assert_eq!(
        sync_repo.sync_status().await.unwrap().pending,
        pending_before + 2
    );
    assert!(
        catalog_repo
            .find_seller_by_code("v2")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        catalog_repo
            .find_store_by_code("S2")
            .await
            .unwrap()
            .is_some()
    );

    let conflict = sellers
        .upsert(
            &context,
            demo.store_id,
            UpsertSellerRequest {
                seller_id: None,
                code: "v2".into(),
                name: "Duplicado".into(),
                phone1: None,
                phone2: None,
                active: true,
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(conflict, ApplicationError::Conflict(_)));

    let mut remote_seller = seller.clone();
    remote_seller.name = "Vendedor remoto".into();
    remote_seller.active = false;
    let change = MasterDataChange::new(
        MasterDataKind::Seller,
        seller.id.as_uuid(),
        2,
        tpv_domain::ids::DeviceId::new(),
        time::OffsetDateTime::now_utc(),
        serde_json::to_value(&remote_seller).unwrap(),
        false,
    )
    .unwrap();
    let result = master_data
        .apply_change(context.tenant_id, demo.store_id, &change)
        .await
        .unwrap();
    assert!(result.applied);
    let stored_seller = catalog_repo
        .find_seller_by_code("v2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_seller.name, "Vendedor remoto");
    assert!(!stored_seller.active);
    assert!(
        catalog_repo
            .list_sellers()
            .await
            .unwrap()
            .iter()
            .any(|item| item.id == seller.id && !item.active)
    );

    let mut remote_store = store_master.clone();
    remote_store.name = "Tienda remota".into();
    let change = MasterDataChange::new(
        MasterDataKind::Store,
        store_master.id.as_uuid(),
        2,
        tpv_domain::ids::DeviceId::new(),
        time::OffsetDateTime::now_utc(),
        serde_json::to_value(&remote_store).unwrap(),
        false,
    )
    .unwrap();
    let result = master_data
        .apply_change(context.tenant_id, demo.store_id, &change)
        .await
        .unwrap();
    assert!(result.applied);
    let stored_store = catalog_repo
        .find_store_by_code("S2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored_store.name, "Tienda remota");
}
