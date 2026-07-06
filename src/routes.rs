use std::{path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE, HOST},
    },
    response::{Html, Response},
    routing::{delete, get, post},
};
use qrcode::{QrCode, render::svg};
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;
use tower_http::services::ServeDir;
use uuid::Uuid;

use crate::{
    books::{header_safe_filename, public_books, read_books, remove_file_if_exists, write_books},
    config::Config,
    error::{AppError, AppResult},
    library::store_upload,
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(upload_page))
        .route("/qr/{target}", get(qr_code))
        .route("/api/books", get(list_books))
        .route("/api/books/{id}", delete(delete_book))
        .route("/upload", post(upload_book))
        .route("/books/{id}/download", get(download_book))
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(state.config.max_upload_bytes))
        .with_state(state)
}

async fn upload_page() -> Html<&'static str> {
    Html(include_str!("../static/upload.html"))
}

async fn list_books(
    State(state): State<AppState>,
) -> AppResult<Json<Vec<crate::books::PublicBook>>> {
    Ok(Json(public_books(&state.config).await?))
}

async fn qr_code(
    State(state): State<AppState>,
    AxumPath(target): AxumPath<String>,
    headers: HeaderMap,
) -> AppResult<Response> {
    let path = match target.as_str() {
        "page.svg" => "/",
        _ => return Err(AppError::not_found("QR target not found")),
    };
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("raspi.local:3001");
    let host = host_with_port(host, state.config.port);
    let url = format!("http://{host}{path}");
    let code = QrCode::new(url.as_bytes()).map_err(AppError::internal)?;
    let image = with_svg_description(
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#111111"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        &url,
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "image/svg+xml")
        .body(Body::from(image))
        .map_err(AppError::internal)
}

fn host_with_port(host: &str, port: u16) -> String {
    let host = host.trim();
    if host.is_empty() {
        return format!("raspi.local:{port}");
    }

    if host_has_port(host) {
        host.to_string()
    } else {
        format!("{host}:{port}")
    }
}

fn host_has_port(host: &str) -> bool {
    if host.starts_with('[') {
        return host
            .rsplit_once("]:")
            .and_then(|(_, port)| port.parse::<u16>().ok())
            .is_some();
    }

    host.rsplit_once(':')
        .filter(|(name, _)| !name.contains(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .is_some()
}

fn with_svg_description(mut image: String, url: &str) -> String {
    if let Some(svg_start) = image.find("<svg") {
        if let Some(svg_tag_end) = image[svg_start..].find('>') {
            let index = svg_start + svg_tag_end;
            image.insert_str(index + 1, &format!("<desc>{}</desc>", escape_xml(url)));
        }
    } else if let Some(index) = image.find('>') {
        image.insert_str(index + 1, &format!("<desc>{}</desc>", escape_xml(url)));
    }
    image
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[allow(dead_code)]
fn _qr_preview_for_tests(url: &str) -> String {
    let code = QrCode::new(url.as_bytes()).expect("valid qr payload");
    with_svg_description(
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#111111"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        url,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_configured_port_when_host_has_none() {
        assert_eq!(host_with_port("raspi.local", 3001), "raspi.local:3001");
    }

    #[test]
    fn keeps_existing_host_port() {
        assert_eq!(host_with_port("raspi.local:3001", 3001), "raspi.local:3001");
    }

    #[test]
    fn adds_svg_description_for_debugging() {
        let svg = with_svg_description("<svg></svg>".to_string(), "http://raspi.local:3001/");
        assert!(svg.contains("<desc>http://raspi.local:3001/</desc>"));
    }
}

async fn upload_book(State(state): State<AppState>, mut multipart: Multipart) -> AppResult<String> {
    let mut saved_upload: Option<(PathBuf, String, u64)> = None;

    while let Some(mut field) = multipart.next_field().await.map_err(AppError::internal)? {
        if field.name() != Some("file") {
            continue;
        }

        let original_name = field
            .file_name()
            .map(str::to_owned)
            .unwrap_or_else(|| "book.epub".to_string());

        if !original_name.to_ascii_lowercase().ends_with(".epub") {
            return Err(AppError::bad_request("Only EPUB files are supported."));
        }

        let upload_path = state
            .config
            .uploads_dir
            .join(format!("upload-{}.epub", Uuid::new_v4()));
        let mut file = File::create(&upload_path)
            .await
            .map_err(AppError::internal)?;
        let mut size = 0_u64;

        while let Some(chunk) = field.chunk().await.map_err(AppError::internal)? {
            size += chunk.len() as u64;
            file.write_all(&chunk).await.map_err(AppError::internal)?;
        }

        if size == 0 {
            remove_file_if_exists(&upload_path).await?;
            return Err(AppError::bad_request(
                "Invalid file submitted: the EPUB is empty.",
            ));
        }

        saved_upload = Some((upload_path, original_name, size));
        break;
    }

    let (upload_path, original_name, _) =
        saved_upload.ok_or_else(|| AppError::bad_request("Choose an EPUB file first."))?;

    let result = store_upload(&state.config, &upload_path, &original_name).await;
    if result.is_err() {
        let _ = remove_file_if_exists(&upload_path).await;
    }

    let book = result?;
    Ok(format!("Ready for Kobo: {}", book.filename))
}

async fn download_book(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Response> {
    let books = read_books(&state.config).await?;
    let book = books
        .iter()
        .find(|book| book.id == id)
        .ok_or_else(|| AppError::not_found("Book not found"))?;
    let path = state.config.books_dir.join(&book.stored_filename);
    let file = File::open(path).await.map_err(AppError::internal)?;
    let stream = ReaderStream::new(file);

    let content_disposition = format!(
        "attachment; filename=\"{}\"",
        header_safe_filename(&book.filename)
    );
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/epub+zip")
        .header(
            CONTENT_DISPOSITION,
            HeaderValue::from_str(&content_disposition).map_err(AppError::internal)?,
        )
        .body(Body::from_stream(stream))
        .map_err(AppError::internal)
}

async fn delete_book(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> AppResult<Json<serde_json::Value>> {
    let mut books = read_books(&state.config).await?;
    let index = books
        .iter()
        .position(|book| book.id == id)
        .ok_or_else(|| AppError::not_found("Book not found"))?;
    let book = books.remove(index);

    remove_file_if_exists(&state.config.books_dir.join(book.stored_filename)).await?;
    write_books(&state.config, &books).await?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
