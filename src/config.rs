use std::{
    env,
    path::{Path, PathBuf},
};

pub struct Config {
    pub port: u16,
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
    pub shelves_dir: PathBuf,
    pub kepubify_bin: PathBuf,
    pub max_upload_bytes: usize,
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
            .unwrap_or(800);
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

        Self {
            port,
            database_path: data_dir.join("library.sqlite3"),
            shelves_dir: data_dir.join("shelves"),
            data_dir,
            kepubify_bin,
            max_upload_bytes: max_upload_mb * 1024 * 1024,
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
