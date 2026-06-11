# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/redb/4.1.0.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package redb 4.1.0. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/redb/4.1.0.md.
- Source identity: crates.io package redb 4.1.0.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## redb 4.1.0

- Verification date: 2026-05-08
- Enabled features in this workspace: none

### Why Beryl Uses It

Beryl uses `redb` as the embedded pure-Rust storage engine for per-workspace semantic state under the configured Beryl home directory's `workspaces/` child, defaulting to `~/.beryl/workspaces/`.

### Symbols Used By This Workspace

- `Database`
- `ReadableDatabase`
- `TableDefinition`

### Lifecycle And Threading Notes

- `Database::create` and `Database::open` attach to one on-disk database file.
- Reads and writes happen through explicit transactions opened from the database handle.
- `begin_read` and `begin_write` are synchronous filesystem operations and must stay off the `gpui` thread, which matches the workspace responsiveness contract.
- redb supports concurrent readers with a single writer transaction model, which is sufficient for Beryl's current single-GUI-instance ownership model.

### Integration Gotchas

- Table schemas are identified by string table names and Rust key/value types via `TableDefinition`.
- Beryl currently stores workspace manifests as serialized JSON blobs inside one metadata table rather than spreading small records across multiple tables.
- If a workspace database file exists but the metadata table or manifest key is missing, Beryl currently treats that as an unreadable workspace record rather than silently synthesizing data.

### Minimal Upstream Entrypoints

- `redb-4.1.0/src/lib.rs`
- `redb-4.1.0/src/db.rs`
- `redb-4.1.0/tests/basic_tests.rs`

### Verification Inputs

- Commands run:
  - `cargo search redb --limit 5`
  - `cargo info redb`
  - `rg -n "pub fn open\\(|pub fn create\\(" <redb-4.1.0>/src/db.rs`
  - `rg -n "TableDefinition<.*\\[u8\\]|insert\\(.*\\&\\[|value\\(\\)" <redb-4.1.0>`
- Workspace files consulted:
  - `Cargo.toml`
  - `crates/beryl-app/Cargo.toml`
  - `crates/beryl-app/src/workspace_persistence.rs`
  - upstream entrypoints listed above

