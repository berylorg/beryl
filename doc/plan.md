# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. First reconcile live authority and the execution window with the
updated engineering-rigor contract so later implementation protects concrete supported-envelope
consequences without duplicating dependency guarantees, runtime validation, or review machinery.

Preserve the architecture that has real product cost and purpose: bounded GUI/editor/transcript
streaming, explicit Beryl-owned resource limits, atomic durable mutation, exact acknowledgement-loss
reconciliation, Syndic/CAS identity and external-effect fencing, cross-domain asset proof, and
bounded recovery. Do not infer whole-store validation, continuous semantic proof, blanket
adversarial review, arbitrary-scale support, or duplicated consumer checks from persistence or an
unqualified verification requirement.

Keep only this bounded cleanup and draft-marker slice in the durable plan. Later GUI, repair,
recovery, branch, asset, integration, and closure boundaries remain represented by the active
rework tracker until their own bounded slices are ready. Functional checks run normally; sustained
stress or performance work requires a concrete unresolved question and prior Operator AC-power
coordination.

# Phase 208: Complete Prepared-Mutation Integration (finished)

Converted all 21 `beryl-home-store` integration and support mutations to consuming one-time
preparation and reader-free contribution, preserved the intentional validation-only boundary, and
corrected stale non-`Copy` handle and one-shot fault/reopen fixtures without production or schema
changes. Structural review found no executable old mutation surface or needless defensive layer.

All-test/all-feature compilation passed; the exact Phase 208 package gate passed 204 of 204 tests,
`beryl-state` representatives passed 16 of 16, and unchanged `syndic-storage` retained its accepted
698-of-698 result. Two unrelated durable-start footprint assertions were excluded exactly and are
owned by Phase 209. Independent semantic completion review closed without blocking findings.

# Phase 209: Reconcile Durable-Start Footprint Authority (pending)

Recompute the package-owned direct and queued durable-start record, encoded-byte, and Fjall journal
envelopes from the accepted Syndic and Asset participant footprints; reconcile the Beryl-home
storage system, `beryl-home-store` package authority, and exact footprint fixtures without changing
source behavior or persisted schema. Verify both participant pairs, the shared admission maximum,
and checked overflow, then obtain independent authority review.

# Phase 210: Reconcile Draft-Marker Admission Authority (pending)

Reconcile the image-assets system's removed coordinator, cumulative-digest, and immediate-replay
wording with the controlling Syndic arbitrary-order durable-index, post-EOF assignment, byte-exact
replay, and package-derived binding contracts before replacement implementation begins. Obtain
independent authority review and change no source or persisted schema.

# Phase 211: Establish Monotonic Draft-Label Protection (pending)

Add the independently revisioned thread-keyed draft-label protection head before rebuilding
readiness. Initialize it from accepted authority, advance it only with committed draft allocation,
reserve above it and live transient reservations, and keep permanent accepted authority and origin
spans distinct. Prove monotonicity, affected-record corruption, restart, acceptance, and
reconciliation with focused tests and independent semantic review.

# Phase 212: Replace Draft-Marker Readiness With Lean Admission Indexes (pending)

Implement operation-owned source-order staging and target-id admission indexes with arbitrary page
arrival, bounded post-EOF assignment, exact target consumption, incremental reclamation of
superseded paths, enforced
per-operation and home-wide cross-restart retained-resource limits, and only the minimum durable
head/receipt needed for replay and reconciliation. Preserve the fixed HomeStore proof protocol and
cross-domain Asset witness without reintroducing the removed source-order digest, same-object replay,
repeated identical witness reads, or caller binding injection.

# Phase 213: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 214: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
