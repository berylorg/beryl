# First Runtime Editor Recovery

## Current Disposition

Setup mounting is paused on 2026-10-07. Accepted runtime/root admission and both executable
launch forms remain unchanged. No native GUI, picker, backend process or clipboard was exercised.
The incomplete mounting draft is unaccepted and excluded from production source.

## Invalidated Assumption

The phase 734 mounting approach reused `InitialComposerCandidate` to open the first conversation
editor under the exact committed onboarding window/claim and retained admission lease. This is
appropriate for ordinary construction, but its cleanup path is insufficient when Home fails after
editor opening and before publication.

[InitialComposerCandidate::drive_retirement](../../crates/beryl-app/src/main_window/initial_composer/retirement.rs)
requires ordinary storage access for opening reconciliation, preparation of fresh-session
abandonment and classification of its outcome. Syndic's
[fresh-session abandonment implementation](../../crates/syndic-storage/src/draft_piece/publication/abandon_fresh.rs)
accepts `&HomeStore` in both `prepare_abandon_fresh_draft_editor_candidate_session` and
`reconcile_abandon_fresh_draft_editor_candidate_session`. It has no equivalent consumer of borrowed
`HomeCandidateRecoveryAccess`. Failed Home cannot provide ordinary admission; recovery cannot
pretend this original unpublished editor was disposed by dropping it.

The existing Running-thread publication path also requires a selected predecessor and cannot
attach the first threadless shell. Restore preparation requires a Restoring claim; the committed
onboarding claim is Active. Neither is a valid substitute for first-conversation preparation.

## Recommended Correction

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

## Resumption

The bounded draft at `.tmp/runtime-setup-mount-draft` contains graph-owned admission flights,
first-conversation preparation/attachment, shared picker variants and the incomplete shell/catalog
consumer. It has not been compiled or behaviorally qualified. Restore only after the recovery
prerequisite is accepted and reconsider all source against then-current authority. The prior
[mounting readiness](../audits/runtime-root-creation-readiness.md) established admission readiness
but did not qualify this first-editor failed-home disposal boundary.

## Review

Independent source/authority review confirmed the gap and found no blocking documentation findings.
The recommendation explicitly separates retained immutable intent/outcome evidence from retired
graph resources and covers committed opening before classification. Neither the mounting draft nor
the proposed recovery capability has compilation or behavioral acceptance.
