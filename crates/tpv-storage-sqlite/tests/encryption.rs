use std::io::Read;
use std::path::{Path, PathBuf};

use sqlx::Row;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tpv_application::CatalogRepository;
use tpv_storage_sqlite::{DatabaseKey, SqliteStore};
use uuid::Uuid;

fn temp_dir() -> PathBuf {
    std::env::temp_dir().join(format!("tpv-security-{}", Uuid::now_v7()))
}

fn read_header(path: &Path) -> Vec<u8> {
    let mut header = vec![0_u8; 16];
    let mut file = std::fs::File::open(path).unwrap();
    file.read_exact(&mut header).unwrap();
    header
}

#[tokio::test]
async fn encrypted_database_requires_the_right_key() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("tpv.db");
    let path_str = path.to_str().unwrap();
    let key = DatabaseKey::generate();

    {
        let store = SqliteStore::connect(path_str, &key).await.unwrap();
        store.seed_demo().await.unwrap();
    }

    assert_ne!(read_header(&path), b"SQLite format 3\0");
    let wrong_key = DatabaseKey::generate();
    assert!(SqliteStore::connect(path_str, &wrong_key).await.is_err());

    let store = SqliteStore::connect(path_str, &key).await.unwrap();
    assert!(!store.list_stores().await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn migrates_legacy_plaintext_database_in_place() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("legacy.db");
    let path_str = path.to_str().unwrap();

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path_str)
                .create_if_missing(true),
        )
        .await
        .unwrap();
    sqlx::query("CREATE TABLE legacy_data (id INTEGER PRIMARY KEY, value TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO legacy_data (value) VALUES ('conservado')")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    assert_eq!(read_header(&path), b"SQLite format 3\0");

    let key = DatabaseKey::generate();
    let store = SqliteStore::connect(path_str, &key).await.unwrap();
    let row = sqlx::query("SELECT value FROM legacy_data")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("value"), "conservado");
    assert_ne!(read_header(&path), b"SQLite format 3\0");

    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn encrypted_backup_can_be_restored_with_the_same_key() {
    let dir = temp_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let database = dir.join("tpv.db");
    let backup = dir.join("backups").join("snapshot.db");
    let key = DatabaseKey::generate();

    let store = SqliteStore::connect(database.to_str().unwrap(), &key)
        .await
        .unwrap();
    store.seed_demo().await.unwrap();
    let bytes = store.backup_to(&backup).await.unwrap();
    assert!(bytes > 0);
    assert_ne!(read_header(&backup), b"SQLite format 3\0");
    assert!(store.backup_to(&backup).await.is_err());

    let wrong_key = DatabaseKey::generate();
    assert!(
        SqliteStore::connect(backup.to_str().unwrap(), &wrong_key)
            .await
            .is_err()
    );
    let restored = SqliteStore::connect(backup.to_str().unwrap(), &key)
        .await
        .unwrap();
    assert!(!restored.list_stores().await.unwrap().is_empty());

    let _ = std::fs::remove_dir_all(dir);
}
