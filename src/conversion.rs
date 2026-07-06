use std::path::Path;

use tokio::process::Command;

use crate::{
    config::Config,
    error::{AppError, AppResult},
};

pub async fn run_kepubify(config: &Config, input_path: &Path, output_path: &Path) -> AppResult<()> {
    let output = Command::new(&config.kepubify_bin)
        .arg("-v")
        .arg("-u")
        .arg("-o")
        .arg(output_path.file_name().unwrap_or_default())
        .arg(input_path.file_name().unwrap_or_default())
        .current_dir(input_path.parent().unwrap_or_else(|| Path::new(".")))
        .output()
        .await
        .map_err(AppError::internal)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::internal(format!(
            "kepubify exited with status {}{}{}",
            output.status,
            if stderr.is_empty() { "" } else { "\n" },
            stderr
        )));
    }

    Ok(())
}
