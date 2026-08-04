# Reason For Investigation

Phase 13 requires strict JSON decoding whose live memory is independent of message, object-key,
string, and number length. The decoder must accept arbitrary input fragmentation, emit decoded
name and string fragments, preserve exact structural order, and stop for downstream capacity
without retaining or consuming unaccepted content.

# Outcome

No inspected Rust JSON parser supplies that complete production contract.

`serde_json` 1.0.149 decodes escaped strings and object keys into complete scratch storage before
the visitor receives them. Nom 8.0.0 reports incomplete input and expects retry with more retained
input; it does not commit scalar fragments or suspend for output capacity. Struson 0.7.2 offers a
reader for large string values but returns object member names whole. Actson 2.1.0 accumulates the
current string, field name, or number in a growing byte vector. `bufjson` 1.1.0 is the closest: its
low-level lexical machine resumes across chunks, but its ordinary analyzers and parser retain all
buffers for a complete token, and escaped-string expansion produces a complete allocation.

Using only `bufjson`'s lexical machine would still leave the caller responsible for incremental
unescaping, structural/schema progress, number handling, and output backpressure. Nom would leave
the same hard state while adding retry-oriented input semantics. The investigation therefore
supports a small independent fixed-residency parser project rather than a production parser
dependency. Existing `serde_json` remains useful only as a bounded-fixture differential oracle.

The local integration sites inspected were `crates/beryl-backend/src/incoming_json/provider/`,
`crates/beryl-backend/src/provider_observation.rs`, the root `Cargo.lock`, and workspace manifests.

# Sources

- Serde project, `serde_json` 1.0.149 source, `de.rs` and `read.rs`, resolved by the Beryl
  `Cargo.lock`; https://docs.serde.rs/src/serde_json/de.rs.html; accessed 2026-07-18.
- Rust Bakery, Nom 8.0.0 crate documentation, “Streaming / Complete” and `Parser` contract;
  https://docs.rs/nom/8.0.0/nom/; accessed 2026-07-18.
- Vincent Schapp, `bufjson` 1.1.0 crate, `ReadAnalyzer`, lexical state machine, and unescaping API;
  https://docs.rs/bufjson/1.1.0/bufjson/;
  https://docs.rs/bufjson/1.1.0/bufjson/lexical/read/struct.ReadAnalyzer.html;
  accessed 2026-07-18.
- Marcono1234, Struson 0.7.2 `JsonReader` API;
  https://docs.rs/struson/0.7.2/struson/reader/trait.JsonReader.html; accessed 2026-07-18.
- Michel Krämer, Actson 2.1.0 parser source;
  https://docs.rs/actson/2.1.0/src/actson/parser.rs.html; accessed 2026-07-18.
- AWS SDK for Rust, locally cached `aws-smithy-json` 0.61.6 source, `deserialize` token iterator;
  not resolved by the Beryl workspace and inspected only as a candidate on 2026-07-18.
