# Reason For Investigation

The complete simplification audit evaluated whether a mature registry JSON deserializer could
replace substantial custom response and streamed-provider parsing in beryl-backend.

# Outcome

serde_json 1.0.151 is a mature, portable Rust candidate with MIT/Apache-2.0 licensing and Rust 1.71
MSRV. It is an investigated candidate version, not Beryl's resolved version: the audit baseline
resolves 1.0.149. No package or lockfile change was made.

The inspected deserializer owns a scratch Vec and decodes complete retained strings before calling
the visitor. A bounded result type therefore does not by itself bound intermediate escaped-string
or member-name residency. Custom visitors would still need Beryl's order, duplicate, identity,
and retention handling; raw values or a DOM would retain the wrong representation. Optional
float_roundtrip paths also retain long number spellings. Existing outbound serializer use is outside
this replacement question.

No compatible net-reducing ingress replacement was established under current contracts. A private
pull cursor over the existing bounded-json event API is a separate architectural candidate that
preserves incremental recognition. Reconsider serde_json only if an owning design deliberately
changes the relevant input/residency envelope and measures the remaining adapters.

BR-010 separately considered typed deserialization for ancillary initialize/config/model-list
responses under an explicitly changed input policy. Bounded retained labels and record counts do
not bound discarded input metadata. A cap would reject formerly valid large descriptions/settings;
an example 1 MiB budget is only a design/prototype question. Normal Serde field-order and duplicate
handling also differs from the current grammar. Since classification already consumes a prefix,
the route needs an admitted prefix replay/handoff as well as allocation, depth, full-consumption
and connection-poison rules. It cannot restart a whole-object deserializer on the remaining reader.
This alternative overlaps BJ-003's bounded-json pull proposal and carries no accepted saving.
Canonical provider content, streamed user input, recovery history and repeated terminal items are
excluded from any such cap proposal. Full context is retained in
[BR-010](../../../../audits/code-simplification/reviews/backend-rest-findings.json).

# Sources

- Registry authority: crates.io, serde_json 1.0.151; inspected 2026-09-06. The normal candidate
  uses default `std`; optional alloc, arbitrary_precision, float_roundtrip, preserve_order,
  raw_value, and unbounded_depth were identified rather than silently enabled.
- [Registry metadata](https://crates.io/api/v1/crates/serde_json), including version, license,
  release history, and feature information.
- [Versioned package](https://docs.rs/crate/serde_json/1.0.151).
- [Exact tagged deserializer](https://raw.githubusercontent.com/serde-rs/json/v1.0.151/src/de.rs),
  scratch ownership, deserialize_str before visitor dispatch, and long-number paths.
- Beryl baseline `e6172f7c49bebef78d51831e621e70c8ac0d6a07`: Cargo.lock selects serde_json 1.0.149;
  backend manifest, `incoming_json/provider.rs`, response machines, and number accumulator.
- [Complete local evidence](../../../../audits/code-simplification/reviews/backend-json-findings.json),
  BJ-003 and BJ-005. Existing 1.0.149 memory was consulted separately. This was source/API
  investigation; no installation, local build, benchmark, or advisory clearance is claimed.
