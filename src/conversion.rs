use std::{path::Path, process::Stdio};

use tokio::process::Command;

use crate::{
    config::Config,
    error::{AppError, AppResult},
};

pub async fn run_kepubify(config: &Config, input_path: &Path, output_path: &Path) -> AppResult<()> {
    let _permit = config
        .conversion_slots
        .acquire()
        .await
        .map_err(AppError::internal)?;
    let mut command = Command::new(&config.kepubify_bin);
    command
        .arg("-v")
        .arg("-u")
        .arg("-o")
        .arg(output_path.file_name().unwrap_or_default())
        .arg(input_path.file_name().unwrap_or_default())
        .current_dir(input_path.parent().unwrap_or_else(|| Path::new(".")))
        .kill_on_drop(true)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_process_limits(&mut command);

    let child = command.spawn().map_err(AppError::internal)?;
    let output =
        match tokio::time::timeout(config.conversion_timeout, child.wait_with_output()).await {
            Ok(result) => result.map_err(AppError::internal)?,
            Err(_) => return Err(AppError::internal("kepubify conversion timed out")),
        };

    if !output.status.success() {
        return Err(AppError::internal(format!(
            "kepubify exited with status {}",
            output.status
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn apply_process_limits(command: &mut Command) {
    // SAFETY: pre_exec runs in the child after fork. These direct setrlimit
    // syscalls do not allocate or access shared process state.
    unsafe {
        command.pre_exec(|| {
            set_limit(libc::RLIMIT_AS, 1024 * 1024 * 1024)?;
            set_limit(libc::RLIMIT_CPU, 300)?;
            set_limit(libc::RLIMIT_FSIZE, 600 * 1024 * 1024)?;
            set_limit(libc::RLIMIT_NOFILE, 256)?;
            Ok(())
        });
    }
}

#[cfg(unix)]
fn set_limit(resource: libc::__rlimit_resource_t, value: libc::rlim_t) -> std::io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: value,
        rlim_max: value,
    };
    // SAFETY: limit points to a valid rlimit for the duration of the syscall.
    if unsafe { libc::setrlimit(resource, &limit) } == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(unix))]
fn apply_process_limits(_command: &mut Command) {}
