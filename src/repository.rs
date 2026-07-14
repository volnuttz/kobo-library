use std::{path::Path, str::FromStr};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::{
    books::Book,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Shelf {
    pub id: String,
    pub token_hash: Vec<u8>,
    pub state: String,
    pub revision: i64,
    pub expires_at: DateTime<Utc>,
    pub hard_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CleanupShelf {
    pub id: String,
    pub state: String,
}

#[derive(Debug)]
pub struct ServiceMetrics {
    pub active_shelves: i64,
    pub stored_bytes: i64,
    pub cleanup_lag_seconds: i64,
}

#[async_trait]
pub trait ShelfRepository: Send + Sync {
    async fn create_shelf(
        &self,
        id: &str,
        token_hash: &[u8],
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        hard_expires_at: DateTime<Utc>,
    ) -> AppResult<()>;
    async fn shelf_by_token_hash(&self, token_hash: &[u8]) -> AppResult<Option<Shelf>>;
    async fn touch_activity(
        &self,
        shelf_id: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> AppResult<()>;
    async fn cleanup_candidates(&self, now: DateTime<Utc>) -> AppResult<Vec<CleanupShelf>>;
    async fn claim_expiring(&self, shelf_id: &str, now: DateTime<Utc>) -> AppResult<bool>;
    async fn delete_expiring(&self, shelf_id: &str) -> AppResult<()>;
    async fn service_metrics(&self, now: DateTime<Utc>) -> AppResult<ServiceMetrics>;
}

#[async_trait]
pub trait BookRepository: Send + Sync {
    async fn list_books(&self, shelf_id: &str) -> AppResult<Vec<Book>>;
    async fn book(&self, shelf_id: &str, book_id: &str) -> AppResult<Option<Book>>;
    #[cfg_attr(not(test), allow(dead_code))]
    async fn insert_pending(&self, book: &Book) -> AppResult<()>;
    async fn reserve_pending(&self, book: &Book, max_books: i64) -> AppResult<bool>;
    #[cfg_attr(not(test), allow(dead_code))]
    async fn finalize_book(&self, shelf_id: &str, book_id: &str, size: i64) -> AppResult<bool>;
    async fn finalize_book_with_quotas(
        &self,
        shelf_id: &str,
        book_id: &str,
        size: i64,
        max_shelf_bytes: i64,
        max_service_bytes: i64,
    ) -> AppResult<bool>;
    async fn mark_deleting(&self, shelf_id: &str, book_id: &str) -> AppResult<Option<Book>>;
    async fn finish_deleting(&self, shelf_id: &str, book_id: &str) -> AppResult<()>;
    async fn discard_pending(&self, shelf_id: &str, book_id: &str) -> AppResult<()>;
    async fn incomplete_books(&self) -> AppResult<Vec<Book>>;
}

#[derive(Clone)]
pub struct Database {
    pub(crate) pool: SqlitePool,
}

impl Database {
    pub async fn open(path: &Path) -> AppResult<Self> {
        let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))
            .map_err(AppError::internal)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .map_err(AppError::internal)?;
        sqlx::migrate!()
            .run(&pool)
            .await
            .map_err(AppError::internal)?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub(crate) async fn memory() -> AppResult<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .map_err(AppError::internal)?
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(AppError::internal)?;
        sqlx::migrate!()
            .run(&pool)
            .await
            .map_err(AppError::internal)?;
        Ok(Self { pool })
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use uuid::Uuid;

    async fn create_test_shelf(database: &Database, shelf_id: &str) {
        let now = Utc::now();
        database
            .create_shelf(
                shelf_id,
                Uuid::new_v4().as_bytes(),
                now,
                now + chrono::Duration::hours(12),
                now + chrono::Duration::hours(24),
            )
            .await
            .unwrap();
    }

    fn pending_book(shelf_id: &str, book_id: &str) -> Book {
        Book {
            id: book_id.to_string(),
            shelf_id: shelf_id.to_string(),
            status: "pending".to_string(),
            title: "Book".to_string(),
            author: None,
            filename: "Book.kepub.epub".to_string(),
            original_name: "Book.epub".to_string(),
            stored_filename: format!("{book_id}.kepub.epub"),
            size: 0,
            uploaded_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn book_lookups_are_isolated_by_shelf() {
        let database = Database::memory().await.unwrap();
        let shelf_a = Uuid::new_v4().to_string();
        let shelf_b = Uuid::new_v4().to_string();
        create_test_shelf(&database, &shelf_a).await;
        create_test_shelf(&database, &shelf_b).await;
        let book_id = Uuid::new_v4().to_string();
        database
            .insert_pending(&pending_book(&shelf_a, &book_id))
            .await
            .unwrap();
        database
            .finalize_book(&shelf_a, &book_id, 42)
            .await
            .unwrap();

        assert!(database.book(&shelf_a, &book_id).await.unwrap().is_some());
        assert!(database.book(&shelf_b, &book_id).await.unwrap().is_none());
        assert!(database.list_books(&shelf_b).await.unwrap().is_empty());
        assert!(
            database
                .mark_deleting(&shelf_b, &book_id)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn concurrent_book_publications_do_not_lose_updates() {
        let database = Database::memory().await.unwrap();
        let shelf_id = Uuid::new_v4().to_string();
        create_test_shelf(&database, &shelf_id).await;
        let first_id = Uuid::new_v4().to_string();
        let second_id = Uuid::new_v4().to_string();
        let first = pending_book(&shelf_id, &first_id);
        let second = pending_book(&shelf_id, &second_id);

        let (first_result, second_result) = tokio::join!(
            database.insert_pending(&first),
            database.insert_pending(&second)
        );
        first_result.unwrap();
        second_result.unwrap();
        let (first_result, second_result) = tokio::join!(
            database.finalize_book(&shelf_id, &first_id, 10),
            database.finalize_book(&shelf_id, &second_id, 20)
        );
        assert!(first_result.unwrap());
        assert!(second_result.unwrap());

        assert_eq!(database.list_books(&shelf_id).await.unwrap().len(), 2);
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM shelves WHERE id = ?")
            .bind(&shelf_id)
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(revision, 2);
    }

    #[tokio::test]
    async fn migrations_and_metadata_survive_restart() {
        let path = std::env::temp_dir().join(format!("kobo-library-{}.sqlite3", Uuid::new_v4()));
        let shelf_id = Uuid::new_v4().to_string();
        {
            let database = Database::open(&path).await.unwrap();
            create_test_shelf(&database, &shelf_id).await;
            database.pool.close().await;
        }
        {
            let database = Database::open(&path).await.unwrap();
            let stored_id: String = sqlx::query_scalar("SELECT id FROM shelves WHERE id = ?")
                .bind(&shelf_id)
                .fetch_one(&database.pool)
                .await
                .unwrap();
            assert_eq!(stored_id, shelf_id);
            database.pool.close().await;
        }
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite3-shm"));
        let _ = std::fs::remove_file(path.with_extension("sqlite3-wal"));
    }

    #[tokio::test]
    async fn book_count_and_byte_quotas_are_atomic() {
        let database = Database::memory().await.unwrap();
        let shelf_id = Uuid::new_v4().to_string();
        create_test_shelf(&database, &shelf_id).await;
        let first_id = Uuid::new_v4().to_string();
        let second_id = Uuid::new_v4().to_string();
        let first = pending_book(&shelf_id, &first_id);
        let second = pending_book(&shelf_id, &second_id);

        assert!(database.reserve_pending(&first, 1).await.unwrap());
        assert!(!database.reserve_pending(&second, 1).await.unwrap());
        assert!(
            !database
                .finalize_book_with_quotas(&shelf_id, &first_id, 11, 10, 100)
                .await
                .unwrap()
        );
        assert!(
            database
                .finalize_book_with_quotas(&shelf_id, &first_id, 10, 10, 10)
                .await
                .unwrap()
        );
    }
}

#[async_trait]
impl ShelfRepository for Database {
    async fn create_shelf(
        &self,
        id: &str,
        token_hash: &[u8],
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        hard_expires_at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query("INSERT INTO shelves (id, token_hash, state, revision, created_at, last_seen_at, last_activity_at, expires_at, hard_expires_at) VALUES (?, ?, 'active', 0, ?, ?, ?, ?, ?)")
            .bind(id).bind(token_hash).bind(now).bind(now).bind(now).bind(expires_at).bind(hard_expires_at)
            .execute(&self.pool).await.map_err(AppError::internal)?;
        Ok(())
    }

    async fn shelf_by_token_hash(&self, token_hash: &[u8]) -> AppResult<Option<Shelf>> {
        sqlx::query_as("SELECT id, token_hash, state, revision, expires_at, hard_expires_at FROM shelves WHERE token_hash = ?")
            .bind(token_hash).fetch_optional(&self.pool).await.map_err(AppError::internal)
    }

    async fn touch_activity(
        &self,
        shelf_id: &str,
        now: DateTime<Utc>,
        expires_at: DateTime<Utc>,
    ) -> AppResult<()> {
        sqlx::query("UPDATE shelves SET last_seen_at = ?, last_activity_at = ?, expires_at = ? WHERE id = ? AND state = 'active'")
            .bind(now).bind(now).bind(expires_at).bind(shelf_id)
            .execute(&self.pool).await.map_err(AppError::internal)?;
        Ok(())
    }

    async fn cleanup_candidates(&self, now: DateTime<Utc>) -> AppResult<Vec<CleanupShelf>> {
        sqlx::query_as("SELECT id, state FROM shelves WHERE state = 'expiring' OR (state = 'active' AND (expires_at <= ? OR hard_expires_at <= ?))")
            .bind(now).bind(now).fetch_all(&self.pool).await.map_err(AppError::internal)
    }

    async fn claim_expiring(&self, shelf_id: &str, now: DateTime<Utc>) -> AppResult<bool> {
        let changed = sqlx::query("UPDATE shelves SET state = 'expiring' WHERE id = ? AND state = 'active' AND (expires_at <= ? OR hard_expires_at <= ?)")
            .bind(shelf_id).bind(now).bind(now).execute(&self.pool).await.map_err(AppError::internal)?.rows_affected() == 1;
        Ok(changed)
    }

    async fn delete_expiring(&self, shelf_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM shelves WHERE id = ? AND state = 'expiring'")
            .bind(shelf_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }

    async fn service_metrics(&self, now: DateTime<Utc>) -> AppResult<ServiceMetrics> {
        let active_shelves = sqlx::query_scalar("SELECT COUNT(*) FROM shelves WHERE state = 'active' AND expires_at > ? AND hard_expires_at > ?")
            .bind(now).bind(now).fetch_one(&self.pool).await.map_err(AppError::internal)?;
        let stored_bytes =
            sqlx::query_scalar("SELECT COALESCE(SUM(size), 0) FROM books WHERE status = 'ready'")
                .fetch_one(&self.pool)
                .await
                .map_err(AppError::internal)?;
        let oldest: Option<DateTime<Utc>> = sqlx::query_scalar("SELECT MIN(CASE WHEN expires_at < hard_expires_at THEN expires_at ELSE hard_expires_at END) FROM shelves WHERE state = 'expiring' OR expires_at <= ? OR hard_expires_at <= ?")
            .bind(now).bind(now).fetch_one(&self.pool).await.map_err(AppError::internal)?;
        Ok(ServiceMetrics {
            active_shelves,
            stored_bytes,
            cleanup_lag_seconds: oldest.map_or(0, |expired| (now - expired).num_seconds().max(0)),
        })
    }
}

#[async_trait]
impl BookRepository for Database {
    async fn list_books(&self, shelf_id: &str) -> AppResult<Vec<Book>> {
        sqlx::query_as("SELECT id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at FROM books WHERE shelf_id = ? AND status = 'ready' ORDER BY uploaded_at DESC")
            .bind(shelf_id).fetch_all(&self.pool).await.map_err(AppError::internal)
    }

    async fn book(&self, shelf_id: &str, book_id: &str) -> AppResult<Option<Book>> {
        sqlx::query_as("SELECT id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at FROM books WHERE shelf_id = ? AND id = ? AND status = 'ready'")
            .bind(shelf_id).bind(book_id).fetch_optional(&self.pool).await.map_err(AppError::internal)
    }

    async fn insert_pending(&self, book: &Book) -> AppResult<()> {
        sqlx::query("INSERT INTO books (id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at) VALUES (?, ?, 'pending', ?, ?, ?, ?, ?, 0, ?)")
            .bind(&book.id).bind(&book.shelf_id).bind(&book.title).bind(&book.author)
            .bind(&book.filename).bind(&book.original_name).bind(&book.stored_filename)
            .bind(book.uploaded_at).execute(&self.pool).await.map_err(AppError::internal)?;
        Ok(())
    }

    async fn reserve_pending(&self, book: &Book, max_books: i64) -> AppResult<bool> {
        let changed = sqlx::query("INSERT INTO books (id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at) SELECT ?, ?, 'pending', ?, ?, ?, ?, ?, 0, ? WHERE (SELECT COUNT(*) FROM books WHERE shelf_id = ? AND status != 'deleting') < ?")
            .bind(&book.id).bind(&book.shelf_id).bind(&book.title).bind(&book.author)
            .bind(&book.filename).bind(&book.original_name).bind(&book.stored_filename)
            .bind(book.uploaded_at).bind(&book.shelf_id).bind(max_books)
            .execute(&self.pool).await.map_err(AppError::internal)?.rows_affected() == 1;
        Ok(changed)
    }

    async fn finalize_book(&self, shelf_id: &str, book_id: &str, size: i64) -> AppResult<bool> {
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        let changed = sqlx::query("UPDATE books SET status = 'ready', size = ? WHERE shelf_id = ? AND id = ? AND status = 'pending'")
            .bind(size).bind(shelf_id).bind(book_id).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected() == 1;
        if changed {
            sqlx::query("UPDATE shelves SET revision = revision + 1 WHERE id = ?")
                .bind(shelf_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        }
        tx.commit().await.map_err(AppError::internal)?;
        Ok(changed)
    }

    async fn finalize_book_with_quotas(
        &self,
        shelf_id: &str,
        book_id: &str,
        size: i64,
        max_shelf_bytes: i64,
        max_service_bytes: i64,
    ) -> AppResult<bool> {
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        let changed = sqlx::query("UPDATE books SET status = 'ready', size = ? WHERE shelf_id = ? AND id = ? AND status = 'pending' AND (SELECT COALESCE(SUM(size), 0) FROM books WHERE shelf_id = ? AND status = 'ready') + ? <= ? AND (SELECT COALESCE(SUM(size), 0) FROM books WHERE status = 'ready') + ? <= ?")
            .bind(size).bind(shelf_id).bind(book_id).bind(shelf_id).bind(size)
            .bind(max_shelf_bytes).bind(size).bind(max_service_bytes)
            .execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected() == 1;
        if changed {
            sqlx::query("UPDATE shelves SET revision = revision + 1 WHERE id = ?")
                .bind(shelf_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        }
        tx.commit().await.map_err(AppError::internal)?;
        Ok(changed)
    }

    async fn mark_deleting(&self, shelf_id: &str, book_id: &str) -> AppResult<Option<Book>> {
        let mut tx = self.pool.begin().await.map_err(AppError::internal)?;
        let book: Option<Book> = sqlx::query_as("UPDATE books SET status = 'deleting' WHERE shelf_id = ? AND id = ? AND status = 'ready' RETURNING id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at")
            .bind(shelf_id).bind(book_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if book.is_some() {
            sqlx::query("UPDATE shelves SET revision = revision + 1 WHERE id = ?")
                .bind(shelf_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        }
        tx.commit().await.map_err(AppError::internal)?;
        Ok(book)
    }

    async fn finish_deleting(&self, shelf_id: &str, book_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM books WHERE shelf_id = ? AND id = ? AND status = 'deleting'")
            .bind(shelf_id)
            .bind(book_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }

    async fn incomplete_books(&self) -> AppResult<Vec<Book>> {
        sqlx::query_as("SELECT id, shelf_id, status, title, author, filename, original_name, stored_filename, size, uploaded_at FROM books WHERE status != 'ready'")
            .fetch_all(&self.pool).await.map_err(AppError::internal)
    }

    async fn discard_pending(&self, shelf_id: &str, book_id: &str) -> AppResult<()> {
        sqlx::query("DELETE FROM books WHERE shelf_id = ? AND id = ? AND status = 'pending'")
            .bind(shelf_id)
            .bind(book_id)
            .execute(&self.pool)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }
}
