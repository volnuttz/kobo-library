---
name: review-public-hosting
description: Review Epub Drop changes for safe internet hosting and bounded anonymous use. Use when code or configuration affects public endpoints, capability URLs, uploads/downloads, EPUB or ZIP parsing, kepubify execution, quotas, rate limiting, expiration cleanup, proxy/HTTPS behavior, logging, deployment, or operational readiness.
---

# Review Public Hosting

Audit a proposed or completed change against the public-hosting trust boundary.
Prioritize concrete exploit paths and operational failure modes over generic
security advice.

## Review Workflow

1. Read `docs/architecture.md`, `docs/decisions.md`, and the relevant diff,
   configuration, tests, and route/data flow.
2. Trace untrusted input from request to database, filesystem, subprocess,
   response, logs, and cleanup.
3. Review the controls below. For each finding, identify the affected path,
   realistic impact, and smallest robust remediation.
4. Verify claims with tests or local inspection where possible. Distinguish a
   confirmed defect from a defense-in-depth recommendation.
5. Report findings in severity order. State explicitly when no actionable issue
   is found and list any checks that could not run.

## Required Controls

### Authorization and secrecy

- Require a valid high-entropy capability for every shelf resource.
- Scope book access by shelf and resist cross-shelf ID substitution.
- Avoid token disclosure through logs, errors, redirects, analytics, third-party
  resources, referers, or cache keys.
- Configure cache behavior so private shelf responses are not shared.

### Resource exhaustion

- Bound request size/time, shelf creation, book count, stored bytes, archive
  expansion, conversion duration/concurrency, downloads, and global disk use.
- Stream large bodies and clean partial uploads after all failure paths.
- Treat ZIP parsing and `kepubify` as attacker-controlled work.

### Data and process safety

- Reject traversal and header injection; derive paths from internal IDs.
- Avoid shell interpolation when launching conversion.
- Handle database/file partial failure, restart, concurrency, and cleanup retry.
- Prevent expired or expiring shelves from accepting new mutations.

### Web and deployment boundary

- Require HTTPS in production and trust forwarded headers only from configured
  proxies.
- Review CSP, MIME types, download disposition, `noindex`, CSRF implications,
  and browser compatibility.
- Ensure metrics reveal capacity and cleanup lag without exposing credentials or
  book metadata unnecessarily.

## Severity Guide

- Critical: immediate broad compromise, arbitrary execution, or unbounded public
  abuse with severe impact.
- High: capability bypass/leak, cross-shelf access, traversal, archive/conversion
  exhaustion, or reliable loss of cleanup control.
- Medium: meaningful defense gap requiring conditions or with bounded impact.
- Low: limited hardening or operational visibility issue.

Do not approve public readiness merely because functional tests pass. Tie the
conclusion to the Phase 5 exit criteria in `docs/roadmap.md`.
