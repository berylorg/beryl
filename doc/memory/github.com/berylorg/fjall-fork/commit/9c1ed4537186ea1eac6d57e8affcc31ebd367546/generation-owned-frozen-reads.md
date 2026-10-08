# Reason For Investigation

Frozen Thread Switcher queries must preserve one coherent collection while live metadata changes,
without allowing external query handles to retain a retired Beryl Home database generation.

# Outcome

`Database::snapshot` captures a snapshot nonce and clones the database. `Snapshot` therefore owns
both a version pin and database lifetime. Dropping `SnapshotNonce` closes its tracker pin. An
escaped caller-owned snapshot would defeat Beryl's non-owning generation-handle retirement contract.

The reusable mechanism is a snapshot held solely inside the Home generation, accessed through
opaque non-owning identities and admitted bounded typed reads. Release and generation retirement
must drain admitted reads and drop retained snapshots before disposing the generation. Existing
ordinary Home reads capture a new short-lived snapshot per call; revision sandwiches detect drift
but cannot implement the retained immutable collection.

# Sources

- Canonical remote: https://github.com/berylorg/fjall-fork.git; requested ref: checked-out HEAD;
  resolved commit: `9c1ed4537186ea1eac6d57e8affcc31ebd367546`; inspected 2026-10-08. Source worktree
  was clean. Beryl resolves local `fjall-fork` exactly at Cargo version `3.1.6`.
- `src/db.rs`, `Database::snapshot`: nonce capture and database clone.
- `src/snapshot.rs`, `Snapshot`: retained nonce and database ownership.
- `src/snapshot_nonce.rs`, `SnapshotNonce::drop`: tracker pin closure.
- Beryl `crates/beryl-home-store/src/read.rs`: ordinary per-call snapshot and bounded typed reads;
  `src/store.rs`: generation retirement and attachment disposal.
- Reproduction: inspect the above symbols at the exact commit; compare Beryl `Cargo.toml` and
  `Cargo.lock` dependency identity and `git status --short` in the owned fork.
