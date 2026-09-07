# Reason For Investigation

The complete source simplification audit found repeated manual Display and Error implementations
in Beryl's model and state domains. Determine whether the already selected thiserror version can
remove that code without changing public errors or diagnostic chains.

# Outcome

thiserror 2.0.18 is already a workspace dependency and is consumed by backend and stream. Its
Error derive supports the field interpolation needed by the nine model error enums inspected in
audit finding MS-001. A model dependency declaration and exact message attributes can replace their
manual formatting and empty Error implementations without a package-version upgrade.

Apply this only after comparing each error's behavior. A field named `source` is inferred as the
error source even without an explicit attribute. `from` implies a source; transparent forwarding
also changes behavior. Preserve existing source chains and conversions, or retain manual exceptions
where a derive would expose a previously hidden cause. Check implicit backtrace behavior too.

The crate has a long-established 1.x/2.x release line, dual MIT/Apache-2.0 licensing, and a declared
Rust 1.68 minimum below Beryl's 1.92 minimum. This investigation establishes API fit for inspected
errors. No build, benchmark, or advisory scan was performed, and no dependency was added or upgraded.

# Sources

- Registry authority: crates.io, thiserror 2.0.18, inspected 2026-09-06. Default features select
  `std`; the matching derive implementation is thiserror-impl 2.0.18.
- [Versioned API documentation](https://docs.rs/thiserror/2.0.18/thiserror/), especially Display,
  named-source inference, from, transparent forwarding, and backtrace behavior.
- [Versioned package metadata](https://docs.rs/crate/thiserror/2.0.18).
- [Tagged manifest](https://raw.githubusercontent.com/dtolnay/thiserror/2.0.18/Cargo.toml), used for
  license, feature, and MSRV information.
- Beryl baseline `e6172f7c49bebef78d51831e621e70c8ac0d6a07`: root `Cargo.toml`, `Cargo.lock`,
  model/backend/stream member manifests; all model error bodies; state error bodies inspected by
  the corresponding full-file audit.
- [Model/stream evidence](../../../../audits/code-simplification/reviews/model-stream-findings.json),
  MS-001. The lockfile also contains transitive thiserror 1.0.69; this proposal does not upgrade it.
