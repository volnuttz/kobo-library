use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub shelves_dir: PathBuf,
    pub kepubify_bin: PathBuf,
    pub max_upload_bytes: usize,
    pub max_books_per_shelf: i64,
    pub max_shelf_bytes: i64,
    pub max_service_bytes: i64,
    pub max_archive_entries: usize,
    pub max_decompressed_bytes: u64,
    pub conversion_timeout: std::time::Duration,
    pub conversion_slots: Arc<tokio::sync::Semaphore>,
    pub upload_slots: Arc<tokio::sync::Semaphore>,
    pub download_slots: Arc<tokio::sync::Semaphore>,
    pub public_base_url: Option<String>,
    pub shelf_access_code: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let port = env::var("PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(3001);
        let data_dir = env::var_os("DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("data"));
        let max_upload_mb = env::var("MAX_UPLOAD_MB")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(100);
        let conversion_concurrency = env::var("CONVERSION_CONCURRENCY")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(2);
        let kepubify_bin = env::var_os("KEPUBIFY_BIN")
            .map(|value| absolute_path(&cwd, PathBuf::from(value)))
            .unwrap_or_else(|| {
                let local = cwd.join("bin").join("kepubify");
                if local.exists() {
                    local
                } else {
                    PathBuf::from("kepubify")
                }
            });
        let public_base_url = env::var("PUBLIC_BASE_URL")
            .ok()
            .map(|value| value.trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());
        let shelf_access_code = env::var("SHELF_ACCESS_CODE")
            .ok()
            .filter(|value| !value.is_empty());

        Self {
            port,
            database_path: data_dir.join("library.sqlite3"),
            shelves_dir: data_dir.join("shelves"),
            data_dir,
            kepubify_bin,
            max_upload_bytes: max_upload_mb * 1024 * 1024,
            max_books_per_shelf: 20,
            max_shelf_bytes: 500 * 1024 * 1024,
            max_service_bytes: 10 * 1024 * 1024 * 1024,
            max_archive_entries: 10_000,
            max_decompressed_bytes: 500 * 1024 * 1024,
            conversion_timeout: std::time::Duration::from_secs(300),
            conversion_slots: Arc::new(tokio::sync::Semaphore::new(conversion_concurrency)),
            upload_slots: Arc::new(tokio::sync::Semaphore::new(8)),
            download_slots: Arc::new(tokio::sync::Semaphore::new(32)),
            public_base_url,
            shelf_access_code,
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        let Some(base) = &self.public_base_url else {
            return Ok(());
        };
        let url = url::Url::parse(base)?;
        anyhow::ensure!(
            url.scheme() == "https" && url.host_str().is_some(),
            "PUBLIC_BASE_URL must be an absolute HTTPS URL"
        );
        anyhow::ensure!(
            self.shelf_access_code.is_some(),
            "SHELF_ACCESS_CODE is required when PUBLIC_BASE_URL is configured"
        );
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn for_test_with_public_url(public_base_url: &str) -> Self {
        Self::for_test(
            std::env::temp_dir().join("epub-drop-config-test"),
            Some(public_base_url),
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        data_dir: PathBuf,
        public_base_url: Option<&str>,
        shelf_access_code: Option<&str>,
    ) -> Self {
        Self {
            port: 3001,
            database_path: data_dir.join("library.sqlite3"),
            shelves_dir: data_dir.join("shelves"),
            data_dir,
            kepubify_bin: PathBuf::from("kepubify"),
            max_upload_bytes: 100 * 1024 * 1024,
            max_books_per_shelf: 20,
            max_shelf_bytes: 500 * 1024 * 1024,
            max_service_bytes: 10 * 1024 * 1024 * 1024,
            max_archive_entries: 10_000,
            max_decompressed_bytes: 500 * 1024 * 1024,
            conversion_timeout: std::time::Duration::from_secs(300),
            conversion_slots: Arc::new(tokio::sync::Semaphore::new(2)),
            upload_slots: Arc::new(tokio::sync::Semaphore::new(8)),
            download_slots: Arc::new(tokio::sync::Semaphore::new(32)),
            public_base_url: public_base_url.map(str::to_string),
            shelf_access_code: shelf_access_code.map(str::to_string),
        }
    }
}

fn absolute_path(cwd: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_configuration_requires_https_and_access_code() {
        let root = std::env::temp_dir().join("kobo-config-validation");
        assert!(
            Config::for_test(root.clone(), Some("http://example.test"), Some("code"))
                .validate()
                .is_err()
        );
        assert!(
            Config::for_test(root.clone(), Some("https://example.test"), None)
                .validate()
                .is_err()
        );
        assert!(
            Config::for_test(root, Some("https://example.test"), Some("code"))
                .validate()
                .is_ok()
        );
    }
}
