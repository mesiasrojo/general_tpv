use std::sync::Arc;

use tpv_application::{
    ApplicationError, CatalogRepository, FamilyService, MasterDataRepository, MasterDataService,
    SyncOutboxRepository, TaxService, UpsertFamilyRequest, UpsertTaxRequest,
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
