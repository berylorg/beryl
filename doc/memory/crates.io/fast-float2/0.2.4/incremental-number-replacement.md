# Reason For Investigation

Determine whether an established decimal-to-float implementation could remove Beryl's custom
bounded accumulator for fragmented JSON numbers during the complete simplification audit.

# Outcome

fast-float2 0.2.4 exposes parse and parse_partial over contiguous byte input through AsRef. It
does not expose resumable mantissa/exponent state for a fragmented, arbitrarily long spelling.
Beryl's accumulator retains a bounded significant-digit window, sticky nonzero information, and
exponent/sign state. Using fast-float2 for final conversion would retain that accumulator and its
normalization responsibilities. Removing the accumulator would instead require proportional input
storage or another custom incremental converter.

No source reduction was demonstrated, so no replacement is proposed. A performance comparison is
a different question. The crate has MIT/Apache-2.0 licensing, a declared Rust 1.37 minimum, default
std and optional no_std use, and an established decimal-conversion algorithm lineage; those facts
do not fill the incremental API gap.

# Sources

- Registry authority: crates.io, fast-float2 0.2.4, checked 2026-09-06. It is not selected in the
  Beryl baseline lockfile and was not installed, added, or built.
- [Registry metadata](https://crates.io/api/v1/crates/fast-float2), exact release, features,
  license, MSRV, and release history.
- [Exact v0.2.4 API source](https://raw.githubusercontent.com/Alexhuszagh/fast-float-rust/v0.2.4/src/lib.rs),
  parse and parse_partial entry points and contiguous-input bounds.
- [0.2.3 generated API documentation](https://docs.rs/fast-float2/0.2.3/fast_float2/) was supporting
  background only; the actual candidate's API was checked against the exact v0.2.4 source above.
- Beryl baseline `e6172f7c49bebef78d51831e621e70c8ac0d6a07`:
  `crates/beryl-backend/src/incoming_json/provider/machine/number.rs`, including the complete
  NumberAccumulator and decimal normalization bodies.
- [Complete local evidence](../../../../audits/code-simplification/reviews/backend-json-findings.json),
  BJ-005. No benchmark or advisory clearance is claimed.
