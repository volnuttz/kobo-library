# Ephemeral Shelves Roadmap

## Goal

Turn Epub Drop into an internet-hosted service where opening the site creates
a temporary shelf, scanning its QR code joins another device to that shelf, and
all joined devices can upload, list, download, and delete EPUBs. Shelves and
their files expire after inactivity.

## Product Acceptance Criteria

- A new visitor can create an empty shelf without an account.
- The shelf page displays a QR code that joins the same shelf on another device.
- Changes made on one device become visible on other joined devices without a
  manual full-page refresh.
- Shelf members can upload, download, and delete only that shelf's books.
- An unguessable shelf capability is required for every shelf operation.
- The UI communicates expiration and behaves clearly after expiration.
- Expired metadata, books, and abandoned uploads are eventually deleted.
- The public service has bounded upload, storage, conversion, and request costs.
- Core interactions remain usable in the Kobo browser.

## Phase 0 — Baseline and Decisions

- [x] Record the target architecture and security invariants.
- [x] Create repository guidance and reusable implementation/review skills.
- [x] Define supported Kobo models/browser capabilities with a physical-device
  smoke-test checklist.
- [x] Decide the initial shelf inactivity lifetime and maximum lifetime.
- [x] Decide upload size, book count, and total shelf storage quotas.
- [x] Decide whether shelf creation is fully public or gated by a deployment-wide
  access code.
- [x] Define a platform-neutral single-instance deployment contract and HTTPS
  termination requirements.

Exit criteria: open decisions needed by the MVP have owners/defaults, and the
current local behavior is covered sufficiently to refactor safely.

Phase 0 completed with accepted defaults in `docs/decisions.md`, the physical
device procedure in `docs/kobo-smoke-test.md`, and current behavior and known
gaps recorded in `docs/baseline.md`. A physical pass remains a Phase 3 exit
requirement; defining the repeatable checklist is the Phase 0 deliverable.

## Phase 1 — Persistence Foundation

- [x] Introduce SQLite and a migration mechanism.
- [x] Add `shelves` and `books` tables with foreign keys and timestamps.
- [x] Create repository interfaces for shelf and book operations.
- [x] Move metadata from the global `books.json` file to SQLite.
- [x] Store files under shelf-specific directories.
- [x] Make book creation/deletion resilient to metadata/file operation failures.
- [x] Add tests for concurrent mutations and shelf isolation.
- [x] Define migration behavior for an existing local library, or explicitly
  document that the hosted mode starts clean.

Exit criteria: metadata supports multiple isolated shelves, survives restart,
and no route needs to read or rewrite a global JSON collection.

Phase 1 completed with embedded SQLite migrations, shelf/book repositories,
shelf-scoped paths, recoverable book publication/deletion states, and startup
reconciliation. The transitional trusted-network routes use one internal
compatibility shelf until Phase 2 adds capability authorization. Deployments
always start clean; `books.json` is neither detected nor imported.

## Phase 2 — Shelf Lifecycle and Capability Access

- [x] Create a shelf service with cryptographically random capability tokens.
- [x] Store only a token hash where practical and use constant-time comparison.
- [x] Make `GET /` create a shelf and redirect to its canonical URL.
- [x] Add shelf-scoped page and API routes.
- [x] Generate a QR code for the current shelf URL using the configured public
  base URL or trusted proxy information.
- [x] Return a consistent expired/not-found response without leaking shelf
  existence unnecessarily.
- [x] Ensure logs and errors redact shelf capabilities.
- [x] Add tests proving a token cannot read or mutate another shelf.

Exit criteria: two independently created shelves remain isolated and QR joining
grants access to exactly one shelf.

Phase 2 completed with 256-bit URL-safe capabilities, hash-only persistence,
constant-time verification, capability-scoped routes, explicit QR base URLs,
uniform unavailable-shelf responses, and HTTP-level cross-shelf denial tests.
Ungated local mode creates on `GET /`; setting `SHELF_ACCESS_CODE` enables the
ADR-009 creation form without putting that code in shelf URLs.

## Phase 3 — Shared Device Experience

- [x] Scope upload, list, download, and delete UI actions to the current shelf.
- [x] Add low-cost polling compatible with the Kobo browser.
- [x] Add a shelf revision or ETag so unchanged polling is inexpensive.
- [x] Refresh immediately after local mutations and when a page regains focus.
- [x] Display shelf expiration and useful empty, expired, upload, and conversion
  states.
- [x] Verify ES5-era JavaScript and avoid relying on WebSockets or modern browser
  APIs for the critical flow.
- [x] Run the end-to-end two-device test: Kobo creates, phone joins/uploads, Kobo
  sees/downloads, either device deletes.

Exit criteria: the complete QR-assisted workflow works across two devices,
including at least one target Kobo.

Phase 3 software work includes five-second conditional XHR polling, stable
`304` responses, immediate mutation/focus refresh, expiration and failure UI,
and automated two-client synchronization/isolation coverage. Static checks
reject modern-only JavaScript primitives in the critical flow. The manual
two-device Kobo test was confirmed on 2026-07-15, completing Phase 3.

## Phase 4 — Expiration and Garbage Collection

- [x] Define `last_seen_at`, `last_activity_at`, `expires_at`, and maximum lifetime
  semantics in code and documentation.
- [x] Ensure background polling alone cannot preserve a shelf forever.
- [x] Add an explicit lifecycle state for active/expiring shelves.
- [x] Implement a periodic cleanup worker with a single-runner strategy.
- [x] Make cleanup idempotent and safe to retry after partial failure or restart.
- [x] Prevent new mutations after expiration begins.
- [x] Handle active downloads and conversions during expiration.
- [x] Remove abandoned temporary uploads independently of shelf cleanup.
- [x] Add deterministic clock-based tests around expiry boundaries and retries.

Exit criteria: expired shelves become inaccessible and all associated storage is
eventually reclaimed, including across process restarts.

Phase 4 completed with exact deterministic deadline semantics, guarded active
operations, atomic `active` to `expiring` claims, a single periodic worker,
restart-safe idempotent cleanup, five-minute download grace enforcement, and an
independent one-hour abandoned-upload sweep. Cleanup/retry tests use an injected
clock and do not sleep.

## Phase 5 — Internet Hardening

- [x] Enforce per-file, per-shelf, and service-wide storage limits.
- [x] Rate-limit shelf creation, uploads, downloads, and conversion work.
- [x] Limit ZIP entry count and decompressed size to resist archive bombs.
- [x] Add conversion timeout, concurrency, and process resource controls.
- [x] Validate proxy/public URL configuration and enforce HTTPS in production.
- [x] Add security headers, a restrictive CSP, and `noindex` directives.
- [x] Ensure sensitive paths are excluded from analytics and access logs.
- [x] Add structured metrics for active shelves, disk use, failures, cleanup lag,
  conversion duration, and rejected requests.
- [x] Define backup expectations: ephemeral content does not require backup, but
  deployment configuration and schema migrations do.
- [x] Perform a public-hosting review using `$review-public-hosting`.

Exit criteria: anonymous use cannot create unbounded storage/CPU cost, capability
tokens are handled as secrets, and operators can detect capacity or cleanup
failures.

Phase 5 completed with transactional book/storage quotas, bounded ZIP inspection,
fixed-window request limits, global upload/download/conversion concurrency,
converter timeout and Unix resource limits, restrictive response headers, and
token-free service metrics. Hosted startup requires an HTTPS public origin and
creation access code. The public-hosting review found no unresolved blocker for
the single-process MVP; deployment proxy logging and disk alerts remain Phase 6
operator configuration.

## Phase 6 — Deployment and Launch

- [x] Package and deploy the single-instance Rust service, SQLite database,
  `kepubify`, and persistent temporary storage.
- [x] Configure domain, HTTPS, request-size limits, and token-redacted logging.
- [x] Document environment variables and operational procedures.
- [x] Test graceful restart during uploads, conversions, and downloads.
- [x] Run browser, mobile, and physical Kobo acceptance tests.
- [x] Load-test the configured quotas and conversion concurrency.
- [x] Add a rollback procedure and launch checklist.

Exit criteria: the service meets product acceptance criteria in the hosted
environment and can be operated without manual shelf cleanup.

Phase 6 was marked complete by the product owner on 2026-07-15. The portable
container deployment contract, runtime configuration, release checks, and
rollback procedure are documented in `docs/operations.md`.

## Later, Only If Needed

- Object storage and a shared database for multiple application instances.
- Separate read-only and manager capabilities.
- Human-readable shelf names or optional PINs.
- Server-Sent Events/WebSockets as an enhancement over polling.
- Explicit “keep shelf longer” controls.
- Abuse reporting or account-backed durable shelves.
