# Architecture Decisions

This file records decisions that future work should not repeatedly reopen
without new evidence. Update it when a decision is made or superseded.

## Accepted

### ADR-001: Capability URLs provide shelf membership

All devices with the shelf's high-entropy URL receive the same upload, download,
and delete rights. Accounts and pairing codes are outside the MVP.

### ADR-002: Expire by inactivity, not disconnect detection

HTTP disconnect state is unreliable, especially for sleeping e-readers.
Expiration uses server timestamps. Presence may be displayed but is not the
sole authority for content retention.

### ADR-003: Poll for cross-device updates

Use conditional periodic polling for the core experience because it is simple
and compatible with older Kobo browsers. Real-time transports may be added as a
progressive enhancement later.

### ADR-004: Use SQLite and local shelf-scoped files for the MVP

The first hosted version is a single application instance. SQLite replaces the
global JSON metadata file, while converted books remain on local persistent
storage. Distributed storage is deferred until scaling requires it.

### ADR-005: Cleanup is asynchronous and idempotent

Requests do not synchronously erase entire shelves at the expiry boundary. A
worker claims expired shelves, prevents new mutations, and retries cleanup until
metadata and files are reclaimed.

### ADR-006: Kobo compatibility constrains the critical UI

The primary workflow must work with simple HTML and conservative JavaScript on
a physical Kobo. Modern browser features may enhance but cannot gate the flow.

### ADR-007: Shelves expire after 12 inactive hours and 24 total hours

For the MVP, meaningful activity extends a shelf to 12 hours from that activity,
but never beyond 24 hours from creation. Background polling and passive
heartbeats do not extend either deadline. The application maintainer owns this
policy. Revisit it after launch if cleanup lag, storage pressure, or observed
sharing sessions show that either value is unsuitable.

### ADR-008: Each shelf is limited to 100 MB per EPUB, 20 books, and 500 MB

All three limits apply independently. Uploads that would exceed a limit are
rejected before conversion where possible, and conversion output is also
checked before publication. The application maintainer owns these defaults.
Revisit them using conversion duration, rejection, and disk-capacity metrics.

The current trusted-network `MAX_UPLOAD_MB=800` setting is legacy behavior, not
the hosted-service quota. Phase 3 introduces shelf quotas and Phase 5 completes
the resource-exhaustion controls required for public hosting.

### ADR-009: Shelf creation initially requires a deployment access code

The landing/creation flow requires one deployment-wide access code. Once a
shelf exists, its capability URL alone authorizes shelf operations; the access
code must not appear in that URL. The deployment owner owns and rotates the
code. Revisit fully public creation after rate limits and operational metrics
have been proven under real traffic.

Implementation note: setting `SHELF_ACCESS_CODE` enables the gate. Leaving it
unset is an explicit local-development mode in which `GET /` creates a shelf
immediately; hosted deployment configuration must set it.

### ADR-010: The first deployment uses one Fly.io Machine and managed HTTPS

The first hosted environment is one Fly.io Machine, one persistent volume for
SQLite and shelf files, and Fly Proxy TLS termination with HTTPS-only public
traffic. The deployment owner owns platform configuration. Do not scale beyond
one Machine while SQLite and files are local; revisit the platform if volume
cost, regional requirements, or operational experience justify a move.

### ADR-011: Hosted mode starts with an empty library

There is no automatic import of the trusted-network `books.json` library into
temporary shelves. Operators may retain their old data directory separately.
The application maintainer owns documenting this boundary in the Phase 1
migration and release notes.

### ADR-012: Explicit access and content actions extend inactivity

Entering a shelf through its canonical page, uploading, downloading, or
deleting a book counts as meaningful activity. Conditional polling, QR image
loads, and background heartbeats do not. The application maintainer owns the
precise service-level implementation and deterministic tests in Phase 4.

### ADR-013: Downloads already started at expiration get a short grace period

Expiration rejects new downloads and mutations. A download authorized and
started while the shelf was active may finish for up to five minutes before it
is terminated so cleanup can proceed. The application maintainer owns enforcing
and testing this bound in Phase 4.

## Open Decisions

No decision currently blocks the MVP roadmap. New evidence or implementation
constraints should be recorded here before changing an accepted default.
