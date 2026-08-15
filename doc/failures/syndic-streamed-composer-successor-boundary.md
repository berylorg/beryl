# Scope

Phase 129 incremental autosave, undo, and submission over a revision-bound Syndic current draft.

# Invalidated Approach

Reuse the existing public Syndic draft-save API while replacing only Beryl's resident editor model
with range-backed pages.

# Evidence

`PreparedContent::composer` accepts one complete `ComposerPayload` and builds resident vectors of
all canonical chunks, text spans, and content pieces. `ContentAppend::prepare` accepts that complete
`PreparedContent` and copies the next bounded chunk batch from it. Although the internal ComposerV1
fold accepts bounded text fragments and emits bounded records, its atom-writer and record-sink
traits are reachable publicly only through test support, not through the production crate API.

`DraftPayloadUpdate::prepare_reference` can publish an already sealed successor reference, but no
public production boundary constructs that sealed ComposerV1 successor by streaming the prior
revision plus exact range edits.

# Why It Failed

A range-backed editor would still reconstruct and retain the complete draft and marker collection
before every autosave or submission. Bounded final chunk appends do not repair an upstream
whole-value allocation, so retaining this API would violate the cross-system bounded-dataflow
invariant and Phase 129's removal-only cutover.

# Course Correction

Design and independently accept a production `syndic-storage` boundary that streams an exact prior
ComposerV1 revision plus ordered text and marker edits into unreachable bounded staging, verifies
the canonical successor identity and summary, seals it, and returns the exact reference consumed by
`DraftPayloadUpdate::prepare_reference`. It must preserve revision conflict, cancellation,
supersession, crash, and orphan-staging behavior without exposing a partial draft.

# Affected Work

Root `doc/plan.md` now establishes the streamed successor-construction boundary in Phase 128. Beryl
Phase 129 remains gated on its independent acceptance and availability to `beryl-app`.
