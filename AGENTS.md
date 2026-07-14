# Epub Drop Agent Guide

## Mission

Evolve Epub Drop from one persistent trusted-network library into an
internet-hosted service providing temporary, QR-shareable EPUB shelves while
preserving compatibility with Kobo's older browser.

## Read First

Before planning or changing shelf behavior, read:

- `docs/roadmap.md` for scope, order, and completion criteria.
- `docs/architecture.md` for the target model and invariants.
- `docs/decisions.md` for settled decisions and unresolved product questions.

Read the existing implementation before editing it. Treat documentation as the
target design and tests/code as the current behavior. If they conflict, call out
the conflict rather than silently choosing one.

## Repository Skills

Project skills live under `.agents/skills/` so Codex can discover them
automatically for this repository.

- Use `.agents/skills/implement-ephemeral-shelves/SKILL.md` when implementing or
  refactoring shelf creation, joining, storage, routes, synchronization, or
  expiration.
- Use `.agents/skills/review-public-hosting/SKILL.md` when a change affects anonymous
  uploads, public routes, capability tokens, conversion, cleanup, quotas, or
  deployment safety.

Use both for an internet-facing shelf milestone.

## Working Rules

- Keep changes aligned with the current roadmap milestone; avoid speculative
  infrastructure for later phases.
- Preserve the simple server-rendered HTML and conservative JavaScript style
  unless browser support is deliberately revised.
- Treat shelf URLs as bearer credentials. Never print full tokens in logs,
  errors, analytics, or test snapshots.
- Scope every book lookup and mutation by shelf. A book ID alone must never
  authorize access.
- Use database transactions for multi-step metadata mutations and make cleanup
  idempotent.
- Stream uploads and downloads; do not buffer whole EPUB files in memory.
- Preserve unrelated user changes in the working tree.
- Update the roadmap and relevant design documentation when a milestone or
  architectural decision changes.

## Verification

Run checks appropriate to the change, normally:

```sh
cargo fmt -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

For lifecycle work, also test isolation between shelves, expiration boundaries,
restart behavior, interrupted uploads, and cleanup retries. If a required tool
is unavailable, report which checks could not run.
