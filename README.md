# Kobo Library

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
- Rust/Axum server with a single compiled binary.
- systemd user service support.

## How To Use

1. Open the app from a phone, tablet, or computer to create a shelf.
2. If configured, enter the deployment access code.
3. Scan the QR code to join the same shelf on another device.
4. Tap `Choose EPUB`.
5. Pick an `.epub` or `.kepub.epub` file and tap `Upload`.
6. Tap `Download` beside the book on the Kobo.

The page also includes `Delete` actions for removing books from the local
library.

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
./target/release/kobo-library
```

## systemd User Service

You can run Kobo Library as a systemd user service on a Raspberry Pi or other
Linux home server. Create `~/.config/systemd/user/kobo-library.service`:

```ini
[Unit]
Description=Kobo Library
After=network-online.target

[Service]
Type=simple
WorkingDirectory=/path/to/kobo-library
ExecStart=/path/to/kobo-library/target/release/kobo-library
Environment=PORT=3001
Environment=DATA_DIR=/path/to/kobo-library/data
Environment=KEPUBIFY_BIN=/path/to/kobo-library/bin/kepubify
Environment=MAX_UPLOAD_MB=800
Restart=on-failure

[Install]
WantedBy=default.target
```

Useful commands:

```sh
systemctl --user status kobo-library.service
systemctl --user restart kobo-library.service
systemctl --user stop kobo-library.service
systemctl --user enable kobo-library.service
systemctl --user disable --now kobo-library.service
journalctl --user -u kobo-library.service -f
```

After rebuilding the release binary, restart the service:

```sh
cargo build --release
systemctl --user restart kobo-library.service
```

## Configuration

Configuration is via environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `PORT` | `3001` | HTTP port to listen on. |
| `DATA_DIR` | `./data` | Directory for metadata, uploads, and converted books. |
| `KEPUBIFY_BIN` | `./bin/kepubify` if present, otherwise `kepubify` from `PATH` | Converter executable. |
| `MAX_UPLOAD_MB` | `800` | Maximum request body size in MB. |
| `PUBLIC_BASE_URL` | unset | Authoritative HTTPS origin used in QR shelf URLs. Required for hosted deployment. |
| `SHELF_ACCESS_CODE` | unset | Deployment-wide code required only to create shelves. Must be set for hosted deployment. |

When `PUBLIC_BASE_URL` is set, startup rejects non-HTTPS URLs and requires
`SHELF_ACCESS_CODE`. With no public base URL, request Host is used only for the
ungated local-development flow.

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

Existing `books.json` files are not detected or imported. This version starts
with an empty SQLite library; retain an old data directory separately if needed.

Keep `data/` if you want to preserve the library across rebuilds or service
restarts.

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
  books.rs       book models and filename handling
  repository.rs SQLite shelf/book repositories and migrations
  shelves.rs    capability generation and shelf authorization
  storage.rs     shelf-scoped filesystem paths
  epub.rs        EPUB metadata and kepub detection
  conversion.rs  kepubify wrapper
  error.rs       HTTP error mapping
static/
  upload.html    single app page
  style.css      UI styling
  common.js      upload, list, download, delete behavior
```

## Troubleshooting

Check whether the service is running:

```sh
systemctl --user status kobo-library.service
```

Follow logs:

```sh
journalctl --user -u kobo-library.service -f
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

Capability isolation is implemented, but the resource controls and deployment
hardening required by Phase 5 are not. Do not expose this version directly to
the public internet.
