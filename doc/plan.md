# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. Protect concrete supported-envelope consequences without
duplicating dependency guarantees, runtime validation, or review machinery.

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

# Phase 221: Finalize Exact Committed Local State After Health Closure (finished)

Added the HomeStore-owned move-only capability for finalizing only already-durable, single-domain
attachment-local custody after a later structural failure closes health. Exact receipt, store
generation, domain, revision, handle, and attachment identity remain bound without Fjall, health,
reconciliation, retry, acknowledgement, or publication authority. A supported counter-divergence
regression protects generation identity. Focused finalization tests passed 7/7; HomeStore, Syndic,
and Beryl-state all-target checks and the affected Beryl-app library check passed. Independent
semantic review found no remaining issue and required no adversarial review.

# Phase 222: Own Draft-Marker Receipt Submission And Exact Outcome Reconciliation (pending)

Complete the preserved opaque Syndic page-submission implementation against the accepted
HomeStore local-finalization boundary. Consume exact proof and command outcomes linearly, install
indeterminate custody before local release, classify only through durable exact reconciliation,
and preserve bounded attachment ownership, lost-acknowledgement replay, and final-EOF deferral.

# Phase 223: Assign Draft-Marker Labels And Issue Readiness (pending)

Reserve the package-derived allocation range after exact EOF, consume the source-order tree through
bounded durable continuation, assign the target-id tree, and issue the final move-only proof only
from canonical-empty source authority and exact zero-unassigned closure.

# Phase 224: Close And Reclaim Draft-Marker Admission (pending)

Implement cancellation before and after durable admission, inert terminal closure, incremental
cross-restart cleanup, exact replay/collision retention, and final aggregate resource release without
reactivating or resuming prior-generation operations.

# Phase 225: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 226: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
