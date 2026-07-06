use std::path::Path;

use chrono::Utc;
use tokio::fs;
use uuid::Uuid;

use crate::{
    books::{
        Book, kobo_filename, read_books, remove_file_if_exists, title_from_filename, write_books,
    },
    config::Config,
    conversion::run_kepubify,
    epub,
    error::{AppError, AppResult},
};

pub async fn store_upload(
    config: &Config,
    upload_path: &Path,
    original_name: &str,
) -> AppResult<Book> {
    let metadata = epub::extract_metadata(upload_path)
        .await
        .unwrap_or_default();
    let id = Uuid::new_v4().to_string();
    let filename = kobo_filename(original_name);
    let stored_filename = format!("{id}-{filename}");
    let output_path = config.books_dir.join(&stored_filename);

    if should_skip_conversion(upload_path, original_name).await {
        fs::rename(upload_path, &output_path)
            .await
            .map_err(AppError::internal)?;
    } else {
        let temp_output_path = upload_path.with_extension("kepub.epub");
        run_kepubify(config, upload_path, &temp_output_path).await?;
        fs::rename(&temp_output_path, &output_path)
            .await
            .map_err(AppError::internal)?;
        remove_file_if_exists(upload_path).await?;
    }

    let size = fs::metadata(&output_path)
        .await
        .map_err(AppError::internal)?
        .len();

    let book = Book {
        id,
        title: metadata
            .title
            .unwrap_or_else(|| title_from_filename(&filename)),
        author: metadata.author,
        filename,
        original_name: original_name.to_string(),
        stored_filename,
        size,
        uploaded_at: Utc::now(),
    };

    let mut books = read_books(config).await?;
    books.push(book.clone());
    write_books(config, &books).await?;

    Ok(book)
}

async fn should_skip_conversion(upload_path: &Path, original_name: &str) -> bool {
    original_name.to_ascii_lowercase().ends_with(".kepub.epub")
        || epub::is_kepub(upload_path).await.unwrap_or(false)
}
