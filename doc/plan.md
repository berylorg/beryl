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
