use std::sync::Arc;

use tpv_application::UserRepository;
use tpv_domain::ids::{StoreId, UserId};
use tpv_domain::people::{Role, UserAccount, UserProvision};
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};

#[tokio::test]
async fn provisioned_users_are_applied_by_revision() {
    let store = Arc::new(
        SqliteStore::in_memory_with_key(&DatabaseKey::generate())
            .await
            .unwrap(),
    );
    let demo = store.seed_demo().await.unwrap();
    let user_repo: Arc<dyn UserRepository> = store.clone();

    let user_id = UserId::new();
    let store_id = demo.store_id;
    let mut provision = UserProvision {
        user: UserAccount {
            id: user_id,
            username: "central.cashier".into(),
            display_name: "Cajero Central".into(),
            role: Role::Cashier,
            store_ids: vec![store_id],
            active: true,
        },
        password_hash: "hash-v1".into(),
        revision: 1,
    };

    assert!(user_repo.upsert_provisioned(&provision).await.unwrap());
    let stored = user_repo.find_by_id(user_id).await.unwrap().unwrap();
    assert_eq!(stored.account.display_name, "Cajero Central");
    assert_eq!(stored.account.role, Role::Cashier);
    assert_eq!(
        stored.account.store_ids,
        vec![StoreId::from_uuid(store_id.as_uuid())]
    );
    assert_eq!(stored.password_hash.as_deref(), Some("hash-v1"));

    provision.user.display_name = "Nombre obsoleto".into();
    assert!(!user_repo.upsert_provisioned(&provision).await.unwrap());
    let stored = user_repo.find_by_id(user_id).await.unwrap().unwrap();
    assert_eq!(stored.account.display_name, "Cajero Central");

    provision.user.display_name = "Cajero Actualizado".into();
    provision.user.active = false;
    provision.password_hash = "hash-v2".into();
    provision.revision = 2;
    assert!(user_repo.upsert_provisioned(&provision).await.unwrap());
    let stored = user_repo.find_by_id(user_id).await.unwrap().unwrap();
    assert_eq!(stored.account.display_name, "Cajero Actualizado");
    assert!(!stored.account.active);
    assert_eq!(stored.password_hash.as_deref(), Some("hash-v2"));

    let users = user_repo.list_users().await.unwrap();
    assert!(users.iter().any(|user| user.id == user_id));
}
