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

# Phase 212: Integrate Persisted-Aware Domain Attachment Construction (finished)

Integrated one typed bounded domain-local registration reader and fully initialized attachment
construction before initial or recovery publication, preserving exact retirement, stale fences,
package-owned read-failure severity, and retryable caller-bound failures without a second lifecycle
stage. Focused single-job checks and lifecycle tests passed across HomeStore, Beryl state, and Syndic;
independent semantic review closed after actual structural and bounded-read failure evidence passed.

# Phase 213: Establish The Durable Draft-Marker Admission Substrate (pending)

Implement the admission capacity singleton, operation head, authenticated source-order and target-id
trees, replay-receipt schema, exact retained-resource accounting, and bounded home-generation runtime
attachment and registration reconstruction. Verify canonical encoding, tree and charge invariants,
whole-home cross-restart limits, empty and populated registration, and corruption refusal with
focused tests and independent semantic review.

# Phase 214: Admit Arbitrary-Order Draft-Marker Evidence (pending)

Compose the fixed HomeStore proof and private Asset witness, ingest one association per bounded
durable quantum into both operation trees, reject duplicate targets across pages, and provide exact
byte replay/reconciliation while reclaiming superseded predecessor paths. Do not depend on process-
object continuity, a source-order digest, repeated identical witness reads, or caller binding
injection.

# Phase 215: Assign Draft-Marker Labels And Issue Readiness (pending)

Reserve the package-derived allocation range after exact EOF, consume the source-order tree through
bounded durable continuation, assign the target-id tree, and issue the final move-only proof only
from canonical-empty source authority and exact zero-unassigned closure.

# Phase 216: Close And Reclaim Draft-Marker Admission (pending)

Implement cancellation before and after durable admission, inert terminal closure, incremental
cross-restart cleanup, exact replay/collision retention, and final aggregate resource release without
reactivating or resuming prior-generation operations.

# Phase 217: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 218: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
