use std::sync::Arc;

use tpv_application::{
    ApplicationError, AuditRepository, Clock, SystemClock, TimeClockRepository, TimeClockService,
    UserRepository,
};
use tpv_domain::audit::AuditAction;
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

#[tokio::test]
async fn clock_in_and_out_are_audited_and_only_one_entry_can_stay_open() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let entries: Arc<dyn TimeClockRepository> = store.clone();
    let users: Arc<dyn UserRepository> = store.clone();
    let audits: Arc<dyn AuditRepository> = store.clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let service = TimeClockService::new(entries, users, clock);

    let empty = service
        .status(demo.store_id, demo.user_id, false, 20)
        .await
        .unwrap();
    assert!(empty.active.is_none());
    assert!(empty.recent.is_empty());

    let opened = service.clock_in(demo.user_id, demo.store_id).await.unwrap();
    assert_eq!(opened.user_id, demo.user_id);
    assert!(opened.clock_out.is_none());

    let active = service
        .status(demo.store_id, demo.user_id, false, 20)
        .await
        .unwrap();
    let active = active.active.expect("active entry");
    assert_eq!(active.entry.id, opened.id);
    assert_eq!(active.user_name, "Administrador");

    let duplicate = service.clock_in(demo.user_id, demo.store_id).await;
    assert!(matches!(duplicate, Err(ApplicationError::Conflict(_))));

    let closed = service
        .clock_out(demo.user_id, demo.store_id)
        .await
        .unwrap();
    assert_eq!(closed.id, opened.id);
    assert!(closed.clock_out.is_some());

    let after = service
        .status(demo.store_id, demo.user_id, false, 20)
        .await
        .unwrap();
    assert!(after.active.is_none());
    assert_eq!(after.recent.len(), 1);
    assert_eq!(after.recent[0].entry.id, opened.id);

    let audit = audits.list_recent(Some(demo.store_id), 20).await.unwrap();
    assert!(
        audit
            .iter()
            .any(|event| event.action == AuditAction::TimeClockIn)
    );
    assert!(
        audit
            .iter()
            .any(|event| event.action == AuditAction::TimeClockOut)
    );
}
