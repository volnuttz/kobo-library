# Release Process

Use this process for every public Epub Drop release. It intentionally separates
creating a reproducible release from choosing an OCI registry or hosting
platform.

## Versioning

Use semantic versions. The crate version in `Cargo.toml` is `X.Y.Z`; the Git
release tag is `vX.Y.Z` on the same commit.

- Patch: compatible bug, security, or operational fix.
- Minor: compatible user-visible feature.
- Major: incompatible configuration, route, or deployment contract change.

Do not move or reuse a published release tag. If GitHub immutable releases are
available, create the release as a draft with all assets and notes, then publish
it as immutable.

## Prepare

1. Work from a clean `main` checkout and choose the next version.
2. Update `Cargo.toml`, user-facing documentation, and the release notes.
3. Run the local verification suite:

   ```sh
   cargo fmt -- --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test
   docker build --pull --tag epub-drop:release .
   ```

4. Confirm the CI workflow passed for the exact candidate commit.
5. Build the OCI image using the chosen registry's normal process. Publish a
   version tag and deploy by its immutable digest, not a mutable tag. Record the
   digest in the release record.

## Release gate

Before publishing or deploying, complete the release checks in
`docs/operations.md`, including hosted HTTPS/QR behavior, a graceful restart,
browser/mobile/Kobo acceptance, quota/concurrency load testing, and private
metrics/token-redacted logging. Keep evidence redacted: shelf URLs and access
codes are secrets.

## Publish

1. Create and push the annotated Git tag `vX.Y.Z` only after the gate passes.
2. Create a GitHub release from that tag with the notes below. Attach artifacts
   only if they are reproducible and their checksums are included.
3. Publish/deploy the OCI image only by the recorded digest.
4. Record the deployment in the release notes or the team's deployment system.

## Release-note template

```markdown
## Epub Drop vX.Y.Z

### Summary
- User-visible changes.

### Operations
- OCI image digest: `sha256:...`
- Schema/migration compatibility: compatible / forward-only migration name.
- Deployment configuration changes: none / describe without secrets.

### Verification
- CI: link to the passing run.
- Hosted checks: HTTPS/QR, restart, browser/mobile/Kobo, and load-test evidence.

### Rollback
- Previous compatible image digest: `sha256:...`
- Notes: rollback is safe / use a forward fix because of migration X.
```

Use a forward corrective release rather than restoring only SQLite or only
files. See `docs/operations.md` for the storage and rollback constraints.
