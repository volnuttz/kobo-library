mod books;
mod config;
mod conversion;
mod epub;
mod error;
mod library;
mod repository;
mod routes;
mod shelves;
mod storage;

use std::{net::SocketAddr, sync::Arc};

use config::Config;
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
    library::reconcile_incomplete(database.as_ref(), storage.as_ref()).await?;
    let shelves = Arc::new(ShelfService::new(
        database.clone(),
        storage.clone(),
        Arc::new(SystemClock),
    ));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = routes::router(AppState {
        config: config.clone(),
        database,
        storage,
        shelves,
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("kobo-library is listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
