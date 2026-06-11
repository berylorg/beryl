# Reason For Investigation

Beryl needed a reusable exploration-memory note for the legacy dependency investigation migrated from doc/deps/getrandom/0.4.2.md. The migration preserves source-entrypoint, feature, lifecycle, gotcha, command, and unresolved-question findings that future dependency work may reuse.

# Outcome

The legacy note is preserved below as a dependency exploration memory note for crates.io package getrandom 0.4.2. It is supporting research only; design decisions remain in design docs and implementation sequencing remains in doc/plan.md.

# Sources

- Legacy note: doc/deps/getrandom/0.4.2.md.
- Source identity: crates.io package getrandom 0.4.2.
- Workspace dependency context: Cargo.toml and Cargo.lock in this repository at migration time.
- Additional upstream files, commands, feature flags, local use sites, and follow-up sources are listed in the migrated legacy details below.

# Migrated Legacy Details

## getrandom 0.4.2

Verified on 2026-05-02.

### Workspace Use

- `beryl-backend` uses `getrandom` directly to fill high-entropy capability-token bytes for each managed app-server launch.
- `Cargo.lock` resolves `getrandom 0.4.2` directly for Beryl and transitively through existing dependencies.
- Current resolved feature graph enables `getrandom`'s default feature only; that feature does not enable `std`, `sys_rng`, or `wasm_js`.
- Direct Beryl token generation only needs `getrandom::fill`.

### Symbols Needed By This Workspace

- `getrandom::fill`
- `getrandom::Error`

### Lifecycle And Platform Notes

- `fill(&mut [u8])` fills the whole destination from the system-preferred random source or returns an error; an empty buffer succeeds without an OS call.
- On supported Windows targets, the default backend uses the Windows random API selected by `getrandom`; for ordinary Windows 10+ targets this is `ProcessPrng`.
- On supported Linux targets, including WSL-Linux builds, the default backend uses Linux system randomness through the crate's selected Linux backend.
- Blocking is possible during early boot; this is acceptable for per-launch token generation but should stay outside the `gpui` thread.
- The crate is `no_std` by default. Enabling `std` adds standard error integrations but is not required for filling token bytes.

### Integration Gotchas

- Treat `getrandom::fill` failure as a launch-blocking backend initialization error; do not fall back to weak randomness for capability tokens.
- Generate enough random bytes before text encoding so the final token has high entropy independent of encoding alphabet.
- Do not log raw token bytes or the encoded bearer token.
- `getrandom_backend` cfg overrides affect backend selection globally for this crate version; Beryl should not set one for the normal host-Windows or WSL-Linux path.
- `wasm32-unknown-unknown` is unsupported unless `wasm_js` is enabled, which is outside Beryl's native desktop target.

### Minimal Upstream Entrypoints

- `getrandom-0.4.2/src/lib.rs`
- `getrandom-0.4.2/src/error.rs`
- `getrandom-0.4.2/src/backends.rs`
- `getrandom-0.4.2/src/backends/windows.rs`
- `getrandom-0.4.2/src/backends/getrandom.rs`
- `getrandom-0.4.2/src/backends/linux_android_with_fallback.rs`

### Commands And Files Consulted

- `cargo info getrandom@0.4.2`
- `cargo tree --invert getrandom@0.4.2`
- `cargo tree -e features --invert getrandom@0.4.2`
- `Select-String -Path Cargo.lock -Pattern 'name = "getrandom"' -Context 0,15`
- `Get-Content -Raw Cargo.toml`
- `Get-Content -Raw crates/beryl-backend/Cargo.toml`
- `Get-Content -Raw doc/design.md`
- `Get-Content -Raw crates/beryl-backend/doc/design.md`
- `Get-Content` and `rg` over the upstream source entrypoints listed above.

### Unresolved Questions

- None for the current workspace use.

