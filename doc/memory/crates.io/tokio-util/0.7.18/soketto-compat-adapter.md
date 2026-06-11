# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/tokio-util/0.7.18.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package tokio-util 0.7.18. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/tokio-util/0.7.18.md.
- Source identity: crates.io package tokio-util 0.7.18.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## tokio-util 0.7.18

Verified on 2026-05-10.

### Workspace Use

- `beryl-backend` uses `tokio-util` only to adapt Tokio I/O to futures I/O for the managed `soketto` WebSocket client path.
- Beryl enables `default-features = false` with the `compat` feature.
- The workspace already resolved `tokio-util` transitively through GPUI's HTTP stack, but `beryl-backend` needs a direct dependency so the transport module can use the compatibility adapter deliberately.

### Symbols Needed By This Workspace

- `tokio_util::compat::TokioAsyncReadCompatExt`
- `tokio_util::compat::Compat`

### Lifecycle And I/O Notes

- `TokioAsyncReadCompatExt::compat` adapts a Tokio `AsyncRead`/`AsyncWrite` value to the futures I/O traits expected by `soketto`.
- The adapter does not provide cancellation, backpressure, frame budgets, or timeout policy. Those remain owned by the backend transport and JSON-RPC session layers.
- The backend crate also enables Tokio `net` for async loopback TCP and `time` for bounded transport operations.

### Integration Gotchas

- Keep `tokio-util` use at the transport edge. It should not leak into typed backend normalization or GUI-facing API contracts.
- Do not rely on transitive GPUI-enabled `tokio-util` features for backend transport behavior; the backend direct dependency selects only `compat`.

### Minimal Upstream Entrypoints

- `tokio-util-0.7.18/src/lib.rs`
- `tokio-util-0.7.18/src/compat.rs`

### Commands And Files Consulted

- `cargo metadata --format-version 1`
- `cargo metadata --locked --format-version 1 --no-deps`
- `cargo tree -p beryl-backend -e features`
- `cargo tree -i tokio-util -e features`
- `Select-String -Path Cargo.lock -Pattern 'name = "tokio-util"' -Context 0,10`
- `rg -n "TokioAsyncReadCompatExt|pub struct Compat|pub mod compat" <cargo-registry>/tokio-util-0.7.18/src`
- `Cargo.toml`
- `crates/beryl-backend/Cargo.toml`

### Unresolved Questions

- None for the selected compatibility-adapter use.

