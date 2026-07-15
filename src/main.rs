mod books;
mod config;
mod conversion;
mod epub;
mod error;
mod library;
mod observability;
mod repository;
mod routes;
mod shelves;
mod storage;

use std::{net::SocketAddr, sync::Arc};

use config::Config;
use observability::{Metrics, RateLimiter};
use repository::Database;
use routes::AppState;
use shelves::{ShelfService, SystemClock};
use storage::Storage;
use tokio::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Arc::new(Config::from_env());
    config.validate()?;
    fs::create_dir_all(&config.data_dir).await?;
    fs::create_dir_all(&config.shelves_dir).await?;
    let database = Arc::new(Database::open(&config.database_path).await?);
    let storage = Arc::new(Storage::new(config.shelves_dir.clone()));
    library::reconcile_incomplete(
        database.as_ref(),
        storage.as_ref(),
        config.max_upload_bytes as i64,
        config.max_shelf_bytes,
        config.max_service_bytes,
    )
    .await?;
    let shelves = Arc::new(ShelfService::new(
        database.clone(),
        storage.clone(),
        Arc::new(SystemClock),
    ));
    let metrics = Arc::new(Metrics::default());
    shelves.cleanup_once().await?;
    let cleanup_service = shelves.clone();
    let cleanup_metrics = metrics.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.tick().await;
        loop {
            interval.tick().await;
            match cleanup_service.cleanup_once().await {
                Ok(stats) => Metrics::add(&cleanup_metrics.shelves_cleaned, stats.shelves_removed),
                Err(error) => {
                    Metrics::increment(&cleanup_metrics.cleanup_failures);
                    eprintln!("shelf cleanup failed: {error}");
                }
            }
        }
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = routes::router(AppState {
        config: config.clone(),
        database,
        storage,
        shelves,
        metrics,
        rate_limiter: Arc::new(RateLimiter::default()),
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("epub-drop is listening on http://{addr}");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut terminate =
            signal(SignalKind::terminate()).expect("installing the SIGTERM handler must succeed");
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.expect("installing the Ctrl-C handler must succeed");
            }
            _ = terminate.recv() => {}
        }
    }

    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .expect("installing the Ctrl-C handler must succeed");

    println!("shutdown signal received; draining in-flight requests");
}
