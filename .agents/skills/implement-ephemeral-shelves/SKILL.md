---
name: implement-ephemeral-shelves
description: Implement and refactor Epub Drop's temporary QR-shareable shelf lifecycle. Use for tasks involving shelf creation or joining, capability-scoped routes, SQLite persistence, shelf-specific book storage, cross-device polling, inactivity expiration, cleanup, or migrations away from the global library.
---

# Implement Ephemeral Shelves

Implement one coherent shelf-lifecycle change at a time while preserving shelf
isolation, file safety, and Kobo-browser compatibility.

## Workflow

1. Read `AGENTS.md`, `docs/architecture.md`, `docs/roadmap.md`, and relevant
   accepted decisions in `docs/decisions.md`.
2. Inspect the current code and tests. State whether the task is maintenance,
   a documented deferred item, or a new proposal; identify any design/code
   conflict.
3. Define the lifecycle and failure cases before editing:
   - Which shelf capability authorizes the operation?
   - Which database and filesystem states can be partially completed?
   - What happens if the process restarts at each boundary?
   - Does the action update presence, activity, revision, or expiration?
4. Make the smallest vertical change that can be tested. Keep storage access
   behind repository/service boundaries rather than embedding SQL or path logic
   throughout handlers.
5. Test the happy path, cross-shelf denial, missing/expired shelf behavior,
   concurrent mutation where relevant, and cleanup/restart failure cases.
6. Run formatting, linting, and tests. Update roadmap checkboxes only when their
   exit behavior is actually implemented and verified.

## Invariants

- Resolve every book through an authorized shelf; never query or mutate by book
  ID alone.
- Treat capability tokens as secrets and redact them from observable output.
- Construct disk paths only from server-generated internal IDs.
- Stream EPUB data and publish a book only after conversion succeeds.
- Use transactions for related metadata changes and compensating/idempotent
  operations across database/filesystem boundaries.
- Inject time into expiration logic so tests do not sleep or depend on wall time.
- Ensure polling cannot extend shelf life forever.
- Keep the critical frontend compatible with conservative Kobo JavaScript.

## Scope Control

Do not introduce object storage, distributed queues, WebSockets, accounts, or
multi-instance coordination without an explicit user request and an updated
architecture decision. Record newly required product choices in
`docs/decisions.md` instead of hiding them as implementation defaults.
