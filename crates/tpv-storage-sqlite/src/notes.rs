use std::collections::HashMap;

use async_trait::async_trait;
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use tpv_application::{NoteRepository, Result as AppResult, StoredNote};
use tpv_domain::audit::AuditEvent;
use tpv_domain::ids::{NoteId, StoreId, UserId};
use tpv_domain::notes::{Note, NoteRecipient};

use crate::error::StorageResult;
use crate::helpers::{format_time, parse_time, parse_uuid};
use crate::persistence::insert_audit_tx;
use crate::store::SqliteStore;

const QUERY_CHUNK: usize = 400;

fn row_to_note(row: &SqliteRow) -> StorageResult<Note> {
    Ok(Note {
        id: NoteId::from_uuid(parse_uuid(&row.try_get::<String, _>("id")?)?),
        store_id: StoreId::from_uuid(parse_uuid(&row.try_get::<String, _>("store_id")?)?),
        author_id: UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("author_id")?)?),
        title: row.try_get("title")?,
        body: row.try_get("body")?,
        created_at: parse_time(&row.try_get::<String, _>("created_at")?)?,
    })
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

impl SqliteStore {
    async fn create_note_inner(
        &self,
        note: &Note,
        recipient_ids: &[UserId],
        audit: &AuditEvent,
    ) -> StorageResult<()> {
        let mut tx = self.pool().begin().await?;
        sqlx::query(
            "INSERT INTO notes (id, store_id, author_id, title, body, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(note.id.to_string())
        .bind(note.store_id.to_string())
        .bind(note.author_id.to_string())
        .bind(&note.title)
        .bind(&note.body)
        .bind(format_time(note.created_at)?)
        .execute(&mut *tx)
        .await?;
        for recipient_id in recipient_ids {
            sqlx::query(
                "INSERT INTO note_recipients (note_id, user_id, read_at) VALUES (?, ?, NULL)",
            )
            .bind(note.id.to_string())
            .bind(recipient_id.to_string())
            .execute(&mut *tx)
            .await?;
        }
        insert_audit_tx(&mut tx, audit).await?;
        tx.commit().await?;
        Ok(())
    }

    async fn list_notes_inner(
        &self,
        store_id: StoreId,
        viewer_id: UserId,
        all_users: bool,
        limit: u32,
    ) -> StorageResult<Vec<StoredNote>> {
        let rows = if all_users {
            sqlx::query(
                "SELECT id, store_id, author_id, title, body, created_at
                 FROM notes
                 WHERE store_id = ?
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(store_id.to_string())
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?
        } else {
            sqlx::query(
                "SELECT n.id, n.store_id, n.author_id, n.title, n.body, n.created_at
                 FROM notes n
                 JOIN note_recipients r ON r.note_id = n.id
                 WHERE n.store_id = ? AND r.user_id = ?
                 ORDER BY n.created_at DESC LIMIT ?",
            )
            .bind(store_id.to_string())
            .bind(viewer_id.to_string())
            .bind(i64::from(limit))
            .fetch_all(self.pool())
            .await?
        };
        let notes: Vec<Note> = rows.iter().map(row_to_note).collect::<StorageResult<_>>()?;
        let ids: Vec<NoteId> = notes.iter().map(|note| note.id).collect();
        let mut recipients = self.load_note_recipients(&ids).await?;
        Ok(notes
            .into_iter()
            .map(|note| StoredNote {
                recipients: recipients.remove(&note.id).unwrap_or_default(),
                note,
            })
            .collect())
    }

    async fn load_note_recipients(
        &self,
        note_ids: &[NoteId],
    ) -> StorageResult<HashMap<NoteId, Vec<NoteRecipient>>> {
        let mut grouped: HashMap<NoteId, Vec<NoteRecipient>> = HashMap::new();
        for chunk in note_ids.chunks(QUERY_CHUNK) {
            let sql = format!(
                "SELECT note_id, user_id, read_at FROM note_recipients
                 WHERE note_id IN ({})",
                placeholders(chunk.len())
            );
            let mut query = sqlx::query(&sql);
            for note_id in chunk {
                query = query.bind(note_id.to_string());
            }
            let rows = query.fetch_all(self.pool()).await?;
            for row in rows {
                let note_id = NoteId::from_uuid(parse_uuid(&row.try_get::<String, _>("note_id")?)?);
                let user_id = UserId::from_uuid(parse_uuid(&row.try_get::<String, _>("user_id")?)?);
                let read_at = row
                    .try_get::<Option<String>, _>("read_at")?
                    .map(|value| parse_time(&value))
                    .transpose()?;
                grouped
                    .entry(note_id)
                    .or_default()
                    .push(NoteRecipient { user_id, read_at });
            }
        }
        for recipients in grouped.values_mut() {
            recipients.sort_by_key(|recipient| recipient.user_id.to_string());
        }
        Ok(grouped)
    }

    async fn mark_note_read_inner(
        &self,
        note_id: NoteId,
        user_id: UserId,
        read_at: time::OffsetDateTime,
    ) -> StorageResult<bool> {
        let result = sqlx::query(
            "UPDATE note_recipients
             SET read_at = COALESCE(read_at, ?)
             WHERE note_id = ? AND user_id = ?",
        )
        .bind(format_time(read_at)?)
        .bind(note_id.to_string())
        .bind(user_id.to_string())
        .execute(self.pool())
        .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl NoteRepository for SqliteStore {
    async fn create(
        &self,
        note: &Note,
        recipient_ids: &[UserId],
        audit: &AuditEvent,
    ) -> AppResult<()> {
        Ok(self.create_note_inner(note, recipient_ids, audit).await?)
    }

    async fn list(
        &self,
        store_id: StoreId,
        viewer_id: UserId,
        all_users: bool,
        limit: u32,
    ) -> AppResult<Vec<StoredNote>> {
        Ok(self
            .list_notes_inner(store_id, viewer_id, all_users, limit)
            .await?)
    }

    async fn mark_read(
        &self,
        note_id: NoteId,
        user_id: UserId,
        read_at: time::OffsetDateTime,
    ) -> AppResult<bool> {
        Ok(self.mark_note_read_inner(note_id, user_id, read_at).await?)
    }
}
