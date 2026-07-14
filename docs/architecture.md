# Ephemeral Shelves Architecture

## Context

The current application has one trusted-network library backed by a global JSON
file and filesystem directory. The target is a public, single-instance service
with many isolated, temporary shelves. A shelf URL is a bearer capability that
can be transferred by QR code.

## MVP Shape

```text
Kobo browser ─┐
              ├─ HTTPS ─ Rust/Axum ─ SQLite metadata
Phone browser ┘                  ├─ shelf-scoped files
                                 └─ kepubify subprocess
```

Start with one application instance, one SQLite database, and local persistent
storage. This is intentionally simpler than introducing object storage or a
distributed job system before usage requires them.

## Core Domain

### Shelf

A shelf owns zero or more books and has:

- An internal ID that is safe to use in storage paths.
- A high-entropy bearer capability used to authorize requests.
- Creation, last-seen, last-activity, expiration, and optional hard-expiry times.
- A lifecycle state such as `active` or `expiring`.
- A revision incremented when its visible book collection changes.

### Book

A book belongs to exactly one shelf. Its database identity is not sufficient for
authorization: every lookup must include the shelf ID derived from a valid
capability.

### Presence and Activity

HTTP cannot reliably prove that a device disconnected. Presence is advisory;
expiration is authoritative.

- `last_seen_at` may be updated by bounded heartbeats or page access.
- `last_activity_at` records meaningful interaction such as explicit page entry,
  upload, download, or delete.
- `expires_at` is derived from policy and must never exceed a configured hard
  lifetime if one exists.
- Background polling must not keep an abandoned shelf alive indefinitely.

Exact TTL and extension rules remain product configuration; tests should use an
injectable clock rather than wall-clock sleeps.

For the MVP, creation initializes all presence/activity timestamps. Canonical
page entry, upload, download, and delete set both `last_seen_at` and
`last_activity_at`, then extend `expires_at` by 12 hours without crossing the
24-hour `hard_expires_at`. QR loads and conditional book polling update neither
timestamp. A shelf is unavailable when the clock is equal to either deadline.

## Capability Model

Opening `/` creates a shelf and redirects to a canonical shelf URL. The QR code
contains that URL. Possession grants the same management rights to every joined
device, matching the account-free product goal.

Requirements:

- Generate capabilities with a cryptographically secure RNG and at least 128
  bits of entropy.
- Never authorize by a user-supplied shelf ID alone.
- Avoid storing plaintext capability tokens where practical.
- Never expose full tokens in logs, errors, metrics, analytics, or referers to
  third-party resources.
- Serve no third-party assets from the shelf page.
- Return deliberately similar responses for invalid and expired capabilities.

Separate viewer/manager tokens are deferred until the product requires them.

## Storage

SQLite is the source of truth for shelf and book metadata. Enable foreign keys,
use migrations, and use transactions for related metadata changes. Suggested
logical schema:

```text
shelves(id, token_hash, state, revision, created_at, last_seen_at,
        last_activity_at, expires_at, hard_expires_at)
books(id, shelf_id, status, title, author, filename, stored_filename, size,
      uploaded_at)
```

Store files below a server-generated shelf path:

```text
data/shelves/<internal-shelf-id>/books/<internal-book-id>.kepub.epub
data/shelves/<internal-shelf-id>/uploads/<random-temp-name>.epub
```

Never construct a path from a capability, original filename, or other untrusted
value. Stream request and response bodies.

File and database operations cannot share one atomic transaction. Design them as
recoverable state transitions: use temporary files, publish only completed
books, compensate on failure, and let reconciliation remove orphans.

Phase 1 implements `pending`, `ready`, and `deleting` book states. Only `ready`
books are visible. Startup reconciliation finalizes a `pending` book when its
published file exists, removes its metadata when the file does not exist, and
retries file and metadata removal for `deleting` books.

Phase 2 removes the compatibility shelf. A shelf service generates a 256-bit
random capability, stores only its SHA-256 hash, and resolves the internal shelf
ID before any book lookup or filesystem operation. Capability comparisons are
constant-time after the hash-index lookup.

## HTTP and Synchronization

The canonical shelf URL and all mutations are shelf-scoped. The exact route
shape may change, but authorization must be applied before resolving a book.

The MVP route shape is `/s/<capability>/...`, with a trailing slash on the shelf
page so conservative relative form, XHR, QR, and download URLs remain scoped.
When `SHELF_ACCESS_CODE` is configured, `GET /` serves the creation form and a
successful `POST /shelves` creates the shelf. In ungated local mode, `GET /`
creates immediately. The access code is never included in the capability URL.

`PUBLIC_BASE_URL` is authoritative for QR URLs when configured. Request Host is
used only as the local-development fallback; forwarded headers are not trusted.
Configuring a public base requires HTTPS and an access code at startup.
Shelf responses disable storage in shared/private caches and set a no-referrer
policy so same-origin asset/API requests do not disclose capability paths.

Use periodic conditional polling for the critical shared-device experience.
Return an ETag or revision and allow `304 Not Modified`. WebSockets and
Server-Sent Events are not required for the MVP because older Kobo browsers are
the compatibility baseline.

Phase 3 polls the shelf snapshot every five seconds using XHR and
`If-None-Match`. The snapshot contains the visible book revision, expiration
timestamp, and shelf-scoped books. Its ETag contains both revision and
expiration so explicit activity on another request can refresh the displayed
deadline, while polling itself leaves the ETag and lifetime unchanged. Local
mutations and window focus trigger an immediate conditional refresh.

Derive public QR URLs from an explicit production base URL. Only trust forwarded
host/protocol headers when the server is behind a configured trusted proxy.

## Expiration and Cleanup

Cleanup is a state transition, not merely a recursive delete:

1. Atomically claim an expired active shelf and mark it `expiring`.
2. Reject new uploads and mutations for that shelf.
3. Allow, track, or safely terminate already-started operations according to a
   documented policy.
4. Delete book and temporary files idempotently.
5. Delete metadata, or retain a short token-free tombstone if operations need it.
6. Retry partial failures without damaging another shelf.

A periodic worker is sufficient for one instance. Its correctness must not
depend on running at an exact interval. Also sweep stale upload files that were
orphaned by crashes.

Phase 4 runs one in-process worker every 60 seconds and once during startup.
The worker and operation authorization share a lifecycle lock. Expired active
shelves are atomically marked `expiring`; new operations then fail before book
resolution. Started uploads/conversions and deletes defer the claim until their
operation guard ends. Started downloads may continue through five minutes after
the shelf deadline; after that deadline they no longer block cleanup and their
next stream read terminates.

Cleanup removes the shelf directory before deleting `expiring` metadata. Both
steps are idempotent: filesystem failure leaves the claim for retry, and a
database failure after file deletion retries against an already-absent
directory. Startup reconciliation runs before cleanup, so interrupted book
publication/deletion reaches a recoverable state first. Upload files older than
one hour are swept independently, except in shelves with active operations.

## Public-Hosting Boundaries

Anonymous EPUB conversion is a resource-exhaustion boundary. Enforce layered
limits:

- HTTP body size and request duration.
- Per-file, book-count, and total shelf quotas.
- Archive compressed/decompressed sizes and entry count.
- Conversion timeout and global/per-origin concurrency.
- Shelf creation and request rate limits.
- Global disk capacity protection.

Use HTTPS, security headers, restrictive CSP, `noindex`, safe download headers,
and token-redacted observability. Do not assume an `.epub` extension makes a ZIP
archive safe.

## Compatibility Principles

- Keep critical UI behavior compatible with the target Kobo browser.
- Prefer server-rendered HTML, forms/XHR, and periodic polling.
- Treat advanced browser APIs as progressive enhancement.
- Test the actual e-reader; desktop emulation is not sufficient evidence.

## Scaling Boundary

The MVP assumes one process owns local SQLite and files. Before adding multiple
instances, replace local storage with shared object storage, use a shared
database, and introduce distributed cleanup/conversion coordination. Do not put
multiple independent instances in front of the same local data directory.
