# CAS Phase 17 Activation Reconciliation Across The Health Gate

## Scope

Checkpoint 3 Phase 17 publication of active CAS turn identity and `TurnActivated` through the
connection-owned ordered broker.

## Invalidated Approach

The first implementation reused the ordinary publication helpers unchanged. Each helper submitted
one durability-sensitive command and immediately classified its exact durable status with a point
read, assuming that an ambiguous command failure still permitted that read.

## Evidence

A scoped `AfterPersist` fault on `TurnActivated` committed the exact event and surfaced a
persistence error. The home correctly entered `Verifying`, so the helper's immediate status read
was gated. The broker could not distinguish the already durable event from an absent event and
closed the exact target with `TurnActivationPublicationFailed` instead of exposing the reconciled
start.

## Why It Failed

Home-store health is part of publication authority. A surfaced transient writer failure closes
state-dependent reads and writes until bounded exhaustive verification reopens the same generation.
Command reconciliation that ignores this gate cannot prove an ambiguous old-or-new outcome even
when the intended record is durable.

## Course Correction

The activation owner now uses the broker's existing same-generation verification authority. After
an ambiguous active-identity or activation-event publication error, it verifies only a `Verifying`
home, accepts a concurrent verifier's already healthy result only for the exact retained home
generation, and then classifies the exact command status. Exact durable state continues; absent or
colliding state fails the target closed. No generation-changing recovery, guessed identity, retry
with new authority, or start exposure occurs before exact classification.

## Completion-Unknown Frontier Finding

Fresh completion review found a second invalid assumption in the same activation cut. The
completion-unknown target-close branch treated every missing routed start as proof that no CAS
turn identity had been bound. The broker actually binds the exact CAS turn before publishing its
active identity, so activation-event failure can leave an active-only durable frontier even though
the start is never exposed.

A combined buffered-start, withheld-response, activation-event-failure regression reproduced the
bad frontier choice: cleanup attempted the pre-publication gate after active identity had already
advanced it. The branch now inspects the target's immutable bound-turn proof and delegates to the
existing absent, active-only, or activated frontier classifier. Only a target with no bound CAS
turn uses the pre-publication cleanup path.

## Affected Authority

Phase 19 later removed the response caller as an activation authority. The ordered broker still
performs the same-generation ambiguous-write reconciliation described here, but the connection
driver now releases an exact response only after a proof that broker activation is already durable;
the proof publishes nothing. `doc/systems/cas-live-syndic-transcript/design.md`,
`crates/beryl-app/doc/design.md`, and `doc/failures/cas-phase19-response-activation-authority.md`
control that superseding boundary. The live implementation is owned by the provider-broker
activation publisher and router response proof, not ordinary capture.
