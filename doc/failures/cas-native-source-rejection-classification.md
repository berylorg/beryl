# CAS Native Source Rejection Classification

Phase 10 initially assumed that a failed CAS `thread/resume` or `thread/fork` request could tell the
projection coordinator whether the durable native source had become unavailable. The first
coordinator implementation handled that ambiguity inconsistently: resume retired the source after
every backend error, while fork propagated every backend error and retained the source.

That assumption is invalid against the pinned `codex-cli 0.144.1` contract. CAS flattens both
lineage-invalidating and source-preserving failures into ordinary JSON-RPC errors. Missing rollout,
archived source, closing thread, stale rollout path, contradictory request options, configuration
failure, and other request failures may share code `-32600` with no structured discriminator.
Internal failures may likewise occur after the operation has progressed without proving that the
source rollout is absent. Beryl receives only code, message, and optional untyped data, so neither
the method nor the error code establishes a lineage verdict.

Matching human-readable error text, retiring the source after every rejection, or treating a retry
count as proof that the source is dead would each be a workaround rather than an exact authority
boundary. The Operator selected the target behavior: authoritative lineage loss retires the source
and creates one fresh recovered projection from Syndic history; a source-preserving or unclassified
rejection retains the native binding and receives bounded automatic retry.

CAS 0.144.1 still cannot classify an ordinary rejection for Beryl. Generic errors therefore remain
unclassified and preserve the binding. The Operator accepted an explicit abandon-and-recover
workflow after bounded automatic retry: retry keeps the exact source; `Recover from Syndic history`
revision-checks it and establishes one fresh target projection. A target-owned source is retired;
another thread's fork source is preserved. This supplies the missing authority without message
matching, retry-count inference, CAS modification, or automatic source loss.

Affected implementation is `crates/beryl-app/src/cas_projection/execute/native.rs` and the
normalized backend error boundary in `crates/beryl-backend`. Controlling authority is Phase 10 of
`doc/plan.md` and the native-lineage recovery contract in
`doc/systems/cas-live-syndic-transcript/design.md`.
