mod books;
mod config;
mod conversion;
mod epub;
mod error;
mod library;
mod routes;

use std::{net::SocketAddr, sync::Arc};

use config::Config;
use routes::AppState;
use tokio::fs;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Arc::new(Config::from_env());
    fs::create_dir_all(&config.data_dir).await?;
    fs::create_dir_all(&config.books_dir).await?;
    fs::create_dir_all(&config.uploads_dir).await?;

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    let app = routes::router(AppState {
        config: config.clone(),
    });

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("kobo-library is listening on http://{addr}");
    axum::serve(listener, app).await?;

    Ok(())
}
