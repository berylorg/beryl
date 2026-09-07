# Reason For Investigation

Audit SS-009 examined whether a mature serializer could replace substantial handwritten Syndic
record-value encoders and decoders after full review of 107 schema/codec/value/test files.

# Outcome

Postcard 1.1.3 is a concrete candidate only for a newly authorized record format. Retain V7 codecs
and first consolidate their explicit scalar tags, common value bodies and existing primitives.
V7 fixes integer/identity bytes, family versions, tags, collection bounds, digest preimages and EOF
rejection. Postcard's default varints, positional enum discriminants and length representation differ.
fixint::be can preserve scalar integers but does not automatically preserve u32 string lengths,
optional layouts or every exact tag. Recreating V7 with custom adapters may erase the saving.

If a new schema is independently authorized, prototype bounded wire DTOs for record values only.
Keep ordered keys, canonical hash preimages and streaming validators explicit. Deserialization must
not bypass domain constructors: borrow bounded input, validate lengths/counts before allocation,
then construct invariant-bearing records. Semantic cross-record validation still applies. Use an
explicit remainder check, such as take_from_bytes plus empty remainder, for exact EOF. Define
format retirement and compatibility deliberately; current authority does not allow old-format
adapters. No defensible net saving is counted before the prototype and contract decision.

The exact latest stable version was released 2025-07-24, non-yanked at the 2026-09-06 registry check,
and is MIT OR Apache-2.0. It has a documented stable 1.x wire specification, Mozilla sponsorship
of 1.0 work, and actual current adoption by rust-analyzer's proc-macro-api. Chromium vendor evidence
is evaluation evidence with Shipped:no, not proof of deployed use. The separately developed
postcard2 0.x is outside this assessment.

For bounded Vec-backed values, disable defaults and enable alloc; fixed-slice use may need no alloc.
Default heapless-cas is unnecessary. Published required dependencies include cobs ^0.3.0 and serde
^1.0.100 with derive and defaults disabled. Optional std, heapless, CRC, defmt and experimental derive
are unnecessary. The no_std-focused Rust implementation has no target-specific manifest sections,
but its published manifest and registry declare no fixed MSRV. Edition 2021 is not a Rust 1.92
compatibility guarantee; Beryl builds remain unverified.

Targeted RustSec searches found no postcard/cobs-specific advisory on the investigation date.
Historical heapless RUSTSEC-2020-0145 concerns versions before 0.6.1 and is outside the proposed
disabled heapless feature. This is not a resolved dependency-closure audit or security clearance.
No package was installed, dependency changed, prototype built or test run.

# Sources

- [Registry metadata](https://crates.io/api/v1/crates/postcard), checked 2026-09-06.
- [Exact manifest](https://docs.rs/crate/postcard/1.1.3/source/Cargo.toml),
  [versioned API](https://docs.rs/postcard/1.1.3/postcard/),
  [wire specification](https://postcard.jamesmunns.com/wire-format.html), and
  [fixed-integer adapters](https://docs.rs/postcard/latest/x86_64-pc-windows-msvc/postcard/fixint/index.html).
- [Project](https://github.com/jamesmunns/postcard) and
  [1.1.3 release](https://github.com/jamesmunns/postcard/releases/tag/postcard%2Fv1.1.3).
- [rust-analyzer workspace](https://github.com/rust-lang/rust-analyzer/blob/master/Cargo.toml) and
  [proc-macro-api consumer](https://github.com/rust-lang/rust-analyzer/blob/master/crates/proc-macro-api/Cargo.toml).
- [Chromium vendor record](https://chromium.googlesource.com/chromium/src/+/dea0f70457cfafa8e8473a2bc310d314629df799%5E%21/).
- [RustSec](https://rustsec.org/advisories/) and
  [historical heapless advisory](https://rustsec.org/advisories/RUSTSEC-2020-0145.html).
- Beryl baseline e6172f7c49bebef78d51831e621e70c8ac0d6a07: all assigned Syndic codecs,
  records and values, design-schema-v7.md and package rigor; full evidence in
  [SS-009](../../../../audits/code-simplification/reviews/syndic-schema-findings.json).
