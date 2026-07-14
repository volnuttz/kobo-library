# Trusted-Network Baseline

This records the trusted-network behavior captured before the ephemeral-shelf
refactor. It is a historical refactoring baseline, not a description of the
current persistence implementation and not approval to expose the application
to the internet.

## Current Behavior

- One global library is stored in `DATA_DIR/books.json`; missing metadata means
  an empty library.
- Converted files live in `DATA_DIR/books/` and temporary uploads in
  `DATA_DIR/uploads/`.
- `GET /` serves one upload/list/delete page. All API and book routes operate on
  the same global collection without authentication.
- Upload bodies are streamed to a temporary file. Normal EPUBs are passed to
  `kepubify`; existing kepubs can skip conversion. Downloads are streamed.
- The QR code uses the request Host header and plain HTTP to point at `/`, which
  is suitable only for the current trusted-network mode.

## Characterization Coverage

Unit tests cover filename normalization, response-header sanitization, EPUB
metadata extraction, kepub marker detection, QR helper behavior, and host/port
formatting. Metadata tests also cover missing-file behavior and a write/read
round trip.

Before replacing a path in Phase 1, add characterization at its service or HTTP
boundary when its observable behavior is not covered above. Shelf lifecycle
tests belong with the phase that introduces the lifecycle rather than in this
trusted-network baseline.

## Known Gaps to Preserve as Gaps, Not Behavior

- Read-modify-write JSON mutations are not concurrency-safe or transactional.
- A metadata write failure after publishing a converted file can leave an
  orphan; delete removes the file before persisting metadata.
- Upload cleanup does not cover every converted temporary-output failure or a
  process crash.
- EPUB ZIP expansion and `kepubify` time/concurrency are unbounded.
- The legacy request limit defaults to 800 MB and there are no book-count,
  shelf-storage, rate, or global disk limits.
- Host-derived QR URLs, unauthenticated routes, and current responses are not
  safe public-service behavior.

These are roadmap inputs for Phases 1 through 5. Phase 0 does not broaden the
current application's deployment boundary: keep it on a trusted local network.
