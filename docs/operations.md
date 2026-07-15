# Hosted Operations

The MVP runs one Epub Drop process, one SQLite database, and one local data
volume. Do not scale this application beyond one instance: SQLite and shelf
files are not shared between instances.

## Deployment

The supplied `Dockerfile` builds the release binary and bundles the pinned
`kepubify` release. Deploy that image to any environment meeting the contract
below; this repository intentionally contains no provider-specific deployment
configuration.

The minimum supported Rust version is 1.88; the Docker builder is pinned to
that version so it matches the dependency graph recorded in `Cargo.lock`.

The image verifies the pinned x86_64 `kepubify` binary by SHA-256. For an ARM
image, provide `--build-arg KEPUBIFY_SHA256=...` for the matching official
release asset; do not disable the verification.

The hosting environment must provide:

- Exactly one running application instance and one persistent local volume,
  mounted at `/data` (or another `DATA_DIR`) and writable by the container's
  non-root `epubdrop` user (UID 10001).
- HTTPS-only public traffic, with TLS terminated by the platform or a reverse
  proxy. The application listens on `0.0.0.0:3001` by default.
- A proxy request-body limit of 100 MiB or lower and request timeout no longer
  than the application's six-minute limit. It must stream request bodies rather
  than buffering them to an unbounded intermediary store.
- A health check that accepts `GET /healthz` returning `204`, without following
  the capability-creating redirect at `/`.
- Secret injection for `PUBLIC_BASE_URL` and `SHELF_ACCESS_CODE`; never bake
  either into an image or configuration committed to source control.
- A stop policy that sends `SIGTERM` to the application and allows up to five
  minutes for in-flight work to drain before force-killing it.
- Private metrics scraping or an equivalent access restriction for `/metrics`.
  Proxy/access logs must redact the full request path and query before export;
  `/s/<capability>` is a bearer credential.

For a container smoke test, substitute real secrets and a disposable host data
directory:

```sh
docker build --pull --tag epub-drop:release .
docker run --rm --name epub-drop -p 3001:3001 \
  -v /absolute/path/to/test-data:/data \
  -e PUBLIC_BASE_URL=https://books.example.com \
  -e SHELF_ACCESS_CODE='long-random-creation-code' \
  epub-drop:release
```

`PUBLIC_BASE_URL` must be the exact HTTPS origin users visit. It is embedded in
QR codes, so a mismatch prevents cross-device joining. `SHELF_ACCESS_CODE` is
a deployment secret, not a shelf credential; rotate it with the hosting
platform's secret manager and never put it in an image, deployment manifest,
ticket, or shell history.

## Configuration

| Variable | Hosted value | Notes |
| --- | --- | --- |
| `PORT` | `3001` | Must match `http_service.internal_port`. |
| `DATA_DIR` | `/data` | Must be the mounted persistent volume. |
| `KEPUBIFY_BIN` | `/usr/local/bin/kepubify` | Provided by the container image. |
| `PUBLIC_BASE_URL` | `https://your-domain` | Required in hosted mode; HTTPS only. |
| `SHELF_ACCESS_CODE` | secret | Required in hosted mode; gates shelf creation. |
| `MAX_UPLOAD_MB` | `100` | Per-request and converted-file limit. |
| `CONVERSION_CONCURRENCY` | `2` | Keep at two until load testing proves a safe change. |

The fixed MVP limits are 20 books and 500 MiB per shelf, 10 GiB of ready books
service-wide, eight concurrent uploads, 32 concurrent downloads, and two
conversions. The 20 GB volume leaves operational headroom above the service
quota; review disk alerts before increasing service limits.

## Monitoring and cleanup

The process runs cleanup at startup and every minute. Keep the instance running
so cleanup continues even when no one is browsing. Scrape `/metrics` through a
private platform integration or access-controlled monitoring path; it contains
aggregate counters only, never shelf capabilities or book metadata. Alert on
sustained high `kobo_stored_bytes`,
non-zero or increasing `kobo_cleanup_failures`, a growing
`kobo_cleanup_lag_seconds`, and repeated rejected requests.

Do not enable request logs that retain URL paths without redaction: `/s/<token>`
is a bearer credential. If proxy logs are required for incident response,
configure them to remove the entire path/query or replace capability path
segments with a one-way truncated hash. Do not log request bodies, filenames,
or access-code form values.

Ephemeral shelf contents need no backup. Preserve the repository migrations and
platform configuration. Volume snapshots are not a substitute for that policy;
if a live-shelf recovery is required, restore the whole `/data` volume together
because restoring only SQLite or only files can discard incomplete books during
startup reconciliation.

## Restart, rollback, and release checks

The server drains requests on `SIGTERM`; configure the hosting platform to
allow up to five minutes, matching the conversion timeout. A restart during an upload before publication leaves an
abandoned temporary upload for the independent sweep. A restart after a book is
published but before its metadata is finalized is reconciled at startup.
Downloads and conversions already in flight are allowed to drain within the
platform's five-minute bound; verify this on the deployed Machine before launch.

For every release:

1. Run `cargo fmt -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, and `cargo test`.
2. Run `docker build --pull --tag epub-drop:release .`, then start it with a
   disposable `/data` directory and hosted `PUBLIC_BASE_URL`/`SHELF_ACCESS_CODE`.
3. Confirm `GET /healthz` is `204`, HTTPS works at the public domain, the QR
   URL uses that domain, and metrics are collected without exposing a token.
4. Exercise a graceful platform restart: one upload, one conversion, and
   one throttled download must either complete during the five-minute drain or
   leave only state that startup reconciliation/sweeping repairs. Confirm a
   fresh request is accepted after restart.
5. Run browser and mobile acceptance, then the physical Kobo checklist in
   `docs/kobo-smoke-test.md`; retain redacted evidence only.
6. Load-test the configured boundaries in a non-production app: concurrent
   uploads/conversions, 20-book and 500 MiB shelf limits, 10 GiB service
   reservation, request-rate rejection, archive rejection, and recovery after
   termination. Do not point load tests at the public production shelf service.

Rollback uses the last known-good image, never a volume restore. Use the
hosting platform's image-version rollback mechanism to redeploy that image.

Before rolling back, check whether the older image understands the current
SQLite migrations. Migrations are forward-only, so if compatibility is not
explicitly verified, roll forward with a corrective image instead. Never attach
a second Machine to the same volume or delete `/data` as a rollback shortcut.
