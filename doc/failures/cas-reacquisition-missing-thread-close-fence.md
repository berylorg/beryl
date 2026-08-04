# Scope

Same-process, same-native CAS thread reacquisition after a completion/live narrative mismatch.

# Invalidated Approach

The first implementation represented the old subscription only as a quarantined loaded-thread
registry entry. It removed the terminal live-event target before creating that anchor, resumed the
thread through an otherwise fresh replacement connection, and atomically transferred the registry
entry after an idle response.

That registry-only anchor was treated as proof that the remote CAS subscription and exact in-memory
thread remained alive throughout the overlap.

# Evidence

`thread/closed` is normalized as a thread-scoped event, but the event router currently invalidates a
loaded generation only when an exact live-event target exists. With no target, it increments the
unmatched-event counter and continues. Narrative-mismatch terminal convergence removes its target
before quarantine, and the replacement connection has no target while `thread/resume` is in flight.

Therefore a close notification on the old quarantined connection or on the replacement connection
can be discarded while the local registry still reports the anchor as live. The transfer may then
publish a new loaded generation and release the old anchor. For recovered lineage, CAS may have
cold-reconstructed the thread from rollout even though that path has no transactional proof that the
one-time injected prefix was durably appended.

Independent review also found that later recovered-binding retirement discards the exact retired
loaded generation and writes the immutable injection generation into stale provenance. After a
valid `G0 -> G1` handoff, that would record `G0` instead of the observed retired `G1`.

# Why It Failed

A turn target and a loaded-thread subscription are different lifetimes. Reusing target-scoped
invalidation for a period intentionally having no turn target left remote subscription loss outside
the authority model. Checking the registry before and after resume cannot repair this because the
lost close signal never changes the registry.

Likewise, injection-establishment provenance and current loaded-session provenance are distinct.
Using the former as a fallback for an observed retirement erases the exact later generation.

# Course Correction

Do not ship the registry-only quarantine transfer.

The clean handoff needs two explicit non-execution sides: the old quarantined anchor and a bounded
replacement resume reservation. Connection-scoped `thread/closed` handling must revoke the old side
or poison the replacement reservation even when no turn target exists. Transfer may publish a new
loaded generation only by atomically consuming both exact connection generations under retirement
serialization. A close that wins before transfer prevents transfer; a close that wins after transfer
invalidates the new loaded authority. Only closing the obsolete old generation after successful
transfer is harmless.

Retirement must return and persist the exact observed loaded generation. `RecoveredInjectionProof`
continues to retain `G0`; stale loaded-session provenance records `G1` when `G1` is what was retired,
and records no generation when no loaded entry was observed.

# Required Proof

Tests must inject old- and replacement-side closure before resume, during resume, immediately before
transfer, and after transfer. They must prove that no pre-transfer loss can yield a projection, that
post-transfer new-side loss revokes the projection, and that only post-transfer old-side cleanup is
non-authorizing. A reopen proof must retain injection generation `G0` and stale observed generation
`G1` as separate facts.

# Later Cutover Regression

Persistent-failure service-epoch reconnaissance found that the live implementation no longer
satisfies this correction. The selected backend classifier recognizes `thread/closed` only as a
discarded compact control, exposes no normalized ordered operation for it, and therefore gives the
app no connection-scoped close fact to apply when a turn target is absent. The app retains the
target-local close reason and the authoritative connection-scoped design text, but the required
ingress path is missing.

Service-epoch adoption cannot safely cross that gap: a close buffered before, during, or after the
epoch barrier could leave an apparently live quarantined lease. Restore normalized ordered
`thread/closed` ingress and stable connection-registry revocation as an explicit predecessor to
epoch adoption. Classifier recognition or target-local router closure alone is not completion.

# Phase 80 Restoration

Phase 80 restored `thread/closed` as one strict incremental backend operation containing only the
bounded CAS thread identity. The stable app forwarding sink now consumes it before replaceable
broker cancellation, records the exact router-lane fence, releases the router lock, and then enters
the observing connection's authority gate to invalidate active, quarantined, or reserved loaded
authority. Sink rejection preserves operation ownership and fails the connection closed on poison
or retired-lane overflow.

Focused regressions prove both reservation sides, the gate-held transfer race, old/new post-transfer
scope, target-present and target-absent closure, publication-owner delay, retirement, poison, both
persistent-failure cut orders, and installed quarantine. Phase 82 must make this stable interception
resolve the currently adopted service-epoch router endpoint; retaining the pre-adoption router
directly would reopen the same authority gap across epoch replacement.
