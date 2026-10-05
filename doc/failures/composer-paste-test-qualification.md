# Composer Paste Test Qualification Corrections

Scope: captured composer paste qualification, 2026-10-05.

The initial combined nextest selector used the complete `app_services::` module when only
service-graph configuration and recovery were intended. That selector included unrelated native
window harnesses. After 83 passing checks, its two-resident native recovery harness aborted with
a thread stack overflow. The run ended and its exact abandoned temporary fixture was reclaimed.
The corrected qualification selects the six pure service-graph cases, pure recovery-graph case
and affected host, isolated GPUI composer and Notice cases explicitly. Do not use a broad module
selector as a substitute for enumerating harness behavior when native windows are excluded.

A fresh-image storage test also expected a successful retry and ordinary reads on the same
home generation after `BeforeSidecarWrite`. Physical sidecar storage failure is structural and
fails that generation. The correct test checks the retained editor and Notice, the failed health
and rejection of further ordinary reads. Recovery requires the existing generation replacement
boundary; a same-generation retry cannot establish success. The same correction applies to shell
Notice tests. Repeated dismissible Notice identity remains covered by injected clipboard-write
failure, which does not fail storage.

The focused mounted run exposed a production producer defect: marker-only replacement emitted
its marker page, then used the proposal-page ordinal to decide whether an empty UTF-8 replacement
was still required. Because the marker had advanced that ordinal, selected text survived the host
mutation and the widget rejected the successor extent. Empty replacement must instead depend on
whether any inserted text was emitted; evidence and staging replay must both retain that exact
replacement. Settlement errors must also retain their own diagnostic instead of being reported
as a history-frontier failure.

Private-origin disposal fixtures must publish the candidate marker edit before disposing its
session. The production lifecycle requires clean equality between the published and newest roots;
discarding an unpublished source cannot prove queued foreign-origin disposal behavior.

Persistent mutation feedback also exposed a production gate gap. The owner cleared that feedback
and acquired a new clipboard snapshot because its shared mutation gate omitted current unavailable,
admitted-unavailable and committed-unavailable outcomes. Those persistent outcomes must disable
dependent mutation and submission until ordinary reconciliation releases the boundary. The gate
now includes them and synchronizes widget readonly state when feedback or dispatch changes.
Determinate size, capacity and storage refusals retain their ordinary distinct handling.

Authenticated private-marker metadata includes the compact origin selector and exceeds the old
fixture's 256-byte mutation object budget. Successful private-paste qualification uses the supported
4096-byte budget. A valid smaller configuration still needs an ordinary refusal: preparation must
check each known marker metadata weight against the configured object page ceiling before widget
begin. Deferring this check to evidence submission stranded a pending edit on
`ObjectByteLimitExceeded`. Dedicated mounted refusal coverage must preserve the exact prior editor,
private-source eligibility and queue reuse rather than bypassing that ceiling.

Atomic private-origin refusal exposed redundant host reconciliation. Outcome-aware begin
reconciliation already proved `Source`, but the host retained `begin_attempted` and cancellation
then called generic reconciliation, which rejects marker-readiness begins. Clear that flag only
after proven `Source`; unknown and failed reconciliation still retain their original custody.

A generic mounted driver returned on any selected identity change, including an intermediate
session-generation update while staging still retained the predecessor root. Assertions after
settlement then inspected that stale captured identity. Paste success must wait for a changed root
and completed pending work and reacquire the authoritative selected identity. Proven noncommit
preserves root, history, logical range and widget state; it need not freeze internal session
generations used by ordinary staging.

A restart identity fixture also reused one live editor session with a new widget coordinator whose
operation counter restarted at one. The supported lifecycle retains one coordinator for that
session. A restart fixture must dispose and reactivate the normal host/editor session against the
persisted root before creating the fresh coordinator and process source owner.

The stronger private-paste driver exposed a real builder rejection: diagnostics retained
`Rejected(DuplicateEmptyRange)`. Host translation emitted the marker and following first text at
one collapsed insertion point as two replacements. The existing continuation grammar must join
nonempty first text to a preceding fragment at that exact collapsed range. Nonempty selected
replacement ranges and later text continuation retain their original handling.

Fresh-session activation reads the published draft selector. Runtime service disposal does not
publish a dirty candidate. The restart fixture must complete ordinary host flush and marker sealing,
prove the published root equals its committed candidate, and then open the fresh session; otherwise
its marker-page range lies outside the published empty root and correctly fails gap validation.
