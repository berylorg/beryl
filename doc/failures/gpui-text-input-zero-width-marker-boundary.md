# Scope

Phase 129 integration of canonical Syndic draft image markers with the range-backed
`gpui-text-input` widget pinned at revision `975e3d44ee16ba457f31c8364bf5b937791f567b`.

# Invalidated Approach

Treat source-zero-width marker editing as a boundary that can be added entirely within
`gpui-text-input`, then require the replacement owned GPUI layout session itself to prove global
uniqueness of arbitrary opaque object identities while retaining only constant state.

# Evidence

`gpui-text-input/src/range_source/page_impl.rs` rejects an empty `AtomFact::global_range`, and
`src/range_edit/staging.rs` rejects empty inserted and removed atom ranges. Syndic exposes each
canonical image marker at an absolute zero-width logical UTF-8 offset through
`SyndicContentTextSegmentBoundary::logical_offset`.

The widget emits only `RangeTextInputEvent::InlineAtomClicked(AtomId)` and renders every atom through
one editor-wide oversize presentation. Its public boundary exposes neither owner-supplied per-atom
presentation state nor the realized anchor geometry required by Beryl's
`image marker` widget contract. It also exposes no host-initiated ordinary marker-insertion entry
point through the exact range-backed mutation lifecycle.

The pinned owned GPUI revision `b83f38e38839ab1b917febfbbacfbed900e57e09` also rejects the
required canonical layout representation. `StreamingLayoutSession::admit_oversize_atom` rejects an
empty logical range, while `validate_order` permits one only for a logical-line end.
`StreamingLayoutHit` and `StreamingLayoutMap` expose only `u64` logical offsets, so geometry and hit
results cannot distinguish multiple objects or adjacent gaps sharing one source-byte anchor.

The first owned GPUI target draft then required object identities to be unique across an unbounded
stream, made duplicate identity invalid, required the session to validate identity, and prohibited
retaining the preceding object collection. No fixed-size exact validator can distinguish every
possible prior set of opaque identities, so it cannot decide whether a later identity is globally
new without an unbounded set or a stronger source proof.

A later interaction review found that restoration export reused cached scroll witnesses and
selected fallback proofs by byte offset alone. Multiple objects at one anchor make the adjacent gap
part of the exact position, so byte-only matching can restore the wrong composite anchor.

# Why It Failed

Fabricating visible label bytes to give a zero-width marker a nonempty atom range would change the
canonical logical coordinate domain and create the kind of compatibility projection forbidden by
the Beryl-home cutover. Rendering or hit-testing a separate Beryl overlay would split atom geometry,
selection, focus, and mutation ownership away from the canonical range-backed editor.

Adding the correct object paging and gap-witness protocol only in `gpui-text-input` still cannot
enter those objects into the mandated canonical GPUI layout stream. Mapping them to nonempty ranges
or painting them outside that stream would reintroduce the same forbidden synthetic coordinate or
split-geometry workaround one dependency layer lower.

Requiring exact global uniqueness inside the constant-state layout session is also the wrong
ownership boundary. A probabilistic filter would violate exactness, while retaining every prior
identity would violate boundedness.

# Course Correction

First add and independently accept an owned GPUI streaming-layout boundary for ordered zero-width
opaque objects and object/gap-specific composite geometry and hit mappings. Then add the app-neutral
`gpui-text-input` boundary with separately paged revision-bound objects, stable same-anchor order,
constant-size adjacent-object gap witnesses, bounded presentation, exact realized
activation geometry, and ordinary staged object mutations. Publish and pin the dependency boundary
before Beryl integration; do not add a compatibility adapter, fabricated source bytes, or custom
overlay at any layer.

Global identity uniqueness belongs to the consumer's revision-bound object-source authority. GPUI
validates only facts available at its bounded boundary: current composite-position continuity,
adjacent gap witnesses, agreement between an admitted object and its two edges, and strict
same-anchor order. It does not retain or claim to validate the complete prior identity set.

Restoration validation and export must compare the complete `SourcePosition`, including its exact
same-anchor gap witness; a byte-only proof is not an admissible substitute.

# Affected Work

The Operator approved the identity-authority correction. Root `doc/plan.md` Phase 126 completed the
corrected GPUI boundary, and Phase 127 independently accepted exact composite positions, bounded
object paging and presentation, crate-owned scalar proofs, separate residency, and the ordinary
no-object GPUI cutover in `gpui-text-input`. Phase 128 independently accepted exact staged object
mutations and successor adoption, and Phase 129 accepted bounded exact composite clipboard and
payload-free compact restoration. Phase 130 accepted canonical bounded composite realization,
exact object and adjacent-gap geometry, compact checkpoint and target continuation, and terminal
keyed lifecycle handling. Phase 131 accepted cross-owner staged publication, Phase 132 corrects
owned-GPUI composite boundaries, Phase 133 finishes interaction and lifecycle behavior, Phase 134
owns complete sibling acceptance, and Beryl Phase 136 remains
gated on publication and canonical pins.
