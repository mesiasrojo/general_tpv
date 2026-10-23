use std::sync::Arc;

use tpv_application::{
    ApplicationError, CatalogRepository, ProductImportRepository, ProductImportService,
    SyncOutboxRepository,
};
use tpv_domain::Sku;
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

fn csv_with_updates() -> &'static str {
    "source_id;sku;nombre;precio;codigo_iva;iva;familia;activo\n\
     old-1;1;Bolsa reutilizable;0,15;IVA21;21;PACKAGING;1\n\
     old-2;2;Agua mineral 50cl;0,50;IVA10;10;;1\n\
     old-3;IMP-1;Café importado;1,35;IVA10;10;BEBIDAS;1\n"
}

#[tokio::test]
async fn previews_and_commits_an_idempotent_batch() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let repo: Arc<dyn ProductImportRepository> = store.clone();
    let catalog: Arc<dyn CatalogRepository> = store.clone();
    let sync: Arc<dyn SyncOutboxRepository> = store.clone();
    let service = ProductImportService::new(repo);

    let plan = service
        .prepare("micro_server", None, csv_with_updates())
        .unwrap();
    assert_eq!(plan.total_rows, 3);
    assert!(plan.errors.is_empty());

    let preview = service.preview(&plan).await.unwrap();
    assert!(!preview.already_imported);
    assert_eq!(preview.inserted, 1);
    assert_eq!(preview.updated, 1);
    assert_eq!(preview.unchanged, 1);
    assert_eq!(sync.sync_status().await.unwrap().pending, 0);

    let report = service
        .commit(&context, demo.store_id, &plan)
        .await
        .unwrap();
    assert!(report.committed);
    assert!(!report.dry_run);
    assert!(!report.already_imported);
    assert!(report.reconciled());
    assert_eq!(report.inserted, 1);
    assert_eq!(report.updated, 1);
    assert_eq!(report.unchanged, 1);
    assert_eq!(sync.sync_status().await.unwrap().pending, 2);

    let updated = catalog
        .find_product_by_sku(&Sku::new("1").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.name, "Bolsa reutilizable");
    assert_eq!(updated.unit_price.to_string(), "0.15");
    assert_eq!(updated.family_code.as_deref(), Some("PACKAGING"));

    let inserted = catalog
        .find_product_by_sku(&Sku::new("IMP-1").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(inserted.name, "Café importado");
    assert_eq!(inserted.tax_rate.percent_bp, 1000);

    let replay = service
        .commit(&context, demo.store_id, &plan)
        .await
        .unwrap();
    assert!(replay.committed);
    assert!(replay.already_imported);
    assert_eq!(replay.inserted, 1);
    assert_eq!(replay.updated, 1);
    assert_eq!(replay.unchanged, 1);
    assert_eq!(sync.sync_status().await.unwrap().pending, 2);

    // A later batch can change the SKU through the stable source identifier
    // without duplicating the product.
    let moved_csv = "source_id;sku;nombre;precio;codigo_iva;iva\n\
                     old-3;IMP-2;Café importado;1,35;IVA10;10\n";
    let moved_plan = service
        .prepare("micro_server", Some("lote-2"), moved_csv)
        .unwrap();
    let moved = service
        .commit(&context, demo.store_id, &moved_plan)
        .await
        .unwrap();
    assert!(moved.committed);
    assert_eq!(moved.updated, 1);
    assert!(
        catalog
            .find_product_by_sku(&Sku::new("IMP-2").unwrap())
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        catalog
            .find_product_by_sku(&Sku::new("IMP-1").unwrap())
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn invalid_rows_block_the_commit_without_writing() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let repo: Arc<dyn ProductImportRepository> = store.clone();
    let catalog: Arc<dyn CatalogRepository> = store.clone();
    let sync: Arc<dyn SyncOutboxRepository> = store.clone();
    let service = ProductImportService::new(repo);
    let before = catalog.list_products().await.unwrap().len();

    let csv = "source_id,sku,name,unit_price,tax_percent_bp\n\
               new-1,NEW-1,Producto válido,3.50,2100\n\
               new-2,NEW-2,Producto inválido,no-es-precio,2100\n";
    let plan = service.prepare("micro_server", None, csv).unwrap();
    assert_eq!(plan.rows.len(), 1);
    assert_eq!(plan.errors.len(), 1);

    let report = service
        .commit(&context, demo.store_id, &plan)
        .await
        .unwrap();
    assert!(!report.committed);
    assert_eq!(report.invalid_rows, 1);
    assert_eq!(catalog.list_products().await.unwrap().len(), before);
    assert_eq!(sync.sync_status().await.unwrap().pending, 0);
}

#[tokio::test]
async fn source_mapping_cannot_hijack_another_products_sku() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let context = store.sync_context().await.unwrap();
    let repo: Arc<dyn ProductImportRepository> = store.clone();
    let catalog: Arc<dyn CatalogRepository> = store.clone();
    let service = ProductImportService::new(repo);

    let first = service
        .prepare(
            "micro_server",
            Some("batch-a"),
            "source_id,sku,name,unit_price,tax_percent_bp\n\
             c1,CONFLICT-A,Producto A,1.00,2100\n",
        )
        .unwrap();
    service
        .commit(&context, demo.store_id, &first)
        .await
        .unwrap();

    let other = service
        .prepare(
            "micro_server",
            Some("batch-other"),
            "source_id,sku,name,unit_price,tax_percent_bp\n\
             c2,CONFLICT-B,Producto B,2.00,2100\n",
        )
        .unwrap();
    service
        .commit(&context, demo.store_id, &other)
        .await
        .unwrap();

    let second = service
        .prepare(
            "micro_server",
            Some("batch-b"),
            "source_id,sku,name,unit_price,tax_percent_bp\n\
             c1,CONFLICT-B,Producto B,2.00,2100\n",
        )
        .unwrap();
    let error = service
        .commit(&context, demo.store_id, &second)
        .await
        .unwrap_err();
    assert!(matches!(error, ApplicationError::Conflict(_)));

    let product_a = catalog
        .find_product_by_sku(&Sku::new("CONFLICT-A").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(product_a.name, "Producto A");
    assert_eq!(product_a.unit_price.to_string(), "1.00");
}
