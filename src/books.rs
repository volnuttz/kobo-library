use std::{cmp::Reverse, path::Path};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{
    config::Config,
    error::{AppError, AppResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    pub filename: String,
    pub original_name: String,
    pub stored_filename: String,
    pub size: u64,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicBook {
    id: String,
    title: String,
    author: Option<String>,
    download_url: String,
}

impl From<&Book> for PublicBook {
    fn from(book: &Book) -> Self {
        Self {
            id: book.id.clone(),
            title: book.title.clone(),
            author: book.author.clone(),
            download_url: format!("/books/{}/download", book.id),
        }
    }
}

pub async fn public_books(config: &Config) -> AppResult<Vec<PublicBook>> {
    let mut books = read_books(config).await?;
    books.sort_by_key(|book| Reverse(book.uploaded_at));
    Ok(books.iter().map(PublicBook::from).collect())
}

pub async fn read_books(config: &Config) -> AppResult<Vec<Book>> {
    match fs::read_to_string(&config.metadata_path).await {
        Ok(data) => serde_json::from_str(&data).map_err(AppError::internal),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(AppError::internal(err)),
    }
}

pub async fn write_books(config: &Config, books: &[Book]) -> AppResult<()> {
    let tmp_path = config.metadata_path.with_extension("json.tmp");
    let data = serde_json::to_string_pretty(books).map_err(AppError::internal)?;
    fs::write(&tmp_path, format!("{data}\n"))
        .await
        .map_err(AppError::internal)?;
    fs::rename(tmp_path, &config.metadata_path)
        .await
        .map_err(AppError::internal)?;
    Ok(())
}

pub async fn remove_file_if_exists(path: &Path) -> AppResult<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::internal(err)),
    }
}

pub fn kobo_filename(filename: &str) -> String {
    let cleaned = sanitize_filename(filename);
    let base = if cleaned.is_empty() {
        "book.epub".to_string()
    } else {
        cleaned
    };

    if base.to_ascii_lowercase().ends_with(".kepub.epub") {
        base
    } else if base.to_ascii_lowercase().ends_with(".epub") {
        format!("{}.kepub.epub", &base[..base.len() - 5])
    } else {
        format!("{base}.kepub.epub")
    }
}

pub fn title_from_filename(filename: &str) -> String {
    filename
        .strip_suffix(".kepub.epub")
        .unwrap_or(filename)
        .to_string()
}

pub fn header_safe_filename(filename: &str) -> String {
    filename.replace(['"', '\\', '\r', '\n'], "_")
}

fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' | ' ' | '(' | ')' => ch,
            _ => '_',
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_epub_filename_to_kepub() {
        assert_eq!(kobo_filename("Dune.epub"), "Dune.kepub.epub");
    }

    #[test]
    fn keeps_existing_kepub_filename() {
        assert_eq!(kobo_filename("Dune.kepub.epub"), "Dune.kepub.epub");
    }

    #[test]
    fn removes_header_unsafe_characters() {
        assert_eq!(header_safe_filename("bad\"\\\r\n.epub"), "bad____.epub");
    }
}
