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

# Phase 195: Establish Generation-Owned Typed Runtime Attachments (finished)

HomeStore now owns one typed runtime attachment per registered live-generation slot with exact
retirement, stale-capability rejection, and failed-candidate cleanup. Locked checks, focused
lifecycle/state tests, formatting, diff checks, and independent semantic review passed.

# Phase 196: Reconcile Engineering Rigor And Simplify Live Authority (finished)

All 33 live design authorities have one valid rigor declaration. Live authority now credits Fjall's
coherent snapshots and atomic durability, removes writer serialization from read-only proof and the
global post-read health canary, and defines one-pass mutation preparation. Draft-marker authority is
protection-first and uses arbitrary-order staging plus bounded post-EOF assignment, two current
authenticated indexes, package-owned custody, incremental reclamation, and one home-wide cross-
restart ceiling of 64 heads, 65,536 associations, and 67,108,864 encoded bytes. The plan and tracker
were compacted, the invalidated defensive-proof assumption was preserved, scoped checks passed, and
fresh independent semantic review closed every applicable finding.

# Phase 197: Simplify HomeStore Proof And Read Publication (wip)

Remove global writer serialization from read-only proof composition and narrow ordinary read
publication to affected-slice health and generation guarantees. Preserve coherent Fjall snapshots,
revision fencing, proof correlation, cancellation, and actual storage-access failure propagation.
Prove concurrent proof/write outcomes and coherent reads across unrelated mutation-health changes
with focused tests and independent semantic review.

Resumable milestone: the implementation and four focused single-job nextest targets pass. Exact
phase diffs are clean; owned files are formatted. Independent semantic completion review remains
before phase acceptance and compaction. Workspace-wide formatting is currently obstructed only by
pre-existing unowned import ordering in `src/proof/command.rs`.

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
