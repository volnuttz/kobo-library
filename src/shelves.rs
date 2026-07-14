use std::sync::Arc;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rand::{RngCore, rngs::OsRng};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    repository::{Shelf, ShelfRepository},
    storage::Storage,
};

const INACTIVITY_LIFETIME: Duration = Duration::hours(12);
const MAXIMUM_LIFETIME: Duration = Duration::hours(24);

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug)]
pub struct CreatedShelf {
    pub token: String,
}

#[derive(Clone)]
pub struct ShelfService {
    repository: Arc<dyn ShelfRepository>,
    storage: Arc<Storage>,
    clock: Arc<dyn Clock>,
}

impl ShelfService {
    pub fn new(
        repository: Arc<dyn ShelfRepository>,
        storage: Arc<Storage>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            storage,
            clock,
        }
    }

    pub async fn create(&self) -> AppResult<CreatedShelf> {
        let mut token_bytes = [0_u8; 32];
        OsRng.fill_bytes(&mut token_bytes);
        let token = URL_SAFE_NO_PAD.encode(token_bytes);
        let token_hash = hash_token(&token);
        let shelf_id = Uuid::new_v4().to_string();
        let now = self.clock.now();

        self.storage.prepare_shelf(&shelf_id).await?;
        self.repository
            .create_shelf(
                &shelf_id,
                &token_hash,
                now,
                now + INACTIVITY_LIFETIME,
                now + MAXIMUM_LIFETIME,
            )
            .await?;
        Ok(CreatedShelf { token })
    }

    pub async fn authorize(&self, token: &str, meaningful_activity: bool) -> AppResult<Shelf> {
        let supplied_hash = hash_token(token);
        let Some(shelf) = self.repository.shelf_by_token_hash(&supplied_hash).await? else {
            return Err(unavailable());
        };

        if shelf.token_hash.len() != supplied_hash.len()
            || !bool::from(shelf.token_hash.ct_eq(supplied_hash.as_slice()))
        {
            return Err(unavailable());
        }

        let now = self.clock.now();
        if shelf.state != "active" || now >= shelf.expires_at || now >= shelf.hard_expires_at {
            return Err(unavailable());
        }

        if meaningful_activity {
            let expires_at = std::cmp::min(now + INACTIVITY_LIFETIME, shelf.hard_expires_at);
            self.repository
                .touch_activity(&shelf.id, now, expires_at)
                .await?;
        }
        Ok(shelf)
    }
}

fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

fn unavailable() -> AppError {
    AppError::not_found("Shelf not found or expired.")
}

#[cfg(test)]
pub(crate) mod tests {
    use std::{path::PathBuf, sync::Mutex};

    use super::*;
    use crate::repository::Database;
    use axum::response::IntoResponse;

    pub struct FixedClock(pub Mutex<DateTime<Utc>>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }

    async fn fixture(
        now: DateTime<Utc>,
    ) -> (ShelfService, Arc<Database>, PathBuf, Arc<FixedClock>) {
        let database = Arc::new(Database::memory().await.unwrap());
        let root = std::env::temp_dir().join(format!("kobo-shelves-{}", Uuid::new_v4()));
        let storage = Arc::new(Storage::new(root.clone()));
        let clock = Arc::new(FixedClock(Mutex::new(now)));
        let service = ShelfService::new(database.clone(), storage, clock.clone());
        (service, database, root, clock)
    }

    #[tokio::test]
    async fn creates_high_entropy_capability_and_stores_only_hash() {
        let now = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, database, root, _) = fixture(now).await;

        let created = service.create().await.unwrap();
        let stored: Vec<u8> = sqlx::query_scalar("SELECT token_hash FROM shelves")
            .fetch_one(&database.pool)
            .await
            .unwrap();

        assert_eq!(created.token.len(), 43);
        assert!(
            created
                .token
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        );
        assert_eq!(stored.len(), 32);
        assert_ne!(stored, created.token.as_bytes());
        assert!(service.authorize(&created.token, false).await.is_ok());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn unknown_and_expired_capabilities_have_the_same_error() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, _, root, clock) = fixture(now).await;
        let created = service.create().await.unwrap();
        let unknown = service
            .authorize("not-a-capability", false)
            .await
            .unwrap_err();
        *clock.0.lock().unwrap() = now + Duration::hours(12);
        let expired = service.authorize(&created.token, false).await.unwrap_err();

        assert_eq!(unknown.to_string(), expired.to_string());
        assert_eq!(
            unknown.into_response().status(),
            expired.into_response().status()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn polling_does_not_extend_but_explicit_access_does() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, database, root, clock) = fixture(now).await;
        let created = service.create().await.unwrap();
        *clock.0.lock().unwrap() = now + Duration::hours(11);

        service.authorize(&created.token, false).await.unwrap();
        let unchanged: DateTime<Utc> = sqlx::query_scalar("SELECT expires_at FROM shelves")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(unchanged, now + Duration::hours(12));

        service.authorize(&created.token, true).await.unwrap();
        let extended: DateTime<Utc> = sqlx::query_scalar("SELECT expires_at FROM shelves")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(extended, now + Duration::hours(23));
        std::fs::remove_dir_all(root).unwrap();
    }
}
