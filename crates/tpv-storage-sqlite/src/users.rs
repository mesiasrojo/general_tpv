use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_application::{Result as AppResult, StoredUser, UserRepository};
use tpv_domain::ids::{StoreId, UserId};
use tpv_domain::people::{Role, UserAccount};

use crate::error::StorageResult;
use crate::helpers::{enum_from_text, enum_to_text, format_time, parse_optional_time, parse_uuid};
use crate::store::SqliteStore;

async fn load_store_ids(store: &SqliteStore, user_id: UserId) -> StorageResult<Vec<StoreId>> {
    let rows = sqlx::query("SELECT store_id FROM user_stores WHERE user_id = ?")
        .bind(user_id.to_string())
        .fetch_all(store.pool())
        .await?;
    rows.iter()
        .map(|row| {
            Ok(StoreId::from_uuid(parse_uuid(
                &row.try_get::<String, _>("store_id")?,
            )?))
        })
        .collect()
}

async fn row_to_stored_user(store: &SqliteStore, row: &SqliteRow) -> StorageResult<StoredUser> {
    let id = UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?);
    let account = UserAccount {
        id,
        username: row.try_get("username")?,
        display_name: row.try_get("display_name")?,
        role: enum_from_text::<Role>(&row.try_get::<String, _>("role")?)?,
        store_ids: load_store_ids(store, id).await?,
        active: row.try_get::<i64, _>("active")? != 0,
    };
    Ok(StoredUser {
        account,
        password_hash: row.try_get("password_hash")?,
        failed_attempts: u32::try_from(row.try_get::<i64, _>("failed_attempts")?).unwrap_or(0),
        locked_until: parse_optional_time(row.try_get("locked_until")?)?,
        last_login_at: parse_optional_time(row.try_get("last_login_at")?)?,
    })
}

impl SqliteStore {
    async fn find_user_by_username_inner(
        &self,
        username: &str,
    ) -> StorageResult<Option<StoredUser>> {
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(self.pool())
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_stored_user(self, &row).await?)),
            None => Ok(None),
        }
    }

    async fn find_user_by_id_inner(&self, id: UserId) -> StorageResult<Option<StoredUser>> {
        let row = sqlx::query("SELECT * FROM users WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(self.pool())
            .await?;
        match row {
            Some(row) => Ok(Some(row_to_stored_user(self, &row).await?)),
            None => Ok(None),
        }
    }

    async fn create_user_inner(
        &self,
        user: &UserAccount,
        password_hash: &str,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO users
                (id, username, display_name, role, active, password_hash, failed_attempts)
             VALUES (?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(user.id.to_string())
        .bind(&user.username)
        .bind(&user.display_name)
        .bind(enum_to_text(&user.role)?)
        .bind(i64::from(user.active))
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;
        for store_id in &user.store_ids {
            sqlx::query("INSERT INTO user_stores (user_id, store_id) VALUES (?, ?)")
                .bind(user.id.to_string())
                .bind(store_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn update_user_inner(&self, user: &UserAccount) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        let result =
            sqlx::query("UPDATE users SET display_name = ?, role = ?, active = ? WHERE id = ?")
                .bind(&user.display_name)
                .bind(enum_to_text(&user.role)?)
                .bind(i64::from(user.active))
                .bind(user.id.to_string())
                .execute(&mut *tx)
                .await?;
        if result.rows_affected() == 0 {
            return Err(crate::error::StorageError::NotFound(format!(
                "user {}",
                user.id
            )));
        }
        sqlx::query("DELETE FROM user_stores WHERE user_id = ?")
            .bind(user.id.to_string())
            .execute(&mut *tx)
            .await?;
        for store_id in &user.store_ids {
            sqlx::query("INSERT INTO user_stores (user_id, store_id) VALUES (?, ?)")
                .bind(user.id.to_string())
                .bind(store_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    async fn set_password_hash_inner(&self, id: UserId, password_hash: &str) -> StorageResult<()> {
        sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(password_hash)
            .bind(id.to_string())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    async fn record_login_inner(
        &self,
        id: UserId,
        failed_attempts: u32,
        locked_until: Option<time::OffsetDateTime>,
        last_login_at: Option<time::OffsetDateTime>,
    ) -> StorageResult<()> {
        sqlx::query(
            "UPDATE users SET failed_attempts = ?, locked_until = ?, last_login_at = ? WHERE id = ?",
        )
        .bind(i64::from(failed_attempts))
        .bind(locked_until.map(format_time).transpose()?)
        .bind(last_login_at.map(format_time).transpose()?)
        .bind(id.to_string())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    async fn list_users_inner(&self) -> StorageResult<Vec<UserAccount>> {
        let rows = sqlx::query("SELECT id FROM users ORDER BY username")
            .fetch_all(self.pool())
            .await?;
        let mut users = Vec::with_capacity(rows.len());
        for row in rows {
            let id = UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?);
            if let Some(stored) = self.find_user_by_id_inner(id).await? {
                users.push(stored.account);
            }
        }
        Ok(users)
    }
}

#[async_trait]
impl UserRepository for SqliteStore {
    async fn find_by_username(&self, username: &str) -> AppResult<Option<StoredUser>> {
        Ok(self.find_user_by_username_inner(username).await?)
    }

    async fn find_by_id(&self, id: UserId) -> AppResult<Option<StoredUser>> {
        Ok(self.find_user_by_id_inner(id).await?)
    }

    async fn create_user(&self, user: &UserAccount, password_hash: &str) -> AppResult<()> {
        Ok(self.create_user_inner(user, password_hash).await?)
    }

    async fn update_user(&self, user: &UserAccount) -> AppResult<()> {
        Ok(self.update_user_inner(user).await?)
    }

    async fn set_password_hash(&self, id: UserId, password_hash: &str) -> AppResult<()> {
        Ok(self.set_password_hash_inner(id, password_hash).await?)
    }

    async fn record_login(
        &self,
        id: UserId,
        failed_attempts: u32,
        locked_until: Option<time::OffsetDateTime>,
        last_login_at: Option<time::OffsetDateTime>,
    ) -> AppResult<()> {
        Ok(self
            .record_login_inner(id, failed_attempts, locked_until, last_login_at)
            .await?)
    }

    async fn list_users(&self) -> AppResult<Vec<UserAccount>> {
        Ok(self.list_users_inner().await?)
    }
}
