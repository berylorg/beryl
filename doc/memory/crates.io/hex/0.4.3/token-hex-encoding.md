# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/hex/0.4.3.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package hex 0.4.3. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/hex/0.4.3.md.
- Source identity: crates.io package hex 0.4.3.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## hex 0.4.3

Verified: 2026-05-08

Enabled features:

- `alloc`
- `default`
- `std`

Declared feature shape:

- `default = ["std"]`
- `std = ["alloc"]`
- `alloc = []`
- `serde` is declared but not enabled.

Workspace use:

- `beryl-backend` uses `hex::encode` to encode random token and nonce bytes as hex strings for managed backend authentication material.

Symbols used:

- `hex::encode<T: AsRef<[u8]>>(data) -> String`

Relevant invariants:

- `hex::encode` requires the `alloc` feature and returns a `String`.
- `hex::encode` accepts any `AsRef<[u8]>`.
- `hex::encode` emits lowercase hex using `0123456789abcdef`.
- Encoded output length is exactly `input.len() * 2`.

Upstream entrypoints:

- `hex-0.4.3/src/lib.rs`

Commands and files consulted:

- `cargo metadata --format-version 1`
- `cargo tree --target all -i hex -e features`
- `Cargo.lock`
- `Cargo.toml`
- `crates/beryl-backend/Cargo.toml`
- `crates/beryl-backend/src/auth.rs`
- `hex-0.4.3/src/lib.rs`

