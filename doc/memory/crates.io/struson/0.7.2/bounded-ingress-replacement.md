# Reason For Investigation

Evaluate whether an existing pull JSON reader can replace backend continuation machines while
preserving exact incremental handling of arbitrarily long ignored names and scalar spellings.

# Outcome

struson 0.7.2 is the closest inspected off-the-shelf API shape. Its advanced Read-based interface
supports streaming string values. It has MIT/Apache-2.0 licensing, Rust 1.85 MSRV, and documented
Windows MSVC, Linux, and macOS targets. Its smaller 0.x release history does not establish the same
adoption history as serde_json; its simple API is explicitly experimental.

Member names and numbers are still exposed as complete scalar values. Default number restrictions
reject spellings over 100 characters and exponent magnitudes over 99; disabling those restrictions
does not provide bounded incremental name/number output. Beryl would retain schema, field-order,
identity, lease, and failure adapters and need to change its sole-recognizer contract.

No compatible net-reducing replacement was demonstrated. The pull-interface idea remains useful:
Beryl's existing bounded-json Parser already yields one owned event with consumed/produced extents,
allowing a smaller local cursor without introducing another recognizer.

# Sources

- Registry authority: crates.io, struson 0.7.2; checked 2026-09-06. Candidate uses the advanced API
  without optional `serde` or `simple-api` features; no package was installed or built.
- [Registry metadata](https://crates.io/api/v1/crates/struson), exact release, license, MSRV,
  feature, and release-history evidence.
- [Versioned crate documentation](https://docs.rs/struson/0.7.2/struson/).
- [JsonStreamReader](https://docs.rs/struson/0.7.2/struson/reader/struct.JsonStreamReader.html),
  member-name, number, and streaming-string APIs.
- [ReaderSettings](https://docs.rs/struson/0.7.2/struson/reader/struct.ReaderSettings.html),
  default numeric restrictions. The audit initially accessed these official pages through their
  latest alias, which resolved to 0.7.2 at the recorded date.
- Beryl baseline `e6172f7c49bebef78d51831e621e70c8ac0d6a07`: backend response machines and
  `incoming_json/provider.rs`; bounded-json consumed at clean commit
  `d3397fa5e41dfad7e7a1476f15048141e30568be`.
- [Complete local comparison](../../../../audits/code-simplification/reviews/backend-json-findings.json),
  BJ-005. No benchmark, platform execution, or advisory clearance is claimed.
