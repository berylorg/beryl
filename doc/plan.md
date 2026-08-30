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

# Phase 211: Establish Monotonic Draft-Label Protection (finished)

Established the independently revisioned thread-keyed protection head, atomic creation and exact
creation reconciliation, bounded reads, explicit scrub validation, and first-acceptance containment
without conflating permanent authority or origin spans. The strict advance primitive remains
package-internal for later committed-allocation integration.

Bounded Cargo check and focused nextest coverage passed for monotonicity, restart, corruption,
creation reconciliation, acceptance, and exhaustive family fixtures; independent semantic review
closed after the affected family matrices were updated.

# Phase 212: Establish The Durable Draft-Marker Admission Substrate (wip)

Implement the admission capacity singleton, operation head, authenticated source-order and target-id
trees, replay-receipt schema, exact retained-resource accounting, and bounded home-generation runtime
attachment and registration reconstruction. Verify canonical encoding, tree and charge invariants,
whole-home cross-restart limits, empty and populated registration, and corruption refusal with
focused tests and independent semantic review.

Implementation is blocked until target authority makes each evidence page either source-only or
accepted-only so every fixed proof role can derive the complete correlation, and gives HomeStore a
typed persisted-aware attachment factory that publishes no domain slot before bounded reconstruction
succeeds. The current mixed-page and parameterless-factory contracts cannot technically satisfy
their privacy and cross-restart guarantees; neither gap may be filled by caller facts or a second
post-registration initialization stage.

# Phase 213: Admit Arbitrary-Order Draft-Marker Evidence (pending)

Compose the fixed HomeStore proof and private Asset witness, ingest one association per bounded
durable quantum into both operation trees, reject duplicate targets across pages, and provide exact
byte replay/reconciliation while reclaiming superseded predecessor paths. Do not depend on process-
object continuity, a source-order digest, repeated identical witness reads, or caller binding
injection.

# Phase 214: Assign Draft-Marker Labels And Issue Readiness (pending)

Reserve the package-derived allocation range after exact EOF, consume the source-order tree through
bounded durable continuation, assign the target-id tree, and issue the final move-only proof only
from canonical-empty source authority and exact zero-unassigned closure.

# Phase 215: Close And Reclaim Draft-Marker Admission (pending)

Implement cancellation before and after durable admission, inert terminal closure, incremental
cross-restart cleanup, exact replay/collision retention, and final aggregate resource release without
reactivating or resuming prior-generation operations.

# Phase 216: Integrate Exact Ordinary Draft-Marker Writer Admission (pending)

Consume only package-issued readiness custody at mutation begin, point-consume exact target
associations during builder progress, and publish candidate, history, settlement, and protection
authority only from canonical empty admission closure. Verify substitution, cancellation,
acknowledgement uncertainty, collision, restart, and resource release with focused state-machine
tests and independent semantic review.

# Phase 217: Integrate Exact Historical Draft-Marker Adoption (pending)

Resolve the historical target entirely inside Syndic, authenticate its retained lineage, root,
marker commitment, selection, and protection containment, then adopt it without ordinary readiness,
reservation, Asset participation, marker scan, or history scan. Verify stale, missing, substituted,
collision, cancellation, and acknowledgement-uncertain outcomes with focused tests and independent
semantic review.
