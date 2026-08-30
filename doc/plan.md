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

# Phase 215: Establish Fixed Draft-Marker Evidence Proof Composition (finished)

Published HomeStore's zero-state fixed-digest protocol marker and completed the private accepted-
only Asset witness with one-time page validation, coherent exact-authority revalidation, and unique-
tuple read coalescing without occurrence loss. Locked package checks, 210 HomeStore tests, 113
Beryl-state tests, focused boundary cases, and independent semantic review all passed.

# Phase 216: Admit Arbitrary-Order Draft-Marker Evidence (pending)

Prepare Syndic source-only and accepted source-plus-witness page attempts, consume only the exact
paired HomeStore proof receipt, and ingest at most one association per durable quantum into both
authenticated operation trees. Reject duplicate targets within or across pages, enforce page, per-
command, per-operation, and home-wide retained-resource bounds, and provide byte-exact replay and
acknowledgement-loss reconciliation while atomically reclaiming superseded replay-only paths and
retaining only the immediate predecessor closure. Do not depend on process-object continuity, a
source-order digest, repeated identical witness reads, or caller binding injection.

# Phase 217: Assign Draft-Marker Labels And Issue Readiness (pending)

Reserve the package-derived allocation range after exact EOF, consume the source-order tree through
bounded durable continuation, assign the target-id tree, and issue the final move-only proof only
from canonical-empty source authority and exact zero-unassigned closure.

# Phase 218: Close And Reclaim Draft-Marker Admission (pending)

Implement cancellation before and after durable admission, inert terminal closure, incremental
cross-restart cleanup, exact replay/collision retention, and final aggregate resource release without
reactivating or resuming prior-generation operations.

# Phase 219: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 220: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
