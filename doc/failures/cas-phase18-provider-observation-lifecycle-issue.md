# Provider Observation Lifecycle Conflicts Need Durable Source Authority

## Context

Phase 18 establishes the storage authority needed by the ordered broker's sealed,
constant-resident observation consumer before Phase 19 publishes through it. The consumer must
preserve exact wire order while canonical item lifecycle remains storage-owned.

## Invalidated Shape

Treating an exact-route duplicate start, repeated or reversed completion, or item-kind conflict as
only a caller-local `incomplete_reason` is invalid. The old materialized `LiveCapture` could retain
that flag until terminal publication, but the ordered broker cutover deliberately removes its
terminal and source-frontier authority.

No legal `ItemFrame` can represent these observations without overwriting or advancing a canonical
item contrary to its durable lifecycle. Abandoning the sealed observation would also be false: the
route and normalized provider evidence are valid, and loss or restart must converge from the same
history-incomplete fact.

A router-only flag, target-close outcome, or deferred receipt would be a second source authority.
It would lose the observation across process loss or recreate the caller/broker ordering split that
the cutover removes.

The focused ordinary-turn repeated-completion case exposed the defect: the canonical first item
needed to remain unchanged while terminal history became `CompletionMismatch`. The compiler's
nonempty-start byte-parity tests and locked storage check passed, so this is not an observation
normalization or bounded-replay failure.

The first issue matrix then exposed a second authority error: structural sealing rejected every
started `SubAgentActivity` observation even though that is the only completion-only kind. That made
the closed `CompletionOnlyItemStarted` reason unreachable before inspection and confused evidence
validity with normal-frame lifecycle admissibility. The backend ingress parser contains the same
premature lifecycle rejection, so the ordered publication cutover must remove it before the broker
can deliver this issue path end to end.

## Course Correction

Syndic owns a bounded provider-observation issue source event. Its private-constructible payload
references the immutable sealed observation, exact CAS item, digest-covered build frontier, and a
closed lifecycle-conflict reason. One atomic mutation proves the conflict, advances source order and
monotonic turn issue state, and leaves canonical item state unchanged.

Structural sealing therefore retains a completion-only start when its grammar and fields are valid.
Normal frame preparation still rejects that lifecycle, while issue classification is the sole path
that may publish `CompletionOnlyItemStarted`.

Normal provider terminal publication retains the provider outcome and selects
`CompletionMismatch`. Source-less convergence retains its primary loss reason while preserving the
separate issue fact. Missing, malformed, mismatched, cancelled, or retired routing still publishes
no issue event.

## Verification

Verification must cover every closed lifecycle conflict, rejection of a legally admissible
observation as an issue, exact retry and source-sequence collision behavior, codec/reopen replay,
sealed-build corruption, normal terminal reason selection, loss retention, and fixed resident
memory for arbitrarily large referenced observations.
