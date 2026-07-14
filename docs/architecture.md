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
books(id, shelf_id, title, author, filename, stored_filename, size, uploaded_at)
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

## HTTP and Synchronization

The canonical shelf URL and all mutations are shelf-scoped. The exact route
shape may change, but authorization must be applied before resolving a book.

Use periodic conditional polling for the critical shared-device experience.
Return an ETag or revision and allow `304 Not Modified`. WebSockets and
Server-Sent Events are not required for the MVP because older Kobo browsers are
the compatibility baseline.

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

