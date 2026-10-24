use std::sync::Arc;

use tpv_application::{
    AuditRepository, Clock, CreateNoteRequest, NoteRepository, NoteService, SystemClock,
    UserRepository,
};
use tpv_domain::audit::AuditAction;
use tpv_domain::ids::UserId;
use tpv_domain::people::{Role, UserAccount};
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

#[tokio::test]
async fn notes_are_delivered_to_recipients_and_track_first_read() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let notes: Arc<dyn NoteRepository> = store.clone();
    let users: Arc<dyn UserRepository> = store.clone();
    let audits: Arc<dyn AuditRepository> = store.clone();
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let service = NoteService::new(notes, users.clone(), clock);

    let cashier_id = UserId::new();
    let mut cashier =
        UserAccount::new(cashier_id, "cajera", "Cajera turno", Role::Cashier).unwrap();
    cashier.store_ids = vec![demo.store_id];
    users
        .create_user(&cashier, "not-a-real-hash")
        .await
        .unwrap();

    let note = service
        .create(
            demo.store_id,
            demo.user_id,
            CreateNoteRequest {
                title: "Cambio de turno".into(),
                body: "Abrir caja a las 08:30.".into(),
                recipient_ids: vec![cashier_id, cashier_id],
            },
        )
        .await
        .unwrap();

    let manager_inbox = service
        .inbox(demo.store_id, demo.user_id, true, 20)
        .await
        .unwrap();
    assert_eq!(manager_inbox.notes.len(), 1);
    assert!(manager_inbox.notes[0].is_author);
    assert_eq!(manager_inbox.notes[0].recipients.len(), 1);
    assert!(!manager_inbox.notes[0].recipients[0].user_name.is_empty());
    assert_eq!(manager_inbox.recipients.len(), 2);

    let cashier_inbox = service
        .inbox(demo.store_id, cashier_id, false, 20)
        .await
        .unwrap();
    assert_eq!(cashier_inbox.notes.len(), 1);
    assert_eq!(cashier_inbox.unread, 1);
    assert_eq!(cashier_inbox.notes[0].note.id, note.id);
    assert!(cashier_inbox.notes[0].viewer_read_at.is_none());
    assert!(!cashier_inbox.notes[0].is_author);

    let read_inbox = service
        .read(note.id, demo.store_id, cashier_id, false)
        .await
        .unwrap();
    assert_eq!(read_inbox.unread, 0);
    assert!(read_inbox.notes[0].viewer_read_at.is_some());

    let audit = audits.list_recent(Some(demo.store_id), 20).await.unwrap();
    assert!(
        audit
            .iter()
            .any(|event| event.action == AuditAction::NoteCreated)
    );
}
