use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::ids::{NoteId, StoreId, UserId};
use tpv_domain::notes::Note;
use tpv_domain::people::UserAccount;

use crate::dto::{CreateNoteRequest, NoteInbox, NoteRecipientOption, NoteRecipientView, NoteView};
use crate::error::{ApplicationError, Result};
use crate::ports::{Clock, NoteRepository, StoredNote, UserRepository};

/// Internal store notes with recipient read receipts.
#[derive(Clone)]
pub struct NoteService {
    notes: Arc<dyn NoteRepository>,
    users: Arc<dyn UserRepository>,
    clock: Arc<dyn Clock>,
}

impl NoteService {
    pub fn new(
        notes: Arc<dyn NoteRepository>,
        users: Arc<dyn UserRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            notes,
            users,
            clock,
        }
    }

    pub async fn inbox(
        &self,
        store_id: StoreId,
        viewer_id: UserId,
        can_manage: bool,
        limit: u32,
    ) -> Result<NoteInbox> {
        let limit = limit.clamp(1, 200);
        let stored = self
            .notes
            .list(store_id, viewer_id, can_manage, limit)
            .await?;
        let users = self.users_by_id().await?;
        let recipients = if can_manage {
            users
                .values()
                .filter(|user| user.active && can_receive(user, store_id))
                .map(|user| NoteRecipientOption {
                    user_id: user.id,
                    user_name: user.display_name.clone(),
                })
                .collect()
        } else {
            Vec::new()
        };
        let views: Vec<NoteView> = stored
            .into_iter()
            .map(|stored| self.to_view(stored, viewer_id, &users))
            .collect();
        let unread = views
            .iter()
            .filter(|view| view.viewer_read_at.is_none() && !view.is_author)
            .count();
        let unread = u32::try_from(unread).unwrap_or(u32::MAX);
        Ok(NoteInbox {
            notes: views,
            unread,
            recipients,
        })
    }

    pub async fn create(
        &self,
        store_id: StoreId,
        author_id: UserId,
        request: CreateNoteRequest,
    ) -> Result<Note> {
        let now = self.clock.now();
        let note = Note::new(
            NoteId::new(),
            store_id,
            author_id,
            request.title,
            request.body,
            now,
        )?;
        let mut seen = HashSet::new();
        let mut recipient_ids = Vec::new();
        for user_id in request.recipient_ids {
            if seen.insert(user_id) {
                recipient_ids.push(user_id);
            }
        }
        if recipient_ids.is_empty() {
            return Err(ApplicationError::Validation(
                "note requires at least one recipient".into(),
            ));
        }
        let users = self.users_by_id().await?;
        for recipient_id in &recipient_ids {
            let user = users.get(recipient_id).ok_or_else(|| {
                ApplicationError::Validation(format!("unknown note recipient {recipient_id}"))
            })?;
            if !user.active || !can_receive(user, store_id) {
                return Err(ApplicationError::Validation(format!(
                    "{} cannot receive notes for this store",
                    user.display_name
                )));
            }
        }
        let audit = AuditEvent::new(
            now,
            Some(author_id),
            Some(store_id),
            AuditAction::NoteCreated,
            "note",
            note.id.to_string(),
            serde_json::json!({
                "title": note.title,
                "recipient_ids": recipient_ids,
            }),
        )?;
        self.notes.create(&note, &recipient_ids, &audit).await?;
        Ok(note)
    }

    /// Marks a note as read for the viewer and returns the refreshed inbox.
    pub async fn read(
        &self,
        note_id: NoteId,
        store_id: StoreId,
        viewer_id: UserId,
        can_manage: bool,
    ) -> Result<NoteInbox> {
        self.notes
            .mark_read(note_id, viewer_id, self.clock.now())
            .await?;
        self.inbox(store_id, viewer_id, can_manage, 100).await
    }

    async fn users_by_id(&self) -> Result<HashMap<UserId, UserAccount>> {
        Ok(self
            .users
            .list_users()
            .await?
            .into_iter()
            .map(|user| (user.id, user))
            .collect())
    }

    fn to_view(
        &self,
        stored: StoredNote,
        viewer_id: UserId,
        users: &HashMap<UserId, UserAccount>,
    ) -> NoteView {
        let viewer_read_at = stored
            .recipients
            .iter()
            .find(|recipient| recipient.user_id == viewer_id)
            .and_then(|recipient| recipient.read_at);
        let recipients = stored
            .recipients
            .iter()
            .map(|recipient| NoteRecipientView {
                user_id: recipient.user_id,
                user_name: users
                    .get(&recipient.user_id)
                    .map(|user| user.display_name.clone())
                    .unwrap_or_else(|| "Usuario desconocido".to_string()),
                read_at: recipient.read_at,
            })
            .collect();
        NoteView {
            author_name: users
                .get(&stored.note.author_id)
                .map(|user| user.display_name.clone())
                .unwrap_or_else(|| "Usuario desconocido".to_string()),
            is_author: stored.note.author_id == viewer_id,
            note: stored.note,
            recipients,
            viewer_read_at,
        }
    }
}

fn can_receive(user: &UserAccount, store_id: StoreId) -> bool {
    user.role == tpv_domain::people::Role::Admin || user.store_ids.contains(&store_id)
}
