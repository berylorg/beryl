# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/tempfile/3.27.0.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package tempfile 3.27.0. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/tempfile/3.27.0.md.
- Source identity: crates.io package tempfile 3.27.0.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## tempfile 3.27.0

Verified on 2026-05-08.

### Workspace Use

- `beryl-app` uses `tempfile` for short-lived adjacent files used by durable settings, startup metadata, and workspace image-asset writes.
- Tests use `TempDir` for isolated filesystem roots.
- `Cargo.lock` resolves `tempfile 3.27.0`; the workspace centralizes the version in the root `Cargo.toml`.
- Current resolved feature graph enables `tempfile`'s default feature, which includes `getrandom`.

### Symbols Needed By This Workspace

- `tempfile::TempDir`
- `tempfile::NamedTempFile`
- `tempfile::Builder`
- `tempfile::tempdir`
- `NamedTempFile::new_in`
- `NamedTempFile::as_file`
- `NamedTempFile::persist`
- `TempDir::path`
- `TempDir::close`

### Lifecycle And Platform Notes

- `TempDir` removes its directory tree on drop; `close` reports cleanup errors explicitly.
- `NamedTempFile` removes its path on drop. Cleanup errors during drop are ignored; use explicit APIs when cleanup failure must be observed.
- `NamedTempFile::new_in` creates the temporary path in the requested directory, which keeps later persistence on the same filesystem as the destination.
- `NamedTempFile::persist` moves the temporary file to the target path and atomically replaces an existing target where the platform supports that operation.
- `persist` does not sync file contents or the containing directory. Call `flush` or `sync_all` explicitly before persisting when the Beryl write path requires it.
- On Windows, overwrite persistence uses `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING`.

### Integration Gotchas

- Production code should depend on `tempfile` directly with `workspace = true`; do not rely on transitive access through `gpui`.
- A failed `persist` returns the underlying `io::Error` plus the still-owned temporary file. Map the error while letting the returned file drop so normal cleanup runs.
- `persist_noclobber` fails when the destination exists and has weaker portability guarantees than overwrite `persist`; use it only when no-overwrite behavior is required.
- `tempfile` is not a durability abstraction. Keep Beryl-owned ordering, flushing, and metadata-commit policy explicit.
- `NamedTempFile` uses a visible path until cleanup, so it should be used only for short-lived adjacent write staging, not for durable preview or user-facing storage.

### Minimal Upstream Entrypoints

- `tempfile-3.27.0/src/lib.rs`
- `tempfile-3.27.0/src/file/mod.rs`
- `tempfile-3.27.0/src/file/imp/windows.rs`
- `tempfile-3.27.0/src/file/imp/unix.rs`

### Commands And Files Consulted

- `cargo metadata --format-version 1 --no-deps`
- `cargo tree -e features -p beryl-app -i tempfile`
- `Select-String -Path Cargo.lock -Pattern 'name = "tempfile"' -Context 0,8`
- `Select-String -Path Cargo.toml -Pattern '^tempfile' -Context 0,2`
- `rg -n "tempfile::|NamedTempFile|tempfile\\(" crates`
- `Get-Content` and `Select-String` over the upstream source entrypoints listed above.

### Unresolved Questions

- None for the current workspace use.

