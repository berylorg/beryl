# First Runtime Editor Recovery

## Current Disposition

Unpublished first-editor cleanup is accepted on 2026-10-07; see
[acceptance evidence](../audits/unpublished-first-editor-recovery-qualification.md). Resumed setup
mounting exposed a separate missing consumer: fresh first-editor construction and same-shell
attachment during private recovery of committed onboarding. Cleanup remains valid, but does not
prepare that successor. Setup mounting is blocked pending this correction. Accepted runtime/root
admission and both executable forms remain unchanged. No native GUI, picker, backend process or
clipboard was exercised. All mounting source is archived, unaccepted and excluded from production.

## Invalidated Assumption

The phase 734 mounting approach reused `InitialComposerCandidate` to open the first conversation
editor under the exact committed onboarding window/claim and retained admission lease. This is
appropriate for ordinary construction, but its cleanup path is insufficient when Home fails after
editor opening and before publication.

[InitialComposerCandidate::drive_retirement](../../crates/beryl-app/src/main_window/initial_composer/retirement.rs)
requires ordinary storage access for opening reconciliation, preparation of fresh-session
abandonment and classification of its outcome. Syndic's
[fresh-session abandonment implementation](../../crates/syndic-storage/src/draft_piece/publication/abandon_fresh.rs)
accepted only `&HomeStore` at discovery in both `prepare_abandon_fresh_draft_editor_candidate_session` and
`reconcile_abandon_fresh_draft_editor_candidate_session`. It then had no equivalent consumer of borrowed
`HomeCandidateRecoveryAccess`. Failed Home cannot provide ordinary admission; recovery cannot
pretend this original unpublished editor was disposed by dropping it.

The existing Running-thread publication path also requires a selected predecessor and cannot
attach the first threadless shell. Restore preparation requires a Restoring claim; the committed
onboarding claim is Active. Neither is a valid substitute for first-conversation preparation.

## Accepted Cleanup Correction

Establish a narrow typed same-home recovery capability for this exact unpublished fresh editor
before mounting setup. Syndic should authenticate its original open/disposal identities and
unchanged opening root/history through borrowed private recovery access, prepare and classify
fresh-session abandonment there, and retain ordinary exact-outcome semantics. The app must preserve
the original immutable opening/disposal intent and exact outcome/reconciliation evidence separately
from retired graph resources. Fresh recovery access must authenticate cleanup without transferring
a retired ordinary handle or publication lease to the new generation; old graph retirement must
still complete before reopening.

Update the owning Syndic and app recovery authority before implementation, then qualify failed
Home after editor open, including a committed opening whose classification fails before `opened`
is populated, exact noncommit/commit/ambiguity, collision, substituted or dirty sessions, repeated
recovery and original-owner disposal. Independently review that capability before
resuming the same-shell setup mount. This proposal does not authorize a generic storage bypass,
fresh editor over unsettled custody, publication from retained failed-generation facts or a
helper-only acceptance of phase 734.

## Successor Construction Gap

After first-runtime admission commits, State already contains the runtime, home root, thread,
draft and paired Active claim for the original window. Home can fail before a selected composer
is attached, leaving the visible shell threadless while its durable window is selected.

[ThreadlessRecoveryWindow::prepare](../../crates/beryl-app/src/app_services/recovery_threadless.rs)
requires a sole window with no selection, target, fallback or claim and an empty runtime registry.
It correctly rejects these committed onboarding facts. The ordinary Running recovery contract in
[shell lifecycle authority](../../crates/beryl-app/doc/design-shell-lifecycle.md#ordinary-running-home-recovery-ownership)
also requires threadless membership to retain its no-selection/no-fallback contract and selected
attachment to preserve a resident editor. Neither branch covers this unpublished first selection.

[Ordinary recovery capture](../../crates/beryl-app/src/running_owner/shutdown_session/recovery/ordinary.rs)
derives selection from the mounted resident, so this still-threadless shell captures no selection.
[UnchangedRunning validation](../../crates/beryl-app/src/running_owner/unchanged_running.rs)
then rejects the durable selected window. The correction must explicitly route original onboarding
custody through ordinary capture and preserved-set validation, as well as the attachment driver.

[MainWindowComposerRecoveryPreparation](../../crates/beryl-app/src/main_window/conversation_composer_owner/recovery/preparation.rs)
requires a retired resident editor and restoration seed. Its
[candidate source](../../crates/beryl-app/src/main_window/conversation_composer_owner/service/candidate_source.rs)
authenticates that predecessor's binding, saved checkpoint and positions before rebinding it.
An unpublished first editor has no published resident to preserve, and accepted cleanup disposes
its original fresh session. Manufacturing a predecessor would violate that contract.

[InitialComposerCandidate::advance](../../crates/beryl-app/src/main_window/initial_composer/activation.rs)
prepares opening and host activation through ordinary Home access.
[Syndic opening preparation](../../crates/syndic-storage/src/draft_piece/session.rs)
accepts `&HomeStore`; its existing candidate reconciliation counterpart settles original outcomes
but does not prepare a fresh opening.
[SyndicComposerHost initial activation](../../crates/beryl-app/src/composer_host/activation.rs)
also requires ordinary access for its initial requests. A private reopening candidate cannot use
those ordinary paths, and publishing the graph first would violate the required prepublication
editor and shell preparation boundary.

## Recommended Complete Correction

Define the exact committed-first-conversation recovery branch in the owning app shell/admission
and shared recovery authority before implementing it. Retain original onboarding identity and
actual admission/opening outcomes outside retired graph resources, including failure before the
first-editor owner is created. Ordinary recovery capture and routing must distinguish these exact
committed-first facts from both empty-window and preserved-resident facts. Authenticate the
committed selected window, thread/draft and Active
claim through fresh candidate access; do not weaken the empty-window or preserved-resident branches.

After complete old-resource retirement and accepted original-editor cleanup, prepare a fresh editor,
its host/service and bounded initial content through private candidate access. Retain every new
opening/disposal identity and reconciliation owner across cancellation, candidate failure and
repeated recovery. Attach composer and transcript to the same surviving native reservation, prepare
fresh appearance/notices/subscriptions, and revalidate the exact window/claim and complete binding
before whole-graph publication. No old service or admission lease grants successor authority.

Qualify the complete branch, including failure immediately after onboarding commit, before opening,
after opening but before classification, during service/widget construction and after partial
attachment. Cover exact noncommit/commit/ambiguity/collision, stale candidate delivery, repeated
failure, cleanup, and close/Exit races. Independently review the complete production recovery
consumer; candidate wrappers alone are insufficient. Healthy setup mounting remains phase 734.

## Resumption

The bounded draft at `.tmp/runtime-setup-mount-draft` contains graph-owned admission flights,
first-conversation preparation/attachment, shared picker variants and the incomplete shell/catalog
consumer. The newer `.tmp/runtime-setup-mount-resumed-draft` preserves 34 source/test files against
accepted `0c34dfd3`, including bounded page consumers and unexecuted picker tests. Each archive is
bounded to 2 MiB and has exact path/hash manifests; both are uncompiled and behaviorally unqualified.
The production source was restored to the accepted baseline after every copied hash matched.
Restore only after complete recovery readiness is established and reconsider source against current
authority; preserve accepted initial-composer cleanup rather than copying older files over it. The prior
[mounting readiness](../audits/runtime-root-creation-readiness.md) established admission readiness
but did not qualify this first-editor failed-home disposal boundary.

## Review

Independent source/authority review confirmed the gap and found no blocking documentation findings.
The recommendation explicitly separates retained immutable intent/outcome evidence from retired
graph resources and covers committed opening before classification. Neither the mounting draft nor
the proposed recovery capability had compilation or behavioral acceptance at that blocker checkpoint.

The subsequently authorized cleanup correction is now accepted. Review found and resolved an exact-absence
gap involving an orphan receipt at the original disposal key, while keeping the shared ordinary
algorithms and complete old-graph retirement. The mounting draft remains unaccepted.

Independent read-only review of resumed mounting confirmed the successor construction gap and the
complete correction above, including ordinary capture/routing, candidate host initialization and
no-predecessor attachment. It also identified off-GUI source qualification, partial-service disposal
and coherent transcript attachment still required in the unaccepted healthy mount. No new recovery
branch, mounting implementation or target authority is accepted by this readiness diagnosis.
