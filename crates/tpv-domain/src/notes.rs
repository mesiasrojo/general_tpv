use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::error::{DomainError, Result};
use crate::ids::{NoteId, StoreId, UserId};

/// Internal note addressed to one or more members of the store team.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub store_id: StoreId,
    pub author_id: UserId,
    pub title: String,
    pub body: String,
    pub created_at: OffsetDateTime,
}

impl Note {
    pub fn new(
        id: NoteId,
        store_id: StoreId,
        author_id: UserId,
        title: impl Into<String>,
        body: impl Into<String>,
        created_at: OffsetDateTime,
    ) -> Result<Self> {
        let title = title.into().trim().to_string();
        let body = body.into().trim().to_string();
        if title.is_empty() {
            return Err(DomainError::Validation("note title is required".into()));
        }
        if title.chars().count() > 120 {
            return Err(DomainError::Validation(
                "note title supports at most 120 characters".into(),
            ));
        }
        if body.is_empty() {
            return Err(DomainError::Validation("note body is required".into()));
        }
        if body.chars().count() > 4_000 {
            return Err(DomainError::Validation(
                "note body supports at most 4000 characters".into(),
            ));
        }
        Ok(Self {
            id,
            store_id,
            author_id,
            title,
            body,
            created_at,
        })
    }
}

/// Read receipt for a note recipient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteRecipient {
    pub user_id: UserId,
    pub read_at: Option<OffsetDateTime>,
}

#[cfg(test)]
mod tests {
    use super::Note;
    use crate::ids::{NoteId, StoreId, UserId};
    use time::OffsetDateTime;

    #[test]
    fn note_trims_text_and_requires_content() {
        let note = Note::new(
            NoteId::new(),
            StoreId::new(),
            UserId::new(),
            "  Cambio de turno  ",
            "  Abrir caja a las 08:30.  ",
            OffsetDateTime::now_utc(),
        )
        .unwrap();
        assert_eq!(note.title, "Cambio de turno");
        assert_eq!(note.body, "Abrir caja a las 08:30.");

        assert!(
            Note::new(
                NoteId::new(),
                StoreId::new(),
                UserId::new(),
                "   ",
                "Contenido",
                OffsetDateTime::now_utc(),
            )
            .is_err()
        );
        assert!(
            Note::new(
                NoteId::new(),
                StoreId::new(),
                UserId::new(),
                "Título",
                "   ",
                OffsetDateTime::now_utc(),
            )
            .is_err()
        );
    }
}
