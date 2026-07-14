use std::path::Path;

use chrono::Utc;
use tokio::fs;
use uuid::Uuid;

use crate::{
    books::{Book, kobo_filename, title_from_filename},
    config::Config,
    conversion::run_kepubify,
    epub,
    error::{AppError, AppResult},
    repository::BookRepository,
    storage::{Storage, remove_file_if_exists},
};

pub async fn store_upload(
    config: &Config,
    repository: &impl BookRepository,
    storage: &Storage,
    shelf_id: &str,
    upload_path: &Path,
    original_name: &str,
) -> AppResult<Book> {
    epub::validate_archive(
        upload_path,
        config.max_archive_entries,
        config.max_decompressed_bytes,
    )
    .await?;
    let metadata = epub::extract_metadata(upload_path)
        .await
        .unwrap_or_default();
    let id = Uuid::new_v4().to_string();
    let filename = kobo_filename(original_name);
    let stored_filename = format!("{id}.kepub.epub");
    let final_path = storage.book_path(shelf_id, &id)?;
    let staged_path = upload_path.with_extension("ready.kepub.epub");

    let book = Book {
        id,
        shelf_id: shelf_id.to_string(),
        status: "pending".to_string(),
        title: metadata
            .title
            .unwrap_or_else(|| title_from_filename(&filename)),
        author: metadata.author,
        filename,
        original_name: original_name.to_string(),
        stored_filename,
        size: 0,
        uploaded_at: Utc::now(),
    };
    if !repository
        .reserve_pending(&book, config.max_books_per_shelf)
        .await?
    {
        return Err(AppError::payload_too_large(
            "This shelf has reached its book limit.",
        ));
    }

    let conversion_result = if should_skip_conversion(upload_path, original_name).await {
        fs::rename(upload_path, &staged_path)
            .await
            .map_err(AppError::internal)
    } else {
        match run_kepubify(config, upload_path, &staged_path).await {
            Ok(()) => remove_file_if_exists(upload_path).await,
            Err(error) => Err(error),
        }
    };
    if let Err(error) = conversion_result {
        let _ = remove_file_if_exists(&staged_path).await;
        repository.discard_pending(shelf_id, &book.id).await?;
        return Err(error);
    }

    if let Err(error) = fs::rename(&staged_path, &final_path).await {
        return Err(AppError::internal(error));
    }
    let size = fs::metadata(&final_path)
        .await
        .map_err(AppError::internal)?
        .len() as i64;
    if size > config.max_upload_bytes as i64 {
        remove_file_if_exists(&final_path).await?;
        repository.discard_pending(shelf_id, &book.id).await?;
        return Err(AppError::payload_too_large(
            "The converted EPUB exceeds the file size limit.",
        ));
    }
    if !repository
        .finalize_book_with_quotas(
            shelf_id,
            &book.id,
            size,
            config.max_shelf_bytes,
            config.max_service_bytes,
        )
        .await?
    {
        remove_file_if_exists(&final_path).await?;
        repository.discard_pending(shelf_id, &book.id).await?;
        return Err(AppError::payload_too_large(
            "Publishing this book would exceed a storage quota.",
        ));
    }

    Ok(Book {
        size,
        status: "ready".to_string(),
        ..book
    })
}

pub async fn delete_book(
    repository: &impl BookRepository,
    storage: &Storage,
    shelf_id: &str,
    book_id: &str,
) -> AppResult<bool> {
    let Some(book) = repository.mark_deleting(shelf_id, book_id).await? else {
        return Ok(false);
    };
    remove_file_if_exists(&storage.book_path(shelf_id, &book.id)?).await?;
    repository.finish_deleting(shelf_id, book_id).await?;
    Ok(true)
}

pub async fn reconcile_incomplete(
    repository: &impl BookRepository,
    storage: &Storage,
    max_file_bytes: i64,
    max_shelf_bytes: i64,
    max_service_bytes: i64,
) -> AppResult<()> {
    for book in repository.incomplete_books().await? {
        let path = storage.book_path(&book.shelf_id, &book.id)?;
        match book.status.as_str() {
            "pending" => match fs::metadata(&path).await {
                Ok(metadata) => {
                    let size = metadata.len() as i64;
                    if size > max_file_bytes
                        || !repository
                            .finalize_book_with_quotas(
                                &book.shelf_id,
                                &book.id,
                                size,
                                max_shelf_bytes,
                                max_service_bytes,
                            )
                            .await?
                    {
                        remove_file_if_exists(&path).await?;
                        repository.discard_pending(&book.shelf_id, &book.id).await?;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    repository.discard_pending(&book.shelf_id, &book.id).await?;
                }
                Err(error) => return Err(AppError::internal(error)),
            },
            "deleting" => {
                remove_file_if_exists(&path).await?;
                repository.finish_deleting(&book.shelf_id, &book.id).await?;
            }
            _ => return Err(AppError::internal("unknown book persistence state")),
        }
    }
    Ok(())
}

async fn should_skip_conversion(upload_path: &Path, original_name: &str) -> bool {
    original_name.to_ascii_lowercase().ends_with(".kepub.epub")
        || epub::is_kepub(upload_path).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::{Database, ShelfRepository};

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

    fn pending_book(shelf_id: &str, book_id: &str, status: &str) -> Book {
        Book {
            id: book_id.to_string(),
            shelf_id: shelf_id.to_string(),
            status: status.to_string(),
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
    async fn reconciliation_finishes_interrupted_publication() {
        let database = Database::memory().await.unwrap();
        let root = std::env::temp_dir().join(format!("kobo-library-{}", Uuid::new_v4()));
        let storage = Storage::new(root.clone());
        let shelf_id = Uuid::new_v4().to_string();
        let book_id = Uuid::new_v4().to_string();
        create_test_shelf(&database, &shelf_id).await;
        storage.prepare_shelf(&shelf_id).await.unwrap();
        database
            .insert_pending(&pending_book(&shelf_id, &book_id, "pending"))
            .await
            .unwrap();
        fs::write(storage.book_path(&shelf_id, &book_id).unwrap(), b"kepub")
            .await
            .unwrap();

        reconcile_incomplete(&database, &storage, 100, 500, 1_000)
            .await
            .unwrap();

        let book = database.book(&shelf_id, &book_id).await.unwrap().unwrap();
        assert_eq!(book.size, 5);
        let revision: i64 = sqlx::query_scalar("SELECT revision FROM shelves WHERE id = ?")
            .bind(&shelf_id)
            .fetch_one(&database.pool)
            .await
            .unwrap();
        assert_eq!(revision, 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn reconciliation_retries_interrupted_deletion() {
        let database = Database::memory().await.unwrap();
        let root = std::env::temp_dir().join(format!("kobo-library-{}", Uuid::new_v4()));
        let storage = Storage::new(root.clone());
        let shelf_id = Uuid::new_v4().to_string();
        let book_id = Uuid::new_v4().to_string();
        create_test_shelf(&database, &shelf_id).await;
        storage.prepare_shelf(&shelf_id).await.unwrap();
        database
            .insert_pending(&pending_book(&shelf_id, &book_id, "pending"))
            .await
            .unwrap();
        database
            .finalize_book(&shelf_id, &book_id, 5)
            .await
            .unwrap();
        fs::write(storage.book_path(&shelf_id, &book_id).unwrap(), b"kepub")
            .await
            .unwrap();
        database.mark_deleting(&shelf_id, &book_id).await.unwrap();

        reconcile_incomplete(&database, &storage, 100, 500, 1_000)
            .await
            .unwrap();

        assert!(database.book(&shelf_id, &book_id).await.unwrap().is_none());
        assert!(!storage.book_path(&shelf_id, &book_id).unwrap().exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
