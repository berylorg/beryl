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

# Phase 206: Remove Superseded Syndic Draft-Marker WIP (finished)

Removed the unaccepted Phase-196 Syndic readiness prototype without replacement architecture or a
compatibility adapter: 13 tracked source/test paths returned to their accepted contents and seven
untracked prototype paths were removed. Scoped cleanliness and live-symbol checks passed; Cargo was
inapplicable while the intentional mutation-trait cutover gap remains. Independent semantic review
closed without blocking findings.

# Phase 207: Convert Syndic Mutation Preparation (pending)

Convert all 80 `syndic-storage` production and test-fault mutation implementations to package-owned
prepared state while preserving its one validation-only participant, exhaustive domain validator,
reader-free reconciliation reservations, CAS, route, binding, record, and natural-record
reconciliation checks. Verify the complete package boundary without a compatibility adapter.

# Phase 208: Complete Prepared-Mutation Integration (pending)

Convert `beryl-home-store` integration fixtures and support mutations, then verify representative
single-domain, cross-domain, validation-only, collision, capacity-refusal, persisted-failure, and
acknowledgement-loss reconciliation paths across all converted packages. Remove every live use of
the old two-pass mutation surface and obtain independent semantic completion review.

# Phase 209: Reconcile Draft-Marker Admission Authority (pending)

Reconcile the image-assets system's removed coordinator, cumulative-digest, and immediate-replay
wording with the controlling Syndic arbitrary-order durable-index, post-EOF assignment, byte-exact
replay, and package-derived binding contracts before replacement implementation begins. Obtain
independent authority review and change no source or persisted schema.

# Phase 210: Establish Monotonic Draft-Label Protection (pending)

Add the independently revisioned thread-keyed draft-label protection head before rebuilding
readiness. Initialize it from accepted authority, advance it only with committed draft allocation,
reserve above it and live transient reservations, and keep permanent accepted authority and origin
spans distinct. Prove monotonicity, affected-record corruption, restart, acceptance, and
reconciliation with focused tests and independent semantic review.

# Phase 211: Replace Draft-Marker Readiness With Lean Admission Indexes (pending)

Implement operation-owned source-order staging and target-id admission indexes with arbitrary page
arrival, bounded post-EOF assignment, exact target consumption, incremental reclamation of
superseded paths, enforced
per-operation and home-wide cross-restart retained-resource limits, and only the minimum durable
head/receipt needed for replay and reconciliation. Preserve the fixed HomeStore proof protocol and
cross-domain Asset witness without reintroducing the removed source-order digest, same-object replay,
repeated identical witness reads, or caller binding injection.

# Phase 212: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 213: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
