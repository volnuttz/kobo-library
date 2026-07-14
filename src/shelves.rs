use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

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
const ABANDONED_UPLOAD_AGE: Duration = Duration::hours(1);

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

#[derive(Debug, Clone, Copy)]
pub enum OperationKind {
    Mutation,
    Download,
}

pub struct OperationGuard {
    shelf_id: String,
    operation_id: Uuid,
    operations: Arc<Mutex<HashMap<String, Vec<TrackedOperation>>>>,
    deadline: Option<DateTime<Utc>>,
    clock: Arc<dyn Clock>,
}

impl OperationGuard {
    pub fn deadline_reached(&self) -> bool {
        self.deadline
            .is_some_and(|deadline| self.clock.now() >= deadline)
    }
}

impl Drop for OperationGuard {
    fn drop(&mut self) {
        let mut operations = self.operations.lock().unwrap();
        if let Some(shelf_operations) = operations.get_mut(&self.shelf_id) {
            shelf_operations.retain(|operation| operation.id != self.operation_id);
            if shelf_operations.is_empty() {
                operations.remove(&self.shelf_id);
            }
        }
    }
}

#[derive(Debug)]
struct TrackedOperation {
    id: Uuid,
    deadline: Option<DateTime<Utc>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanupStats {
    pub shelves_removed: u64,
    pub uploads_removed: u64,
    pub busy_shelves: u64,
}

#[derive(Clone)]
pub struct ShelfService {
    repository: Arc<dyn ShelfRepository>,
    storage: Arc<Storage>,
    clock: Arc<dyn Clock>,
    operations: Arc<Mutex<HashMap<String, Vec<TrackedOperation>>>>,
    lifecycle: Arc<tokio::sync::Mutex<()>>,
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
            operations: Arc::new(Mutex::new(HashMap::new())),
            lifecycle: Arc::new(tokio::sync::Mutex::new(())),
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
        self.authorize_inner(token, meaningful_activity).await
    }

    pub async fn authorize_operation(
        &self,
        token: &str,
        kind: OperationKind,
    ) -> AppResult<(Shelf, OperationGuard)> {
        let _lifecycle = self.lifecycle.lock().await;
        let shelf = self.authorize_inner(token, true).await?;
        let deadline = match kind {
            OperationKind::Mutation => None,
            OperationKind::Download => {
                Some(std::cmp::min(shelf.expires_at, shelf.hard_expires_at) + Duration::minutes(5))
            }
        };
        let operation_id = Uuid::new_v4();
        self.operations
            .lock()
            .unwrap()
            .entry(shelf.id.clone())
            .or_default()
            .push(TrackedOperation {
                id: operation_id,
                deadline,
            });
        let guard = OperationGuard {
            shelf_id: shelf.id.clone(),
            operation_id,
            operations: self.operations.clone(),
            deadline,
            clock: self.clock.clone(),
        };
        Ok((shelf, guard))
    }

    pub async fn cleanup_once(&self) -> AppResult<CleanupStats> {
        let _lifecycle = self.lifecycle.lock().await;
        let now = self.clock.now();
        let mut stats = CleanupStats::default();
        let busy_shelves: HashSet<String> = self
            .operations
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, operations)| {
                operations
                    .iter()
                    .any(|operation| operation.deadline.is_none_or(|deadline| deadline > now))
            })
            .map(|(shelf_id, _)| shelf_id.clone())
            .collect();
        for candidate in self.repository.cleanup_candidates(now).await? {
            if busy_shelves.contains(&candidate.id) {
                stats.busy_shelves += 1;
                continue;
            }
            if candidate.state == "active"
                && !self.repository.claim_expiring(&candidate.id, now).await?
            {
                continue;
            }
            self.storage.remove_shelf(&candidate.id).await?;
            self.repository.delete_expiring(&candidate.id).await?;
            stats.shelves_removed += 1;
        }
        stats.uploads_removed = self
            .storage
            .sweep_stale_uploads(now - ABANDONED_UPLOAD_AGE, &busy_shelves)
            .await?;
        Ok(stats)
    }

    async fn authorize_inner(&self, token: &str, meaningful_activity: bool) -> AppResult<Shelf> {
        let supplied_hash = hash_token(token);
        let Some(mut shelf) = self.repository.shelf_by_token_hash(&supplied_hash).await? else {
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
            shelf.expires_at = expires_at;
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

    #[tokio::test]
    async fn expiry_boundary_is_inaccessible_and_cleanup_is_idempotent() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, database, root, clock) = fixture(now).await;
        let created = service.create().await.unwrap();
        let shelf = service.authorize(&created.token, false).await.unwrap();
        let book_id = Uuid::new_v4().to_string();
        let book_path = service.storage.book_path(&shelf.id, &book_id).unwrap();
        tokio::fs::write(&book_path, b"book").await.unwrap();
        sqlx::query("INSERT INTO books (id, shelf_id, status, title, filename, original_name, stored_filename, size, uploaded_at) VALUES (?, ?, 'ready', 'Book', 'Book.kepub.epub', 'Book.epub', ?, 4, ?)")
            .bind(&book_id).bind(&shelf.id).bind(format!("{book_id}.kepub.epub")).bind(now)
            .execute(&database.pool).await.unwrap();
        *clock.0.lock().unwrap() = now + Duration::hours(12) - Duration::milliseconds(1);
        assert!(service.authorize(&created.token, false).await.is_ok());
        *clock.0.lock().unwrap() = now + Duration::hours(12);
        assert!(service.authorize(&created.token, false).await.is_err());

        let first = service.cleanup_once().await.unwrap();
        assert_eq!(first.shelves_removed, 1);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelves")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let book_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(book_count, 0);
        assert!(!book_path.exists());
        assert_eq!(service.cleanup_once().await.unwrap().shelves_removed, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn active_mutation_defers_cleanup_and_expiring_rejects_new_work() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, _, root, clock) = fixture(now).await;
        let created = service.create().await.unwrap();
        let (_, operation) = service
            .authorize_operation(&created.token, OperationKind::Mutation)
            .await
            .unwrap();
        *clock.0.lock().unwrap() = now + Duration::hours(24);

        let busy = service.cleanup_once().await.unwrap();
        assert_eq!(busy.busy_shelves, 1);
        assert_eq!(busy.shelves_removed, 0);
        assert!(
            service
                .authorize_operation(&created.token, OperationKind::Mutation)
                .await
                .is_err()
        );

        drop(operation);
        assert_eq!(service.cleanup_once().await.unwrap().shelves_removed, 1);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn restart_retries_an_already_expiring_shelf() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, database, root, _) = fixture(now).await;
        let created = service.create().await.unwrap();
        let shelf = service.authorize(&created.token, false).await.unwrap();
        sqlx::query("UPDATE shelves SET state = 'expiring' WHERE id = ?")
            .bind(&shelf.id)
            .execute(&database.pool)
            .await
            .unwrap();
        service.storage.remove_shelf(&shelf.id).await.unwrap();

        assert_eq!(service.cleanup_once().await.unwrap().shelves_removed, 1);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shelves")
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn download_guard_ends_five_minutes_after_expiry() {
        let now: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let (service, _, root, clock) = fixture(now).await;
        let created = service.create().await.unwrap();
        let (_, operation) = service
            .authorize_operation(&created.token, OperationKind::Download)
            .await
            .unwrap();
        *clock.0.lock().unwrap() = now + Duration::hours(12) + Duration::minutes(5);
        assert!(operation.deadline_reached());
        assert_eq!(service.cleanup_once().await.unwrap().shelves_removed, 1);
        drop(operation);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn stale_uploads_are_removed_from_active_shelves() {
        let now = Utc::now() + Duration::hours(2);
        let (service, _, root, _) = fixture(now).await;
        let created = service.create().await.unwrap();
        let shelf = service.authorize(&created.token, false).await.unwrap();
        let upload = service.storage.new_upload_path(&shelf.id).unwrap();
        tokio::fs::write(&upload, b"partial").await.unwrap();

        let stats = service.cleanup_once().await.unwrap();
        assert_eq!(stats.shelves_removed, 0);
        assert_eq!(stats.uploads_removed, 1);
        assert!(!upload.exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn stale_sweep_skips_a_shelf_with_active_work() {
        let now = Utc::now() + Duration::hours(2);
        let (service, _, root, _) = fixture(now).await;
        let created = service.create().await.unwrap();
        let (shelf, operation) = service
            .authorize_operation(&created.token, OperationKind::Mutation)
            .await
            .unwrap();
        let upload = service.storage.new_upload_path(&shelf.id).unwrap();
        tokio::fs::write(&upload, b"active").await.unwrap();

        assert_eq!(service.cleanup_once().await.unwrap().uploads_removed, 0);
        assert!(upload.exists());
        drop(operation);
        assert_eq!(service.cleanup_once().await.unwrap().uploads_removed, 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
