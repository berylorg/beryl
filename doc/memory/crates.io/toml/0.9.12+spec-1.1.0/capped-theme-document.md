# Reason For Investigation

Audit ST-014 evaluated replacing the 909-line theme-document parser and helpers with Beryl's
existing toml/serde dependencies now that theme documents have a fixed 256 KiB byte ceiling.

# Outcome

toml 0.9.12+spec-1.1.0 with serde 1.0.228 is a concrete conditional replacement, estimated at
350-550 net lines after raw records, bound checks and error adapters. It is an alternative to
ST-011 lexical sharing, not additive. The current authority requires incremental parsing and no
whole-document residency. Adoption therefore first requires an explicit exception for one capped
document parse, a finite concurrent-parser envelope, and agreed grammar/error behavior. Keep
manifest streaming/paging, physical identity checks and staged publication.

The parser materializes tokens, events and a DeTable before conversion. A 256 KiB input ceiling
does not bound allocations to 256 KiB. Keep the default 80-level recursion guard and leave unbounded
disabled. Measure worst-case tiny-token and nested-unknown-value input before adoption. Read at
most cap+1 bytes, check UTF-8 and existing role/property/line bounds, deserialize typed raw theme
records, then apply current semantic validation. The direct ValueDeserializer supports u64/u128;
toml::Value::Integer is i64 and is unsuitable as an intermediate for full-width unsigned fields.

Standard TOML 1.1 accepts more syntax than the current compact parser. Current InstalledLoad can
skip malformed unknown values before parsing them, whereas standard TOML validates syntax first.
Both admitted grammar and this omission behavior require explicit decisions. Preserve canonical
serialization if exact bytes are to remain stable. A compatibility and allocation prototype has
not been run; this investigation adds no dependency or implementation.

This exact already-resolved version was published 2026-02-10, was not yanked, and had approximately
65.4 million version downloads at the registry check on 2026-09-06. Cargo uses toml in production.
License is MIT OR Apache-2.0, MSRV 1.76, with native Rust implementation and no external install.
Defaults are std, serde, parse and display. preserve_order, fast_hash and debug are unnecessary
for the proposed semantic parsing. An exact-package OSV query returned an empty object on that
date; this is not a transitive dependency or future advisory clearance.

# Sources

- [Exact registry metadata](https://crates.io/api/v1/crates/toml/0.9.12%2Bspec-1.1.0).
- [Tagged package manifest](https://raw.githubusercontent.com/toml-rs/toml/toml-v0.9.12/crates/toml/Cargo.toml)
  and [workspace manifest](https://raw.githubusercontent.com/toml-rs/toml/toml-v0.9.12/Cargo.toml).
- [Typed value deserializer](https://raw.githubusercontent.com/toml-rs/toml/toml-v0.9.12/crates/toml/src/de/deserializer/value.rs),
  [parser](https://raw.githubusercontent.com/toml-rs/toml/toml-v0.9.12/crates/toml/src/de/parser/mod.rs),
  and [Value representation](https://raw.githubusercontent.com/toml-rs/toml/toml-v0.9.12/crates/toml/src/value.rs).
- [Cargo adoption](https://raw.githubusercontent.com/rust-lang/cargo/master/Cargo.toml).
- [OSV query endpoint](https://api.osv.dev/v1/query), queried for crates.io toml version
  0.9.12+spec-1.1.0 on 2026-09-06 by the state auditor.
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: Cargo.toml, Cargo.lock,
  theme/document.rs and all theme tests; current theme-runtime and state package authority.
  [ST-014](../../../../audits/code-simplification/reviews/state-findings.json) retains full evidence.
