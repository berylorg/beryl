# Composer Marker Feedback Readiness

Source assessment at `d0bf97f8a89145d5742ca7a448e07d1963516387`, 2026-10-04. This is
diagnostic evidence for the root plan, not design authority or acceptance of the remaining
Checkpoint 4 marker-feedback item. No production code, dependency or native GUI launch is part
of this assessment.

## Controlling Boundary

The [composer](../../features/composer/design.md#image-markers-and-assets) requires distinct
operation-size, temporary shared-capacity and storage refusals. A determinate refusal preserves
the entire prior draft, markers, caret, directed selection and undo/redo authority. An uncertain
outcome follows [Durable Mutation Reconciliation](../../features/composer/design.md#durable-mutation-reconciliation):
retain exact intent and evidence, suppress dependent mutations, and never infer noncommit from
unchanged pixels. Cut and subsequent paste are separate edits; refused paste preserves the
successful cut's usable undo and eligible clipboard representation.

The [composer GUI](../../features/composer/gui.md#composer-mutation-feedback-contribution)
contributes one exact record to the Notifications arbiter. It owns no second notice or panel
geometry. Determinate errors are dismissible; terminal unavailability is persistent and offers
no retry or repair. The [Notifications contract](../../features/notifications/design.md#main-conversation-notices)
owns bounded admission, omission, preemption and dismissal. Presentation removal never settles
mutation custody.

The [app edit boundary](../../../crates/beryl-app/doc/design-catalog-and-composer.md#edit-marker-and-candidate-adaptation)
preserves typed refusal and move-only settlement custody. The
[Syndic system](../../systems/syndic-conversation-history/design.md#durable-draft-piece-tree-candidate-sessions-marker-identity-label-admission-and-marker-commitment)
owns the fixed production profile: isolated charge is classified before aggregate occupancy;
checks occur at bounded quanta and an earlier check reserves no future capacity. The profile
limits an insertion-bearing operation, not existing draft population, text-only edits,
removal, history or marker-seal publication. Internal charge ceilings are not a public marker
count. Neither profile tuning nor splitting one edit is an implementation option.

Canonical [conversation composer](../../gui/widgets/conversation-composer/spec.md),
[image marker](../../gui/widgets/image-marker/spec.md) and
[main-window notice](../../gui/widgets/main-window-notice/spec.md) own reusable presentation.
The registered [text-input](../../gui/external-specs.md#gpui-text-input) owns range-backed
editing, evidence/replay, compact selection/history and terminal widget settlement. Existing
`main-window.user-input-panel` and `main-window.overlays` mounts suffice. No new widget or slot
is needed for refusal feedback.

Composer, Notifications and app select `production-application/v2` with
`external-side-effects/v2`. This evidence-only phase requires semantic completion review;
consequential implementation later requires independent semantic review of its affected boundary.

## Existing Production Consumers

- [`admission/readiness.rs`](../../../crates/beryl-app/src/composer_host/mutation/admission/readiness.rs)
  maps Syndic source, page and assignment refusal variants to the host's typed
  `OperationTooLarge` and `CapacityUnavailable`; storage errors retain their provenance.
- [`dispatch/evidence.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner/dispatch/evidence.rs)
  intercepts exact widget evidence before mutation begin, drives bounded evidence/replay, and
  consumes `ComposerHostMutationEvidenceOutcome`. `finish_mutation_evidence` rejects widget
  preflight on determinate refusal without adopting a candidate. Pending and unavailable outcomes
  retain the exact evidence owner; unavailable marks that owner terminally unavailable.
  `record_mutation_feedback` keeps selection, mutation key and typed feedback together.
- [`dispatch.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner/dispatch.rs)
  retains typed post-begin outcomes and distinguishes committed edit, committed intermediate
  work and unavailable presentation. A committed successor-proof failure cannot become an
  unchanged-draft noncommit claim. Existing opaque host outcome flights retain finalization,
  reconciliation and required cleanup; a status read is not their substitute.
- [`conversation_composer_owner.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner.rs)
  exposes feedback only for the selected exact owner in Live or Fencing state.
- [`notices/composer.rs`](../../../crates/beryl-app/src/main_window/shell/notices/composer.rs)
  reads that owner's feedback through the currently mounted contribution. Operation replacement
  removes the old feedback token; owner or mount replacement updates its corresponding subscription.
  `sync_composer_notice`
  admits one bounded arbiter record, does not recreate a dismissed determinate record, and
  retains persistent feedback for later admission under the existing capacity policy.
  `feedback_text` already supplies smaller-selection, released-capacity and storage explanations,
  plus distinct unavailable explanations with no commands.

Thus the central typed refusal-to-visible-notice consumer is already mounted. Reimplementing it
would duplicate accepted work. Removing a notice or owner subscription is only presentation
cleanup; it must not discard the service/host's unresolved flight or make an unavailable editor
writable.

## Evidence Coverage And Remaining Gaps

The [accepted marker-admission record](../../failures/composer-marker-admission.md#evidence-and-status)
records 156 distinct focused cases across overlapping runs, independent semantic review and
canonical dependency publication. This assessment credits that historical evidence for its
documented boundary; it does not claim a new test run or certify every later source revision.

- **Distinct mounted refusal and dismissal:**
  [`notice_mount/composer_feedback.rs`](../../../crates/beryl-app/tests/notice_mount/composer_feedback.rs)
  has `marker_limit_feedback_preserves_editor_and_exact_notice_routes`, a real BeforeCommit
  storage-refusal case, and an AfterPersist persistent-request case. Tests compare candidate
  selection, caret/selection, widget history frontier and realized marker count; assert unchanged
  shell geometry; check guidance, stale dismissal, distinct operation identities and no commands.
  Size and capacity cases use explicit test-only limit overrides, not the production fixed profile.
  Their small draft and realized marker count do not prove preservation of every nonresident
  marker in a large draft.
- **Source refusal and resource release:**
  [`composer_marker_evidence/refusal.rs`](../../../crates/beryl-app/tests/composer_marker_evidence/refusal.rs)
  proves distinct typed refusals, no durable mutation begin, unchanged exact prior binding,
  released settlement custody and empty admission head/charge. It separately exercises storage
  noncommit and stale operation/predecessor generations.
  [`flights.rs`](../../../crates/beryl-app/tests/composer_marker_evidence/flights.rs) exercises
  cancellation and service disposal while a submitted build flight still needs exact drain.
  These are host/component witnesses, not complete mounted large-draft refusal cycles.
  Syndic's [`submission/refusal.rs`](../../../crates/syndic-storage/tests/draft_marker_admission_submission/refusal.rs)
  includes exact production-charge boundary classification, page refusal and refusal after a
  staged prefix. Its pure charge-classifier witness is distinct from an actual oversized edit;
  page and assignment integration cases use test overrides to exercise refusal cheaply.
- **Ambiguity and committed failure:** existing host recovery witnesses and mounted typed transport
  witnesses preserve actual post-begin custody. The accepted record distinguishes injected
  transport from real storage recovery and records a real postcommit successor-read failure.
  Keep these distinctions. Persistent notice tests do not establish a consuming HomeStore recovery
  operation on their shared mounted store, nor may a fresh retry stand in for retained custody.
  [`mutation_completion.rs`](../../../crates/beryl-app/tests/notice_mount/mutation_completion.rs)
  contains intermediate-build, committed-cleanup, retired-owner and actual successor-read failure
  witnesses. It checks exact feedback keys and retained or released host custody as appropriate.
- **Large drafts:**
  [`mounted_composer_scale.rs`](../../../crates/beryl-app/tests/mounted_composer_scale.rs)
  exercises multi-MiB activation, retargeting, edits, history, autosave, EOF realization,
  capacity filler and disposal. Its realization assertions cover current and high-water bytes,
  pages, objects, geometry and combined activation residency. This is useful fixture and budget
  evidence, but not a combined refusal witness. A text-byte size comparison alone does not prove
  that a marker operation exceeds Syndic's association/index charge profile.
- **Cut then refused paste:**
  [`conversation_composer_gpui.rs`](../../../crates/beryl-app/tests/conversation_composer_gpui.rs)
  covers composite marker clipboard ordering, write-before-cut, contiguous limits, release fencing
  and late cut preparation. These do not establish a later production clipboard paste refusal
  or usable cut undo after that refusal.

## Missing Clipboard Consumer

The subsequent [clipboard readiness assessment](clipboard-readiness.md) traces the actual native
and private-source boundaries and records the authority gaps that prevent production mounting.

[`construction.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_owner/construction.rs)
routes copy/cut to the owned bounded clipboard workflow, but emits `RichPastePropagated` for
paste. [`conversation_composer_mount.rs`](../../../crates/beryl-app/src/main_window/conversation_composer_mount.rs)
checks exact selection and re-emits that event; it does the same for `ClipboardLimitExceeded`.
An exhaustive exact-identifier search in app Rust source finds no downstream production consumer
of `RichPastePropagated`. The mount test observes propagation rather than completing the paste.
The production clipboard writer writes fallback text, so these paths also do not demonstrate
retention of the eligible private marker representation for subsequent paste.

This is a missing visible workflow, not a defect in the already mounted refusal notice.
Injecting a proposal-page marker edit cannot establish the missing clipboard command consumer.
The [image feature](../../features/image-assets/design.md#paste-and-draft-outcome) and
[image system](../../systems/image-assets/design.md#bounds-and-recovery) define pending,
admission, cancellation and finite resources; Syndic already authenticates candidate/cut provenance.
Before planning full clipboard mounting, a bounded readiness review must trace the platform
representation, private-token eligibility/lifetime, captured insertion intent, bounded source
and cancellation ownership through actual public APIs. Any missing material ownership or lifecycle
decision belongs in those design authorities, not in this record or an implementation experiment.

## Derivable Next Boundary

One architecture-ready verification boundary can qualify the already mounted refusal path on a
representative large draft. Extend established virtual GPUI fixtures to exercise exact size,
temporary capacity and storage refusal through normal owner dispatch and Notifications, retaining
the canonical production profile. Compare the immutable root and exact history authority as well
as realized caret/selection and markers; browse nonresident ranges after determinate refusal and
prove a subsequent smaller edit and undo/redo remain usable when healthy. Repeated refusal and
dismissal cycles must keep editor and service residency bounded and release settled owned custody.
Use package fixed-profile evidence for actual isolated/aggregate classification and label all
test-only overrides. Storage failure must retain its real health/recovery restrictions.

This boundary needs no new architecture or production rewrite unless qualification finds a
bounded contract violation. It does not accept the whole tracker item. Clipboard readiness and
the actual cut/failed-paste/undo workflow remain separate pending work, followed by combined
acceptance for the remaining Checkpoint 4 item. Native application launch and dependency changes
are not prerequisites for these bounded virtual mounted witnesses.

## Completion Review

Independent source-backed semantic review passed without blocking findings. Root corrected the
reviewed distinction between feedback-token removal and owner/mount subscription replacement.
Local link targets, heading spacing and scoped Git whitespace checks passed. No new runtime test
claim is made; the tracker item remains open pending the qualification and clipboard boundaries.

## Subsequent Qualification

The [large-draft qualification](qualification.md) completes the coupled direct-marker refusal
witness with focused virtual mounted tests and independent review. Its evidence separates test
overrides, actual storage faults and reused production classification. The missing clipboard
consumer and complete tracker acceptance remain pending.
