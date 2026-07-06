# Kobo Library

A small Rust service for keeping a local Kobo-friendly book library on a
Raspberry Pi or other home server.

Open the web page from a phone or computer, upload an EPUB, then open the same
page from the Kobo browser and download the converted book. The app is designed
for a trusted local network and intentionally does not use pairing keys or
accounts.

## Features

- Single local web page for upload, download, and deletion.
- EPUB upload from a phone, tablet, or computer.
- Automatic conversion from `.epub` to `.kepub.epub` with
  [kepubify](https://github.com/pgaskin/kepubify).
- Existing kepubs are detected and stored without running `kepubify` again.
- EPUB metadata extraction for title and author.
- Persistent local library stored under `data/`.
- QR code for the current page, useful when the page is open on the Kobo and you
  want to open it on a phone.
- Rust/Axum server with a single compiled binary.
- systemd user service support.
- Docker Compose support.

## How To Use

1. Open the app from a phone, tablet, or computer.
2. Tap `Choose EPUB`.
3. Pick an `.epub` or `.kepub.epub` file.
4. Tap `Upload`.
5. Open the same app page from the Kobo browser.
6. Tap `Download` beside the book.

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

- `GET /` - main app page.
- `POST /upload` - upload one EPUB file.
- `GET /api/books` - list books used by the UI.
- `GET /books/{id}/download` - download one stored kepub.
- `DELETE /api/books/{id}` - delete one stored book.
- `GET /qr/page.svg` - QR code for the current page URL.

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

## Docker Compose

Docker is not currently installed on this Pi, but the project includes a Compose
setup for machines that have Docker available:

```sh
docker compose up -d --build
```

The image downloads the correct `kepubify` binary for these Linux
architectures:

- x86_64
- ARM64
- ARMv7
- ARMv6

The Compose service mounts local persistent data:

```text
./data:/app/data
```

## Configuration

Configuration is via environment variables:

| Variable | Default | Description |
| --- | --- | --- |
| `PORT` | `3001` | HTTP port to listen on. |
| `DATA_DIR` | `./data` | Directory for metadata, uploads, and converted books. |
| `KEPUBIFY_BIN` | `./bin/kepubify` if present, otherwise `kepubify` from `PATH` | Converter executable. |
| `MAX_UPLOAD_MB` | `800` | Maximum request body size in MB. |

## Data Layout

Runtime data lives under `DATA_DIR`:

```text
data/
  books.json
  books/
  uploads/
```

- `books.json` stores library metadata.
- `books/` stores the downloadable `.kepub.epub` files.
- `uploads/` stores temporary upload files while processing.

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
  books.rs       metadata persistence and public book models
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

Kobo Library has no authentication. Anyone on the reachable network can upload,
download, and delete books.

Use it only on a trusted local network. Do not expose it directly to the public
internet.
