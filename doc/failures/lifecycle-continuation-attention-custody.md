# Continuation Attention Requires Exact Completion Custody

## Invalidated Assumption

Removing a lifecycle outcome and treating every later error as cancellation loses the accepted
attempt before its final disposition is known. Conversely, reporting failure immediately when a
home closes can contradict a final continuation command that already committed. A stop of live
compaction targets its provider turn, not the original yielding turn whose intent must be cancelled.

## Correction

The accepted attempt owns pending failure attention until cancellation, proven admission or failure
settles it. Final continuation settlement takes that exact custody under the existing settlement
fence. A committed result suppresses failure before later-error conversion. An indeterminate result
installs its original reconciliation scope and uses exact reconciliation proof; it never constructs
another continuation. Home failure without such proof ends the pending attempt with bounded feedback.

Compaction admission binds its exact provider turn to the original yield. An admitted stop cancels
that original attempt, including when provider loss follows. Ordinary shutdown cancels accepted
intents before compaction installation as well as installed operations. Genuine content preparation
failure still reports feedback while preserving queued input; a queue alone is not proof that final
settlement selected the user-input winner.

## Decisive Evidence And Verification Lessons

The production-handler tests in
[continuation_attention.rs](../../crates/beryl-app/tests/lifecycle_yield/continuation_attention.rs)
route a real yield and compact-start request, then inject exact provider and storage outcomes.
Pausing after the final command distinguishes committed admission followed by home loss from an
indeterminate admission whose home fails before reconciliation. A routed stop exercises the distinct
compaction and yielding identities.

Failure tests must not retain a synchronous live-home command permit across capture: failure
retirement drains those permits. They must also perform terminal connection shutdown after the
failure cut before joining capture; ordinary invalidation cannot replace that shutdown. Correcting
those fixture lifetimes exposed a real panic when capture requested a scheduler notifier from an
already retired attachment. That lookup now returns a typed error and releases exact yield custody.

The controlling contracts are [Lifecycle Yield](../features/lifecycle-yield/design.md),
[Notifications](../features/notifications/design.md), and
[CAS-live](../systems/cas-live-syndic-transcript/design.md). Process/window assembly and broader
shutdown mounting remain separate acceptance boundaries.
