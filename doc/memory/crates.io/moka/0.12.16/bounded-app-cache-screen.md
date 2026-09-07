# Reason For Investigation

On 2026-09-07, application subsystem synthesis screened Moka as a replacement for repeated
syntax-highlighting and code-panel projection cache mechanics in AS-001. Beryl's caches require
synchronous count, source-byte and retained-byte admission, owner and request identity, coalescing,
worker-retained accounting and different pending-display policies.

# Outcome

Do not adopt Moka for this simplification. Its concurrent storage has best-effort, eventually
consistent capacity/weight policy. Beryl would still need its exact admission and release rules,
request tickets, pending work and display/action coherence. A private shared cache core can remove
duplication without hiding those requirements; no separate dependency savings are established.

The screened configuration is 0.12.16 with defaults disabled and `sync` enabled. The exact manifest
declares Rust 1.71.1 and license expression `(MIT OR Apache-2.0) AND Apache-2.0`. Dependencies include
crossbeam channels/epoch/utils, equivalent, parking_lot, portable-atomic, smallvec, tagptr and UUID.
A hypothetical Beryl dependency graph was not resolved or built. The tagged README reports Linux
CI coverage; Windows and macOS support there is expected rather than CI-tested, and Wasm/WASI and
no_std are unsupported.

The non-yanked release was published on 2026-08-09. The official crates.io production repository
declared exact 0.12.16 with the `future` feature when inspected. This establishes real project
adoption, but not Beryl's synchronous bounded fit. The changelog records older memory-safety and
compute-race fixes. A scoped RustSec search found no direct package advisory; no full graph security
claim follows. No installation or runtime verification was performed.

The two existing pruning scans are each only 17 lines. An LRU-only replacement was also
[screened out](../../lru/0.18.3/app-pruning-screen.md); it would leave the same accounting and
lifecycle responsibilities. Full source, cost and version evidence is in the
[application synthesis](../../../../audits/code-simplification/reviews/subsystem-app.json).

# Sources

- [Exact registry release](https://crates.io/api/v1/crates/moka/0.12.16).
- [Versioned cache API and best-effort bounds](https://docs.rs/moka/0.12.16/moka/sync/struct.Cache.html).
- [Versioned package and feature documentation](https://docs.rs/moka/0.12.16/moka/).
- [Tagged manifest](https://raw.githubusercontent.com/moka-rs/moka/v0.12.16/Cargo.toml).
- [Tagged platform notes](https://raw.githubusercontent.com/moka-rs/moka/v0.12.16/README.md).
- [Tagged changelog](https://raw.githubusercontent.com/moka-rs/moka/v0.12.16/CHANGELOG.md).
- [Official crates.io production manifest](https://raw.githubusercontent.com/rust-lang/crates.io/main/Cargo.toml), observed 2026-09-07.
