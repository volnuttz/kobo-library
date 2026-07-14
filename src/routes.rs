use std::{path::PathBuf, sync::Arc};

use axum::{
    Form, Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, State},
    http::{
        HeaderMap, HeaderValue,
        header::{CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, HOST, REFERRER_POLICY},
    },
    response::{Html, IntoResponse, Redirect, Response},
    routing::{delete, get, post},
};
use qrcode::{QrCode, render::svg};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::{fs::File, io::AsyncWriteExt};
use tokio_util::io::ReaderStream;
use tower_http::services::ServeDir;
use url::Url;

use crate::{
    books::{PublicBook, header_safe_filename},
    config::Config,
    error::{AppError, AppResult},
    library::{delete_book as delete_stored_book, store_upload},
    repository::{BookRepository, Database},
    shelves::ShelfService,
    storage::{Storage, remove_file_if_exists},
};

const CREATE_PAGE: &str = r#"<!doctype html><html><head><title>Kobo Library</title><meta charset="utf-8"/><meta name="viewport" content="width=device-width, initial-scale=1.0"/><meta name="referrer" content="no-referrer"/><link rel="stylesheet" href="/static/style.css"/></head><body><main class="wrapper"><h1>Kobo Library</h1><form action="/shelves" method="post"><label for="access_code">Access code</label><input id="access_code" name="access_code" type="password" required/><input type="submit" value="Create shelf"/></form></main></body></html>"#;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: Arc<Database>,
    pub storage: Arc<Storage>,
    pub shelves: Arc<ShelfService>,
}

#[derive(Deserialize)]
struct CreateShelfForm {
    access_code: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/shelves", post(create_shelf_with_code))
        .route("/s/{token}/", get(shelf_page))
        .route("/s/{token}/qr/{target}", get(qr_code))
        .route("/s/{token}/api/books", get(list_books))
        .route("/s/{token}/api/books/{id}", delete(delete_book))
        .route("/s/{token}/upload", post(upload_book))
        .route("/s/{token}/books/{id}/download", get(download_book))
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(state.config.max_upload_bytes))
        .with_state(state)
}

async fn root(State(state): State<AppState>) -> AppResult<Response> {
    if state.config.shelf_access_code.is_some() {
        return private_response(Html(CREATE_PAGE));
    }
    create_shelf_redirect(&state).await
}

async fn create_shelf_with_code(
    State(state): State<AppState>,
    Form(form): Form<CreateShelfForm>,
) -> AppResult<Response> {
    let Some(expected) = state.config.shelf_access_code.as_deref() else {
        return Err(AppError::not_found("Not found."));
    };
    if !secret_matches(expected, &form.access_code) {
        return Err(AppError::not_found("Invalid access code."));
    }
    create_shelf_redirect(&state).await
}

async fn create_shelf_redirect(state: &AppState) -> AppResult<Response> {
    let created = state.shelves.create().await?;
    Ok(Redirect::to(&format!("/s/{}/", created.token)).into_response())
}

async fn shelf_page(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
) -> AppResult<Response> {
    state.shelves.authorize(&token, true).await?;
    private_response(Html(include_str!("../static/upload.html")))
}

async fn list_books(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
) -> AppResult<Response> {
    let shelf = state.shelves.authorize(&token, false).await?;
    let books = state.database.list_books(&shelf.id).await?;
    private_response(Json(books.iter().map(PublicBook::from).collect::<Vec<_>>()))
}

async fn qr_code(
    State(state): State<AppState>,
    AxumPath((token, target)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> AppResult<Response> {
    state.shelves.authorize(&token, false).await?;
    if target != "page.svg" {
        return Err(AppError::not_found("QR target not found"));
    }
    let url = public_shelf_url(&state.config, &headers, &token)?;
    let code = QrCode::new(url.as_bytes()).map_err(AppError::internal)?;
    let image = with_svg_description(
        code.render::<svg::Color>()
            .min_dimensions(220, 220)
            .dark_color(svg::Color("#111111"))
            .light_color(svg::Color("#ffffff"))
            .build(),
        &url,
    );

    private_response(([(CONTENT_TYPE, "image/svg+xml")], image))
}

async fn upload_book(
    State(state): State<AppState>,
    AxumPath(token): AxumPath<String>,
    mut multipart: Multipart,
) -> AppResult<Response> {
    let shelf = state.shelves.authorize(&token, true).await?;
    let mut saved_upload: Option<(PathBuf, String)> = None;

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

        let upload_path = state.storage.new_upload_path(&shelf.id)?;
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
        saved_upload = Some((upload_path, original_name));
        break;
    }

    let (upload_path, original_name) =
        saved_upload.ok_or_else(|| AppError::bad_request("Choose an EPUB file first."))?;
    let result = store_upload(
        &state.config,
        state.database.as_ref(),
        &state.storage,
        &shelf.id,
        &upload_path,
        &original_name,
    )
    .await;
    if result.is_err() {
        let _ = remove_file_if_exists(&upload_path).await;
    }
    let book = result?;
    private_response(format!("Ready for Kobo: {}", book.filename))
}

async fn download_book(
    State(state): State<AppState>,
    AxumPath((token, id)): AxumPath<(String, String)>,
) -> AppResult<Response> {
    let shelf = state.shelves.authorize(&token, true).await?;
    let book = state
        .database
        .book(&shelf.id, &id)
        .await?
        .ok_or_else(|| AppError::not_found("Book not found"))?;
    let file = File::open(state.storage.book_path(&shelf.id, &book.id)?)
        .await
        .map_err(AppError::internal)?;
    let content_disposition = format!(
        "attachment; filename=\"{}\"",
        header_safe_filename(&book.filename)
    );
    let mut response = Response::new(Body::from_stream(ReaderStream::new(file)));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/epub+zip"),
    );
    response.headers_mut().insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition).map_err(AppError::internal)?,
    );
    add_private_headers(&mut response);
    Ok(response)
}

async fn delete_book(
    State(state): State<AppState>,
    AxumPath((token, id)): AxumPath<(String, String)>,
) -> AppResult<Response> {
    let shelf = state.shelves.authorize(&token, true).await?;
    if !delete_stored_book(state.database.as_ref(), &state.storage, &shelf.id, &id).await? {
        return Err(AppError::not_found("Book not found"));
    }
    private_response(Json(serde_json::json!({ "ok": true })))
}

fn private_response(response: impl IntoResponse) -> AppResult<Response> {
    let mut response = response.into_response();
    add_private_headers(&mut response);
    Ok(response)
}

fn add_private_headers(response: &mut Response) {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
}

fn secret_matches(expected: &str, supplied: &str) -> bool {
    let expected = Sha256::digest(expected.as_bytes());
    let supplied = Sha256::digest(supplied.as_bytes());
    bool::from(expected.ct_eq(&supplied))
}

fn public_shelf_url(config: &Config, headers: &HeaderMap, token: &str) -> AppResult<String> {
    if let Some(base) = &config.public_base_url {
        let mut url = Url::parse(base).map_err(AppError::internal)?;
        if !matches!(url.scheme(), "http" | "https") || url.cannot_be_a_base() {
            return Err(AppError::internal(
                "PUBLIC_BASE_URL must be an HTTP(S) base URL",
            ));
        }
        url.set_path(&format!("/s/{token}/"));
        url.set_query(None);
        url.set_fragment(None);
        return Ok(url.to_string());
    }

    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("raspi.local:3001");
    Ok(format!(
        "http://{}/s/{token}/",
        host_with_port(host, config.port)
    ))
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
    if let Some(svg_start) = image.find("<svg")
        && let Some(svg_tag_end) = image[svg_start..].find('>')
    {
        let index = svg_start + svg_tag_end;
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::{
        books::Book,
        repository::{BookRepository, Database, ShelfRepository},
        shelves::tests::FixedClock,
    };

    async fn test_app() -> (Router, Arc<Database>, Arc<Storage>, String, String, PathBuf) {
        let database = Arc::new(Database::memory().await.unwrap());
        let root = std::env::temp_dir().join(format!("kobo-routes-{}", Uuid::new_v4()));
        let storage = Arc::new(Storage::new(root.join("shelves")));
        let clock = Arc::new(FixedClock(std::sync::Mutex::new(Utc::now())));
        let shelves = Arc::new(ShelfService::new(database.clone(), storage.clone(), clock));
        let first = shelves.create().await.unwrap().token;
        let second = shelves.create().await.unwrap().token;
        let config = Arc::new(Config::for_test(root.clone(), None, None));
        let app = router(AppState {
            config,
            database: database.clone(),
            storage: storage.clone(),
            shelves,
        });
        (app, database, storage, first, second, root)
    }

    #[test]
    fn adds_configured_port_when_host_has_none() {
        assert_eq!(host_with_port("raspi.local", 3001), "raspi.local:3001");
    }

    #[test]
    fn public_base_url_overrides_request_host() {
        let config = Config::for_test_with_public_url("https://books.example.test/base");
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("attacker.example"));
        assert_eq!(
            public_shelf_url(&config, &headers, "token").unwrap(),
            "https://books.example.test/s/token/"
        );
    }

    #[test]
    fn compares_access_codes_without_plaintext_equality() {
        assert!(secret_matches("correct horse", "correct horse"));
        assert!(!secret_matches("correct horse", "wrong"));
    }

    #[tokio::test]
    async fn root_creates_a_shelf_and_redirects_to_its_capability() {
        let (app, _, _, _, _, root) = test_app().await;
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response
            .headers()
            .get("location")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(location.starts_with("/s/") && location.ends_with('/'));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn capability_cannot_list_or_delete_another_shelfs_book() {
        let (app, database, _, first_token, second_token, root) = test_app().await;
        let first_hash = Sha256::digest(first_token.as_bytes()).to_vec();
        let first_shelf = database
            .shelf_by_token_hash(&first_hash)
            .await
            .unwrap()
            .unwrap();
        let book_id = Uuid::new_v4().to_string();
        let book = Book {
            id: book_id.clone(),
            shelf_id: first_shelf.id.clone(),
            status: "pending".to_string(),
            title: "Private book".to_string(),
            author: None,
            filename: "Private.kepub.epub".to_string(),
            original_name: "Private.epub".to_string(),
            stored_filename: format!("{book_id}.kepub.epub"),
            size: 0,
            uploaded_at: Utc::now(),
        };
        database.insert_pending(&book).await.unwrap();
        database
            .finalize_book(&first_shelf.id, &book_id, 10)
            .await
            .unwrap();

        let other_list = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/s/{second_token}/api/books"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(other_list.status(), StatusCode::OK);
        let body = other_list.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(body.as_ref(), b"[]");

        let other_delete = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/s/{second_token}/api/books/{book_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(other_delete.status(), StatusCode::NOT_FOUND);
        assert!(
            database
                .book(&first_shelf.id, &book_id)
                .await
                .unwrap()
                .is_some()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
