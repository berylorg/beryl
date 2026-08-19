# Scope

Range-backed composer activation across `beryl-app`, `gpui-text-input`, and `syndic-storage`.

# Invalidated Approach

Implement the app host and widget binding first, treating existing Syndic draft reads as an
adequate range source and filling any mismatches at the app boundary.

# Decisive Evidence

`gpui-text-input` range binding requires an exact logical byte-and-line extent, directional and
validation text demands with source-selected UTF-8-safe edges, bounded bidirectional marker pages,
and exact gap translation. The Syndic combined-root authority committed logical bytes but not the
required logical-line aggregate, and its read boundary did not completely specify the required
session-qualified activation identity, edge proofs, paging ceilings, or first/last/adjacent marker
proofs.

# Why It Failed

The app could supply the missing facts only by whole-draft or whole-anchor scans, inferred line
counts, detached caching, or compatibility adapters. Those paths would not be authenticated by the
selected root, would violate fixed-residency behavior, and would let the app-host-first phase depend
on assumptions owned by later edit, autosave, materialization, or submission work.

# Accepted Correction

Make Syndic range-source conformance a prerequisite boundary. Combined tree summaries and canonical
digests commit the exact logical-line semantics; storage exposes exact-root, current-root, and
session-candidate reads with bounded directional text, marker, validation, and gap-proof operations;
fresh activation opens and binds one exact session head from the stabilized durable selector. The
app host then consumes that boundary directly without adapters. Editing and autosave remain later
work, and `ComposerV1` materialization and submission remain separate later work.

# Affected Work And Remaining Risk

The target correction is owned by `doc/systems/syndic-conversation-history/design.md` and
`crates/syndic-storage/doc/design.md`; active Phase 140 must implement it before Phase 141 app-host
activation, Phase 142 editing and autosave, and Phase 143 materialization and submission. Source,
tests, and rework completion state must not claim conformance until the summaries, encodings, APIs,
bounded proofs, and failure behavior are implemented and verified together.
