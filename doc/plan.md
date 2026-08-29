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

# Phase 197: Simplify HomeStore Proof And Read Publication (finished)

Read-only proof composition no longer waits for the writer, and ordinary read, proof, and receipt
publication use exact generation and affected-gate confirmation without a global post-read health
canary. Focused single-job nextest coverage passed for nonserialization, admission-edge cancellation,
coherent old-or-new atomic snapshots, unrelated maintenance, and actual structural failures; scoped
format/diff checks passed and fresh independent semantic review closed every applicable finding.

# Phase 198: Collapse Duplicate Mutation Preparation (pending)

Replace mandatory same-snapshot `validate` plus `contribute` rereads with one bounded preparation
boundary that returns validated package-owned state for atomic contribution. Preserve exact family
reservations, validation-only cross-domain participants, CAS checks, batch refusal before commit,
and acknowledgement-loss reconciliation. Convert packages in bounded ownership batches and verify
representative state, collision, and failure paths without a second semantic pass.

# Phase 199: Establish Monotonic Draft-Label Protection (pending)

Add the independently revisioned thread-keyed draft-label protection head before rebuilding
readiness. Initialize it from accepted authority, advance it only with committed draft allocation,
reserve above it and live transient reservations, and keep permanent accepted authority and origin
spans distinct. Prove monotonicity, affected-record corruption, restart, acceptance, and
reconciliation with focused tests and independent semantic review.

# Phase 200: Replace Draft-Marker Readiness With Lean Admission Indexes (pending)

Delete the superseded source-order stream digest and current invalid WIP. Implement operation-owned
source-order staging and target-id admission indexes with arbitrary page arrival, bounded post-EOF
assignment, exact target consumption, incremental reclamation of superseded paths, enforced
per-operation and home-wide cross-restart retained-resource limits, and only the minimum durable
head/receipt needed for replay and reconciliation. Preserve the fixed HomeStore proof protocol and
cross-domain Asset witness while removing same-object replay, repeated identical witness reads, and
caller binding injection.

# Phase 201: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 202: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
