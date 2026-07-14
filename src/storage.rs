use std::path::{Path, PathBuf};

use tokio::fs;
use uuid::Uuid;

use crate::error::{AppError, AppResult};

#[derive(Clone)]
pub struct Storage {
    shelves_dir: PathBuf,
}

impl Storage {
    pub fn new(shelves_dir: PathBuf) -> Self {
        Self { shelves_dir }
    }

    pub async fn prepare_shelf(&self, shelf_id: &str) -> AppResult<()> {
        fs::create_dir_all(self.books_dir(shelf_id)?)
            .await
            .map_err(AppError::internal)?;
        fs::create_dir_all(self.uploads_dir(shelf_id)?)
            .await
            .map_err(AppError::internal)?;
        Ok(())
    }

    pub fn new_upload_path(&self, shelf_id: &str) -> AppResult<PathBuf> {
        Ok(self
            .uploads_dir(shelf_id)?
            .join(format!("{}.epub", Uuid::new_v4())))
    }

    pub fn book_path(&self, shelf_id: &str, book_id: &str) -> AppResult<PathBuf> {
        validate_id(book_id)?;
        Ok(self
            .books_dir(shelf_id)?
            .join(format!("{book_id}.kepub.epub")))
    }

    fn shelf_dir(&self, shelf_id: &str) -> AppResult<PathBuf> {
        validate_id(shelf_id)?;
        Ok(self.shelves_dir.join(shelf_id))
    }

    fn books_dir(&self, shelf_id: &str) -> AppResult<PathBuf> {
        Ok(self.shelf_dir(shelf_id)?.join("books"))
    }

    fn uploads_dir(&self, shelf_id: &str) -> AppResult<PathBuf> {
        Ok(self.shelf_dir(shelf_id)?.join("uploads"))
    }
}

pub async fn remove_file_if_exists(path: &Path) -> AppResult<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::internal(err)),
    }
}

fn validate_id(id: &str) -> AppResult<()> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| AppError::internal("invalid internal storage id"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_untrusted_storage_components() {
        let storage = Storage::new(PathBuf::from("data/shelves"));
        assert!(
            storage
                .book_path("../other", &Uuid::new_v4().to_string())
                .is_err()
        );
        assert!(
            storage
                .book_path(&Uuid::new_v4().to_string(), "../book")
                .is_err()
        );
    }
}
