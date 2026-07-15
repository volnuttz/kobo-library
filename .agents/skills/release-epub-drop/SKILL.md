---
name: release-epub-drop
description: Prepare, verify, and publish Epub Drop releases. Use when Codex needs to cut a version, create release notes or tags, build an OCI release image, verify release readiness, or coordinate a rollback.
---

# Release Epub Drop

Use `docs/release-process.md` and `docs/operations.md` as the release source of
truth. Read `docs/decisions.md` when compatibility or deployment assumptions
change.

1. Inspect the working tree, current `Cargo.toml` version, existing tags, and
   candidate commit. Propose the next semantic version and identify release
   notes before changing anything.
2. Run the local verification suite and confirm CI passed for the exact commit.
3. Review the public-hosting release gate: HTTPS/QR behavior, secret handling,
   private metrics, token-redacted logs, restart recovery, and quota/load
   evidence. State missing evidence plainly.
4. Build the OCI image through the selected registry workflow. Record its
   immutable digest; do not treat a mutable image tag as a deployment identity.
5. Before any external write, obtain explicit user confirmation for each of:
   creating/pushing a Git tag, publishing a GitHub release, pushing an image,
   and deploying or rolling back a hosted service.
6. After publication, produce the release record from the template in
   `docs/release-process.md`, including the commit, tag, image digest,
   verification evidence, and rollback compatibility. Never include a shelf
   capability, access code, or unredacted request path.

Keep releases provider-neutral. Do not add registry or deployment defaults to
the repository without an explicit user decision.
