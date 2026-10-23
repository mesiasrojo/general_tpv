use serde_json::json;
use time::macros::datetime;
use tpv_application::{
    CatalogRepository, MasterDataRepository, PublishMasterDataRequest, SyncOutboxRepository,
};
use tpv_domain::ids::{CustomerId, DeviceId, StoreId};
use tpv_domain::master_data::{MasterDataChange, MasterDataKind};
use tpv_domain::people::Customer;
use tpv_storage_sqlite::SqliteStore;
use uuid::Uuid;

#[tokio::test]
async fn publishes_versioned_master_data_and_records_conflicts() {
    let store = SqliteStore::in_memory().await.unwrap();
    let sync = store.sync_context().await.unwrap();
    let store_id = StoreId::new();
    let entity_id = Uuid::now_v7();

    let event = store
        .publish_change(PublishMasterDataRequest {
            context: sync.clone(),
            store_id,
            kind: MasterDataKind::Product,
            entity_id,
            payload: json!({"name": "Producto A"}),
            deleted: false,
            occurred_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    let first: MasterDataChange = serde_json::from_value(event.payload.clone()).unwrap();
    assert_eq!(first.version, 1);
    assert_eq!(store.pending_events(10).await.unwrap().len(), 1);

    let later = MasterDataChange::new(
        MasterDataKind::Product,
        entity_id,
        1,
        DeviceId::new(),
        first.occurred_at + time::Duration::seconds(1),
        json!({"name": "Producto B"}),
        false,
    )
    .unwrap();
    let result = store
        .apply_change(sync.tenant_id, store_id, &later)
        .await
        .unwrap();
    assert!(result.applied);
    assert!(result.conflict.is_some());

    let second = store
        .publish_change(PublishMasterDataRequest {
            context: sync.clone(),
            store_id,
            kind: MasterDataKind::Product,
            entity_id,
            payload: json!({"name": "Producto C"}),
            deleted: false,
            occurred_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    let second_change: MasterDataChange = serde_json::from_value(second.payload).unwrap();
    assert_eq!(second_change.version, 2);

    let older = MasterDataChange::new(
        MasterDataKind::Product,
        entity_id,
        1,
        DeviceId::new(),
        datetime!(2026-01-01 00:00:00 UTC),
        json!({"name": "Producto antiguo"}),
        false,
    )
    .unwrap();
    let result = store
        .apply_change(sync.tenant_id, store_id, &older)
        .await
        .unwrap();
    assert!(!result.applied);
    assert!(result.conflict.is_none());

    let conflicts = store.list_conflicts(10).await.unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].entity_id, entity_id);
}

#[tokio::test]
async fn publishes_and_applies_customer_master_data() {
    let store = SqliteStore::in_memory().await.unwrap();
    let sync = store.sync_context().await.unwrap();
    let store_id = StoreId::new();
    let customer = Customer::new(CustomerId::new(), "Ana", "García").unwrap();

    let event = store
        .publish_change(PublishMasterDataRequest {
            context: sync.clone(),
            store_id,
            kind: MasterDataKind::Customer,
            entity_id: customer.id.as_uuid(),
            payload: serde_json::to_value(&customer).unwrap(),
            deleted: false,
            occurred_at: time::OffsetDateTime::now_utc(),
        })
        .await
        .unwrap();
    let local = CatalogRepository::find_customer(&store, customer.id)
        .await
        .unwrap()
        .expect("customer must be persisted locally");
    assert_eq!(local.full_name(), "Ana García");

    let published: MasterDataChange = serde_json::from_value(event.payload).unwrap();
    let mut updated = customer.clone();
    updated.phone1 = Some("600123123".into());
    let remote = MasterDataChange::new(
        MasterDataKind::Customer,
        updated.id.as_uuid(),
        published.version + 1,
        DeviceId::new(),
        published.occurred_at + time::Duration::seconds(1),
        serde_json::to_value(&updated).unwrap(),
        false,
    )
    .unwrap();
    let result = store
        .apply_change(sync.tenant_id, store_id, &remote)
        .await
        .unwrap();
    assert!(result.applied);
    let applied = CatalogRepository::find_customer(&store, customer.id)
        .await
        .unwrap()
        .expect("customer must be updated");
    assert_eq!(applied.phone1.as_deref(), Some("600123123"));
}
