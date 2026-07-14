use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, PartialEq, Eq)]
pub struct Book {
    pub id: String,
    pub shelf_id: String,
    pub status: String,
    pub title: String,
    pub author: Option<String>,
    pub filename: String,
    pub original_name: String,
    pub stored_filename: String,
    pub size: i64,
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
