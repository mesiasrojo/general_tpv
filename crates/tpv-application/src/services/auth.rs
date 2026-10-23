use std::sync::Arc;

use time::{Duration, OffsetDateTime};
use tpv_domain::audit::{AuditAction, AuditEvent};
use tpv_domain::ids::UserId;
use tpv_domain::people::{Role, UserAccount};

use crate::dto::SessionUser;
use crate::error::{ApplicationError, Result};
use crate::ports::{AuditRepository, Clock, PasswordHasher, UserRepository};

const MAX_FAILED_ATTEMPTS: u32 = 5;
const LOCK_MINUTES: i64 = 15;
const MIN_PASSWORD_LENGTH: usize = 8;

#[derive(Clone)]
pub struct AuthService {
    users: Arc<dyn UserRepository>,
    hasher: Arc<dyn PasswordHasher>,
    audit: Arc<dyn AuditRepository>,
    clock: Arc<dyn Clock>,
    max_failed_attempts: u32,
    lock_minutes: i64,
}

impl AuthService {
    pub fn new(
        users: Arc<dyn UserRepository>,
        hasher: Arc<dyn PasswordHasher>,
        audit: Arc<dyn AuditRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            users,
            hasher,
            audit,
            clock,
            max_failed_attempts: MAX_FAILED_ATTEMPTS,
            lock_minutes: LOCK_MINUTES,
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<SessionUser> {
        let username = username.trim().to_lowercase();
        if username.is_empty() || password.is_empty() {
            return Err(ApplicationError::Unauthorized(
                "usuario y contraseña son obligatorios".into(),
            ));
        }
        let stored = self
            .users
            .find_by_username(&username)
            .await?
            .ok_or_else(|| ApplicationError::Unauthorized("credenciales inválidas".into()))?;
        if !stored.account.active {
            return Err(ApplicationError::Unauthorized("usuario desactivado".into()));
        }
        let now = self.clock.now();
        if let Some(locked_until) = stored.locked_until {
            if locked_until > now {
                return Err(ApplicationError::Unauthorized(format!(
                    "cuenta bloqueada hasta {}",
                    locked_until
                        .format(&time::format_description::well_known::Rfc3339)
                        .unwrap_or_default()
                )));
            }
        }
        let Some(hash) = stored.password_hash.as_deref() else {
            return Err(ApplicationError::Unauthorized(
                "el usuario no tiene contraseña configurada".into(),
            ));
        };
        let valid = self
            .hasher
            .verify(password, hash)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        if !valid {
            let attempts = stored.failed_attempts.saturating_add(1);
            let locked_until = (attempts >= self.max_failed_attempts)
                .then(|| now + Duration::minutes(self.lock_minutes));
            self.users
                .record_login(
                    stored.account.id,
                    attempts,
                    locked_until,
                    stored.last_login_at,
                )
                .await?;
            self.write_audit(
                now,
                AuditAction::LoginFailed,
                &stored.account,
                serde_json::json!({
                    "attempts": attempts,
                    "locked_until": locked_until,
                }),
            )
            .await?;
            return Err(ApplicationError::Unauthorized(
                "credenciales inválidas".into(),
            ));
        }
        self.users
            .record_login(stored.account.id, 0, None, Some(now))
            .await?;
        self.write_audit(
            now,
            AuditAction::Login,
            &stored.account,
            serde_json::json!({"role": stored.account.role}),
        )
        .await?;
        Ok(SessionUser {
            session_id: uuid::Uuid::now_v7(),
            user: stored.account,
            started_at: now,
        })
    }

    /// Initializes the password of a user only when it has none yet. This is
    /// used by the demo/bootstrap flow and is safe to call repeatedly.
    pub async fn bootstrap_password(&self, username: &str, password: &str) -> Result<()> {
        let username = username.trim().to_lowercase();
        if let Some(stored) = self.users.find_by_username(&username).await? {
            if stored.password_hash.is_none() {
                self.set_password(stored.account.id, password).await?;
            }
            return Ok(());
        }
        let user = UserAccount::new(UserId::new(), &username, "Administrador", Role::Admin)?;
        self.create_user(&user, password).await
    }

    pub async fn create_user(&self, user: &UserAccount, password: &str) -> Result<()> {
        self.validate_password(password)?;
        let hash = self
            .hasher
            .hash(password)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        self.users.create_user(user, &hash).await?;
        let now = self.clock.now();
        self.write_audit(
            now,
            AuditAction::UserCreated,
            user,
            serde_json::json!({"role": user.role}),
        )
        .await
    }

    pub async fn change_password(
        &self,
        user_id: UserId,
        current_password: &str,
        new_password: &str,
    ) -> Result<()> {
        let stored = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| ApplicationError::NotFound(format!("user {user_id}")))?;
        let Some(hash) = stored.password_hash.as_deref() else {
            return Err(ApplicationError::Unauthorized(
                "el usuario no tiene contraseña configurada".into(),
            ));
        };
        let valid = self
            .hasher
            .verify(current_password, hash)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        if !valid {
            return Err(ApplicationError::Unauthorized(
                "la contraseña actual no es correcta".into(),
            ));
        }
        self.set_password(user_id, new_password).await?;
        let now = self.clock.now();
        self.write_audit(
            now,
            AuditAction::PasswordChanged,
            &stored.account,
            serde_json::json!({}),
        )
        .await
    }

    pub async fn list_users(&self) -> Result<Vec<UserAccount>> {
        self.users.list_users().await
    }

    async fn set_password(&self, user_id: UserId, password: &str) -> Result<()> {
        self.validate_password(password)?;
        let hash = self
            .hasher
            .hash(password)
            .map_err(|error| ApplicationError::Internal(error.to_string()))?;
        self.users.set_password_hash(user_id, &hash).await
    }

    fn validate_password(&self, password: &str) -> Result<()> {
        if password.chars().count() < MIN_PASSWORD_LENGTH {
            return Err(ApplicationError::Validation(format!(
                "la contraseña debe tener al menos {MIN_PASSWORD_LENGTH} caracteres"
            )));
        }
        Ok(())
    }

    async fn write_audit(
        &self,
        now: OffsetDateTime,
        action: AuditAction,
        user: &UserAccount,
        details: serde_json::Value,
    ) -> Result<()> {
        let event = AuditEvent::new(
            now,
            Some(user.id),
            None,
            action,
            "user",
            user.id.to_string(),
            details,
        )?;
        self.audit.append(&event).await
    }
}
