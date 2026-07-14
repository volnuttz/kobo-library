use std::{
    fs::File,
    io::{Read, Seek},
    path::{Path, PathBuf},
};

use tokio::task;
use zip::ZipArchive;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Default)]
pub struct BookMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
}

pub async fn extract_metadata(path: &Path) -> AppResult<BookMetadata> {
    let path = PathBuf::from(path);
    task::spawn_blocking(move || extract_metadata_sync(&path))
        .await
        .map_err(AppError::internal)?
}

pub async fn validate_archive(
    path: &Path,
    max_entries: usize,
    max_decompressed_bytes: u64,
) -> AppResult<()> {
    let path = PathBuf::from(path);
    task::spawn_blocking(move || {
        let file = File::open(path).map_err(AppError::internal)?;
        let mut archive = ZipArchive::new(file)
            .map_err(|_| AppError::bad_request("The upload is not a valid EPUB archive."))?;
        if archive.len() > max_entries {
            return Err(AppError::bad_request("The EPUB contains too many files."));
        }
        let mut total = 0_u64;
        for index in 0..archive.len() {
            total =
                total.saturating_add(archive.by_index(index).map_err(AppError::internal)?.size());
            if total > max_decompressed_bytes {
                return Err(AppError::bad_request(
                    "The EPUB expands beyond the allowed size.",
                ));
            }
        }
        Ok(())
    })
    .await
    .map_err(AppError::internal)?
}

pub async fn is_kepub(path: &Path) -> AppResult<bool> {
    let path = PathBuf::from(path);
    task::spawn_blocking(move || is_kepub_sync(&path))
        .await
        .map_err(AppError::internal)?
}

fn extract_metadata_sync(path: &Path) -> AppResult<BookMetadata> {
    let file = File::open(path).map_err(AppError::internal)?;
    let mut archive = ZipArchive::new(file).map_err(AppError::internal)?;
    extract_metadata_from_archive(&mut archive)
}

fn is_kepub_sync(path: &Path) -> AppResult<bool> {
    let file = File::open(path).map_err(AppError::internal)?;
    let mut archive = ZipArchive::new(file).map_err(AppError::internal)?;
    archive_contains_kepub_markers(&mut archive)
}

fn extract_metadata_from_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
) -> AppResult<BookMetadata> {
    let mut container = String::new();
    let mut container_entry = archive
        .by_name("META-INF/container.xml")
        .map_err(AppError::internal)?;
    if container_entry.size() > 1024 * 1024 {
        return Err(AppError::bad_request(
            "EPUB container metadata is too large.",
        ));
    }
    container_entry
        .read_to_string(&mut container)
        .map_err(AppError::internal)?;
    drop(container_entry);

    let container_doc = roxmltree::Document::parse(&container).map_err(AppError::internal)?;
    let opf_path = container_doc
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))
        .and_then(|node| node.attribute("full-path"))
        .ok_or_else(|| AppError::bad_request("EPUB is missing OPF metadata path."))?;

    let mut opf = String::new();
    let mut opf_entry = archive.by_name(opf_path).map_err(AppError::internal)?;
    if opf_entry.size() > 2 * 1024 * 1024 {
        return Err(AppError::bad_request("EPUB package metadata is too large."));
    }
    opf_entry
        .read_to_string(&mut opf)
        .map_err(AppError::internal)?;

    let opf_doc = roxmltree::Document::parse(&opf).map_err(AppError::internal)?;
    Ok(BookMetadata {
        title: text_for_tag(&opf_doc, "title"),
        author: text_for_tag(&opf_doc, "creator"),
    })
}

fn text_for_tag(doc: &roxmltree::Document, tag_name: &str) -> Option<String> {
    doc.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == tag_name)
        .and_then(|node| node.text())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn archive_contains_kepub_markers<R: Read + Seek>(archive: &mut ZipArchive<R>) -> AppResult<bool> {
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(AppError::internal)?;
        let name = entry.name().to_ascii_lowercase();
        if !(name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm")) {
            continue;
        }
        if entry.size() > 2 * 1024 * 1024 {
            continue;
        }

        let mut contents = String::new();
        entry
            .read_to_string(&mut contents)
            .map_err(AppError::internal)?;
        if contents.contains("koboSpan") || contents.contains("id=\"kobo.") {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    #[test]
    fn extracts_title_and_author() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();

        writer
            .start_file("META-INF/container.xml", options)
            .unwrap();
        writer.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();

        writer.start_file("OEBPS/content.opf", options).unwrap();
        writer.write_all(br#"<?xml version="1.0"?><package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>The Left Hand of Darkness</dc:title><dc:creator>Ursula K. Le Guin</dc:creator></metadata></package>"#).unwrap();

        let cursor = writer.finish().unwrap();
        let mut archive = ZipArchive::new(Cursor::new(cursor.into_inner())).unwrap();
        let metadata = extract_metadata_from_archive(&mut archive).unwrap();

        assert_eq!(metadata.title.as_deref(), Some("The Left Hand of Darkness"));
        assert_eq!(metadata.author.as_deref(), Some("Ursula K. Le Guin"));
    }

    #[test]
    fn detects_kepub_markers() {
        let cursor = Cursor::new(Vec::<u8>::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();

        writer.start_file("chapter.xhtml", options).unwrap();
        writer
            .write_all(
                br#"<html><body><span class="koboSpan" id="kobo.1.1">Text</span></body></html>"#,
            )
            .unwrap();

        let cursor = writer.finish().unwrap();
        let mut archive = ZipArchive::new(Cursor::new(cursor.into_inner())).unwrap();

        assert!(archive_contains_kepub_markers(&mut archive).unwrap());
    }

    #[tokio::test]
    async fn rejects_excessive_entries_and_expansion() {
        let path = std::env::temp_dir().join(format!("epub-limit-{}.epub", uuid::Uuid::new_v4()));
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("one", options).unwrap();
        writer.write_all(b"1234").unwrap();
        writer.start_file("two", options).unwrap();
        writer.write_all(b"5678").unwrap();
        writer.finish().unwrap();

        assert!(validate_archive(&path, 1, 100).await.is_err());
        assert!(validate_archive(&path, 10, 7).await.is_err());
        std::fs::remove_file(path).unwrap();
    }
}
