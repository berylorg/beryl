# Reason For Investigation

On 2026-09-06, the complete simplification audit examined whether im 15.1.0 could replace the
authenticated draft-marker admission indexes in `syndic-storage/src/draft_piece/admission`.
Finding SA-009 contains the call-site evidence and preserves this negative screen for reuse.

# Outcome

Rejected for this boundary. OrdMap provides an immutable in-memory B-tree and ordered lookup,
update, removal and structural sharing. Our inference from that API and Beryl's source is that
adapters would still need deterministic durable node identities, authenticated descriptors,
bounded path reads/writes, canonical codecs, exact retained replay closure and command charges.
Loading an entire operation into a map would also lose the current bounded traversal. No net
reduction has been demonstrated; local predicate and successor-assembly reuse remains preferable.

The exact package manifest declares Rust 1.46, edition 2018 and MPL-2.0+; normal dependencies include
bitmaps, rand_core, rand_xoshiro, sized-chunks and typenum. Optional features include serde, rayon,
refpool and testing. No package or feature changes are proposed. The upstream repository reports
that it was archived on 2026-05-03. Its established history and related im-rc usage in Cargo do not
resolve the storage mismatch. The workspace's Rust 1.92 exceeds the declared MSRV, but Windows,
runtime behavior and the dependency closure were not tested. No advisory clearance is claimed;
screening stopped at architecture incompatibility. No installation, build or benchmark occurred.

# Sources

- [Exact OrdMap API](https://docs.rs/im/15.1.0/im/struct.OrdMap.html).
- [Exact published manifest](https://docs.rs/crate/im/15.1.0/source/Cargo.toml).
- [Upstream repository and archive state](https://github.com/bodil/im-rs).
- [Cargo manifest, related-family adoption only](https://github.com/rust-lang/cargo/blob/master/Cargo.toml).
- [Admission review evidence](../../../../audits/code-simplification/reviews/syndic-admission-findings.json).
