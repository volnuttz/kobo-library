# Epub Drop

A Rust service for creating temporary, QR-shareable Kobo-friendly book shelves.

Open the web page to create a shelf, scan its QR code on another device, upload
an EPUB, then download the converted book from the Kobo browser. Each shelf URL
is an unguessable bearer capability; there are no user accounts.

## Features

- Temporary isolated shelves with capability-scoped URLs.
- EPUB upload from a phone, tablet, or computer.
- Automatic conversion from `.epub` to `.kepub.epub` with
  [kepubify](https://github.com/pgaskin/kepubify).
- Existing kepubs are detected and stored without running `kepubify` again.
- EPUB metadata extraction for title and author.
- Persistent SQLite metadata and shelf-scoped files stored under `data/`.
- QR code for the current page, useful when the page is open on the Kobo and you
  want to open it on a phone.
- Low-cost conditional polling keeps joined devices synchronized without
  WebSockets or manual page refreshes.
- Clear empty, retry, upload, expiration, and unavailable-shelf states.
- Twelve-hour inactivity expiration with a 24-hour maximum lifetime and
  automatic retryable cleanup.
- Rust/Axum server with a single compiled binary.
- systemd user service support.

## How To Use

1. Open the app from a phone, tablet, or computer to create a shelf.
2. If configured, enter the deployment access code.
3. Scan the QR code to join the same shelf on another device.
4. Tap `Choose EPUB`.
5. Pick an `.epub` or `.kepub.epub` file and tap `Upload`.
6. Tap `Download` beside the book on the Kobo.

The page also includes `Delete` actions for removing books from the temporary
shelf.

## Book Handling

Normal EPUB files are converted with `kepubify` and stored as `.kepub.epub`
files.

Files are stored without conversion when either of these is true:

- The uploaded filename ends in `.kepub.epub`.
- The EPUB content already contains Kobo/kepub markers such as `koboSpan`.

The app reads EPUB metadata from `META-INF/container.xml` and the package OPF
file. When metadata is available, the UI shows the book title and author instead
of relying on the filename.

## Routes

- `GET /` - create a shelf, or show the access-code form when gated.
- `POST /shelves` - create a shelf in access-code-gated mode.
- `GET /s/{capability}/` - shelf page.
- `POST /s/{capability}/upload` - upload one EPUB.
- `GET /s/{capability}/api/books` - list that shelf's books.
- `GET /s/{capability}/books/{id}/download` - download a scoped kepub.
- `DELETE /s/{capability}/api/books/{id}` - delete a scoped book.
- `GET /s/{capability}/qr/page.svg` - QR code for joining the shelf.

There is no `/receive` page anymore. The app is intentionally a single-page
flow.

## Run Locally

Install a local Linux `kepubify` binary:

```sh
sh scripts/install-kepubify-linux.sh
```

Build and run:

```sh
cargo run
```

Open:

```text
http://SERVER_IP:3001/
```

For a release build:

```sh
cargo build --release
./target/release/epub-drop
```

## systemd User Service

You can run Epub Drop as a systemd user service on a Raspberry Pi or other
Linux home server. Create `~/.config/systemd/user/epub-drop.service`:

```ini
[Unit]
Description=Epub Drop
After=network-online.target

[Service]
Type=simple
WorkingDirectory=/path/to/epub-drop
ExecStart=/path/to/epub-drop/target/release/epub-drop
Environment=PORT=3001
Environment=DATA_DIR=/path/to/epub-drop/data
Environment=KEPUBIFY_BIN=/path/to/epub-drop/bin/kepubify
Environment=MAX_UPLOAD_MB=100
Environment=CONVERSION_CONCURRENCY=2
Restart=on-failure

[Install]
WantedBy=default.target
```

Useful commands:

```sh
systemctl --user status epub-drop.service
systemctl --user restart epub-drop.service
systemctl --user stop epub-drop.service
systemctl --user enable epub-drop.service
systemctl --user disable --now epub-drop.service
journalctl --user -u epub-drop.service -f
```

After rebuilding the release binary, restart the service:

```sh
cargo build --release
systemctl --user restart epub-drop.service
```

## Configuration

Configuration is via environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `PORT` | `3001` | HTTP port to listen on. |
| `DATA_DIR` | `./data` | Directory for metadata, uploads, and converted books. |
| `KEPUBIFY_BIN` | `./bin/kepubify` if present, otherwise `kepubify` from `PATH` | Converter executable. |
| `MAX_UPLOAD_MB` | `100` | Maximum request body and converted EPUB size in MiB. |
| `CONVERSION_CONCURRENCY` | `2` | Maximum simultaneous `kepubify` processes. |
| `PUBLIC_BASE_URL` | unset | Authoritative HTTPS origin used in QR shelf URLs. Required for hosted deployment. |
| `SHELF_ACCESS_CODE` | unset | Deployment-wide code required only to create shelves. Must be set for hosted deployment. |

When `PUBLIC_BASE_URL` is set, startup rejects non-HTTPS URLs and requires
`SHELF_ACCESS_CODE`. With no public base URL, request Host is used only for the
ungated local-development flow.

The MVP also fixes each shelf at 20 books and 500 MiB, with 10 GiB of ready
books service-wide. EPUB archives are limited to 10,000 entries and 500 MiB of
declared decompressed content. Conversion times out after five minutes. The
process bounds concurrent uploads to eight and downloads to 32, and applies
fixed-window limits to shelf creation and capability-scoped actions. Run only
one application instance while these controls and SQLite storage are local.

## Data Layout

Runtime data lives under `DATA_DIR`:

```text
data/
  library.sqlite3
  shelves/
    <internal-shelf-uuid>/
      books/
      uploads/
```

- `library.sqlite3` stores shelf and book metadata and is migrated at startup.
- Each shelf has isolated `books/` and temporary `uploads/` directories.
- Capabilities are not used in storage paths and plaintext capabilities are not
  stored in SQLite.
- Expired shelf directories and metadata are reclaimed by the cleanup worker;
  abandoned upload files are swept after one hour.

Existing `books.json` files are not detected or imported. This version starts
with an empty SQLite library; retain an old data directory separately if needed.

Shelf content is deliberately ephemeral and does not require backup. Back up
deployment configuration, secrets through the platform's secret mechanism, and
the source-controlled SQLite migrations. If operational recovery requires
preserving live shelves across a host failure, snapshot the entire `DATA_DIR`
consistently; restoring only the database or only the files can leave incomplete
book states for startup reconciliation to discard.

## Development

Common checks:

```sh
cargo fmt -- --check
cargo clippy -- -D warnings
cargo test
cargo build --release
```

Project structure:

```text
src/
  main.rs        server startup
  config.rs      environment configuration
  routes.rs      HTTP routes and handlers
  library.rs     upload storage and conversion flow
  observability.rs aggregate metrics and bounded rate limiting
  books.rs       book models and filename handling
  repository.rs SQLite shelf/book repositories and migrations
  shelves.rs    capability generation and shelf authorization
  storage.rs     shelf-scoped filesystem paths
  epub.rs        EPUB metadata and kepub detection
  conversion.rs  kepubify wrapper
  error.rs       HTTP error mapping
static/
  shelf.html     single shelf app page
  style.css      UI styling
  common.js      small shared browser helpers
  app.js         upload, list, download, delete behavior
```

## Troubleshooting

Check whether the service is running:

```sh
systemctl --user status epub-drop.service
```

Follow logs:

```sh
journalctl --user -u epub-drop.service -f
```

Confirm the app responds locally:

```sh
curl -I http://127.0.0.1:3001/
```

Confirm `kepubify` is available:

```sh
./bin/kepubify --version
```

If `raspi.local` does not resolve from another device, use the server IP address:

```text
http://SERVER_IP:3001/
```

mDNS requires same-network access, multicast UDP 5353, and client support for
`.local` hostnames.

## Security

Shelf URLs are bearer credentials: anyone who has one can upload, download, and
delete that shelf's books. Do not paste shelf URLs into logs or public issues.

The application emits no HTTP access log and its metrics are aggregate and
token-free. Configure the public proxy to disable access logging for `/s/*` (or
redact the path to `/s/[capability]/*`) and never send those paths to analytics,
error-reporting, or tracing vendors. Configure HTTPS-only traffic, a request
body limit no larger than the application limit, persistent-volume capacity
alerts, and alerts for cleanup failures/lag and storage approaching 10 GiB.

`GET /metrics` is intentionally low-cardinality and contains no shelf or book
metadata. It is rate-limited but not authenticated; restrict it at the hosting
network or proxy if operational metrics should not be public.
