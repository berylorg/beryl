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

# Phase 203: Convert Session Mutation Preparation (finished)

All nine session production and raw-schema mutations now prepare package-owned state once and
contribute reader-free while preserving exact header, window, claim, restore, revision, reservation,
reconciliation, and typed failure behavior. `ThreadClaimCatalogSource` remains validation-only.
Edition-2024 format, scoped diff, visibility, and structural checks passed, and independent semantic
review closed without findings. The intentional package compile gap remains through Phase 204.

# Phase 204: Convert Asset Mutation Preparation (pending)

Convert Asset production and fault-support mutations to package-owned prepared state without
touching the invalid draft-marker WIP in `asset.rs` or `lib.rs`. Keep `ValidateAssetOwnerHeads`
validation-only and preserve exact sealed-proof, owner-head, successor-witness, reservation, and
reconciliation behavior.

# Phase 205: Accept The Beryl-State Prepared-Mutation Boundary (pending)

Remove every remaining Beryl-state use of the old callback shape, restore all-target and all-feature
compilation, run focused and package-wide cargo-nextest coverage, confirm direct failure provenance
and one-pass operation-bounded reads, and obtain independent semantic completion review.

# Phase 206: Convert Syndic Mutation Preparation (pending)

Convert all `syndic-storage` production and test-fault mutation implementations to package-owned
prepared state while preserving its validation-only participant, CAS, route, binding, record, and
reconciliation checks. Reconcile the overlapping draft-marker WIP against the target design without
committing superseded marker architecture, and verify the package boundary without a compatibility
adapter.

# Phase 207: Complete Prepared-Mutation Integration (pending)

Convert `beryl-home-store` integration fixtures and support mutations, then verify representative
single-domain, cross-domain, validation-only, collision, capacity-refusal, persisted-failure, and
acknowledgement-loss reconciliation paths across all converted packages. Remove every live use of
the old two-pass mutation surface and obtain independent semantic completion review.

# Phase 208: Establish Monotonic Draft-Label Protection (pending)

Add the independently revisioned thread-keyed draft-label protection head before rebuilding
readiness. Initialize it from accepted authority, advance it only with committed draft allocation,
reserve above it and live transient reservations, and keep permanent accepted authority and origin
spans distinct. Prove monotonicity, affected-record corruption, restart, acceptance, and
reconciliation with focused tests and independent semantic review.

# Phase 209: Replace Draft-Marker Readiness With Lean Admission Indexes (pending)

Delete the superseded source-order stream digest and current invalid WIP. Implement operation-owned
source-order staging and target-id admission indexes with arbitrary page arrival, bounded post-EOF
assignment, exact target consumption, incremental reclamation of superseded paths, enforced
per-operation and home-wide cross-restart retained-resource limits, and only the minimum durable
head/receipt needed for replay and reconciliation. Preserve the fixed HomeStore proof protocol and
cross-domain Asset witness while removing same-object replay, repeated identical witness reads, and
caller binding injection.

# Phase 210: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 211: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
