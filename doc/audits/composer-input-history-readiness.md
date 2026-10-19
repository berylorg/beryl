# Accepted-Input Composer History Readiness

Status: source inventory complete; mounted browsing is not accepted. Reviewed against source at
`2e24a258` and the linked authorities. No production source or tests changed for this assessment.
The later authority reconciliation below resolves the original target-decision gaps; the source
inventory and earlier recommendation remain historical evidence of the readiness boundary.

## Controlling Boundary

The [Composer feature](../features/composer/design.md#composer-history) requires GUI-local,
bounded session history of exact accepted inputs, consecutive duplicate collapse, oldest-eligible
eviction, and focused `Alt+Up`/`Alt+Down` browsing in the current conversation scope. Rejected and
empty submissions do not enter it. Recall includes image markers, clears selection and places the
caret at the logical end; forward traversal past the newest entry restores the pre-browse draft.
Editing a recalled entry leaves the retained input unchanged. Thread-edit mode and direction
exhaustion leave the draft and presentation unchanged.

Successful activation publishes composer-history scope with the coherent selected conversation.
The [App supplement](../../crates/beryl-app/doc/design-catalog-and-composer.md#range-backed-composer-host)
requires fixed-capacity compact sealed Syndic input references and range-backed copy-on-write
recall. The [resource contract](../systems/bounded-resource-dataflow/design.md) requires bounded
working sets and release/eviction at every accumulation point. Durable editor Undo/Redo, accepted
input browsing and transcript history are three different responsibilities.

## Existing Production Inputs

- [Submission acceptance](../../crates/beryl-app/src/composer_host/submission/acceptance.rs)
  classifies committed, exact reconciled and already-accepted outcomes before calling
  `finish_submission_success`. Noncommit, cancellation, collision and unresolved reconciliation
  take separate branches. This is the existing exact acceptance point to inspect for a future
  compact history notification; successor-editor readiness is later and may fail independently.
- [Submission capture and completion](../../crates/beryl-app/src/composer_host/submission.rs)
  already flush and stream one immutable candidate. Completion wakes execution, then discards the
  pending request and active editor state. It does not publish an accepted-input history entry.
  [CapturedSubmission](../../crates/beryl-app/src/composer_host/submission/model.rs) carries
  candidate, selector, image-label authority, gate revision/state, Asset proof and materialization
  build identity, without a full payload copy.
- [FirstAcceptance](../../crates/syndic-storage/src/mutation/admission.rs) has authenticated source
  and materialization facts. Its result distinguishes idle acceptance, which names a canonical
  user item, from queued acceptance. The latter does not already have that same canonical-item
  result. A future descriptor must cover both without requiring transcript visibility or waiting
  for queued input promotion.
- The [slot adapter](../../crates/beryl-app/src/main_window/composer_slot/submission.rs) consumes
  `ExactSuccess(_)` by opening the successor. The
  [mounted owner](../../crates/beryl-app/src/main_window/conversation_composer_mount/submission.rs)
  then prepares/promotes the next editor and retires the predecessor. Its detached submission
  drainage also settles accepted work after the GUI wait disappears. Capturing history only in
  the new widget or after successor promotion would miss some exact acceptances.
- The [composer owner](../../crates/beryl-app/src/main_window/conversation_composer_owner.rs)
  retains selection, range input, mutation/clipboard/replacement flights and compact recovery
  state. It has no accepted-input history collection or browse cursor. Its
  [configuration](../../crates/beryl-app/src/main_window/conversation_composer.rs) supplies selected
  identity and range-widget configuration, without a composer-input-history retention setting.
- [Input event routing](../../crates/beryl-app/src/main_window/conversation_composer_owner/construction.rs)
  handles propagated Copy/Cut/Paste/Enter and marker events. The
  [root key handler](../../crates/beryl-app/src/main_window/conversation_composer_owner/render.rs)
  handles Escape for paste/replacement cancellation; neither mounts accepted-input browsing.
  Exact searches of live crate Rust sources found no `Alt+Up`/`Alt+Down`, `alt-up`/`alt-down`,
  accepted-input-history command or recall implementation. This excludes archived source.
- [ComposerHost history](../../crates/beryl-app/src/composer_host/history.rs) maps
  `RangeHistoryIntent` only to `MutationKind::Undo` or `Redo`, selecting an authenticated retained
  draft root. It does not enumerate accepted inputs. Its durable byte-budget retention must not
  become the GUI session-history entry bound.

## Recall Boundary Gap

The existing [sealed import authority](../../crates/syndic-storage/doc/design-draft-storage.md#sealed-input-import-and-replacement-intent)
and [request](../../crates/syndic-storage/src/draft_piece/import/request.rs) are specific to
historical replacement editing. Preparation in
[admission](../../crates/syndic-storage/src/draft_piece/import/admission.rs) requires an exact
saved, empty, quiescent editor; source authentication requires an idle gate with zero live
accepted work, an exact replacement target, and its canonical user-input item. The
[preflight](../../crates/syndic-storage/src/draft_piece/import/preflight.rs) also authenticates the
current selected transcript path/entry. Its eventual draft has replacement submission intent.

For example, after submitting `first`, the Operator types unsent `second` and presses `Alt+Up`.
The feature requires recalling `first` while retaining `second` for forward restoration. The
existing importer refuses the nonempty `second` draft; if `first` is still running, it also
refuses the active gate. Clearing `second`, waiting for idle, or manufacturing a replacement
target would change the required behavior. Reusing replacement import is therefore not a valid
implementation of browsing.

Immutable draft trees, bounded sealed-content decoding, historical Asset proofs and authenticated
root transitions are useful existing mechanisms. They do not by themselves authorize an ordinary
draft recall or pin the pre-browse root across later history eviction and editor retirement.
The owning technical contract must specify that authority and its complete lifecycle before code
planning. A compact root descriptor alone is not proof that the root remains retained or eligible.

## Decisions Needed Before Implementation

These are missing target decisions, rather than permission to weaken existing source guards:

- In the [Composer feature](../features/composer/design.md#composer-history), define the session
  owner and lifetime across successful thread switches, window close/reopen and same-home
  recovery. Current conversation scoping and non-persistence are specified, but do not settle
  whether revisiting a thread in the same window retains its earlier history.
- In the same feature, specify whether recall and forward restoration are ordinary undoable draft
  replacements, and which pre-browse position facts are restored. Existing generic Undo/Redo rules
  protect retained transitions, but do not define this browsing transition or browsing state after
  an edit/Undo. Define visible pending, cancellation and failure behavior for large recall.
- In the [App supplement](../../crates/beryl-app/doc/design-catalog-and-composer.md#range-backed-composer-host)
  and [Syndic draft contract](../../crates/syndic-storage/doc/design-draft-storage.md), define an
  exact accepted-source descriptor for idle/queued/steering acceptance, idempotent delivery into
  the bounded GUI session owner, ordinary-draft recall and pre-browse retention/restoration.
  Cover the existing [Asset proof boundary](../systems/image-assets/design.md#reference-and-label-evidence)
  for markers. Preserve original command/reconciliation custody through cancellation, stale
  delivery, selection retirement, service replacement and failure after a known commit.

Recommended direction for Operator review: retain a bounded history per window and conversation
for that window's lifetime; revalidate compact same-home references after recovery, and clear
them on close/process restart. Capture exact acceptance independently of successor presentation.
Give recall/restoration a typed ordinary-draft transition with bounded preparation and one atomic
adoption, preserving a compact authenticated pre-browse checkpoint. Treat successful recall and
restoration as ordinary undoable replacements; restore the original caret and directed selection
when leaving browsing forward. Keep the current coherent editor while preparation is pending,
allow cancellation before admission, and reuse the existing exact ambiguous-mutation feedback.
These recommendations are not yet authoritative decisions or implementation authorization.

The exact configured entry count, cache organization and scheduling mechanics may remain private
once the owner, eviction semantics and retention guarantees are settled. Consecutive duplicate
comparison must cover exact input semantics including markers through bounded work; neither
acceptance identity nor a different physical root alone establishes content equality.

## Verification Inventory And Next Boundary

Inspected existing test bodies and production call paths; no tests were run and no earlier test
result is claimed as qualification of browsing:

- [Exact-root submission](../../crates/beryl-app/tests/exact_root_submission.rs) covers idle and
  queued acceptance, bounded capture and empty refusal. The queued case confirms acceptance can
  precede a canonical submitted-turn item for that input.
- [Durable Undo/Redo](../../crates/beryl-app/tests/syndic_composer_history.rs) exercises root
  transitions, branching, compact authority and fresh activation. It is not accepted-input recall.
- [Mounted submission](../../crates/beryl-app/tests/mounted_composer_submission/start.rs) and
  [replacement import](../../crates/beryl-app/tests/historical_replacement_host.rs) provide relevant
  fixtures and existing gates; neither proves browsing or pre-browse restoration.

After the target decisions are reconciled, the next implementation boundary should be the complete
selected-thread `Alt+Up`/`Alt+Down` workflow, including bounded acceptance capture, source recall,
restoration and actual shell command routing. Supporting storage/App APIs belong with that
observable outcome unless a separate material architectural acceptance boundary is discovered.
Qualification must distinguish exact acceptance from refusal/uncertainty; prove duplicate collapse,
eviction and direction no-ops; recall large text and image inputs with bounded residency; preserve
and restore a nonempty draft; preserve source immutability after edits; and cover thread-edit mode,
selection change, recovery, stale completion, cancellation, committed-later-failure and disposal.
Use owning Cargo integration targets, focused native command evidence, relevant existing submission/
Undo/Redo/replacement regressions, final production checks and independent semantic/lifetime review.

Root completion review checked every readiness task against source and owning authority. The
inventory establishes the missing capture/recall consumers and identifies the contract work needed
before implementation. It does not accept browsing, choose those missing decisions, or authorize
using a replacement-edit command for ordinary history recall.

## Authority Reconciliation

Operator authorized clean architectural implementation and resolution of the missing recall
contracts. The Composer feature now selects window/conversation history lifetime, undoable
recall/restoration, positions and pending behavior; editor retirement ends browsing while accepted
entries may be revalidated after same-home recovery. App, Syndic, model and Asset authority define
bounded source preparation, acceptance capture, ordinary root adoption and original custody.

The accepted source anchor is a distinct acceptance-purpose form of the existing session-disposal
receipt, atomically written with acceptance. It preserves the 103-family Home declaration and all
ordinary/replacement record bytes. The [source-authority lesson](../failures/composer-recall-source-authority.md)
records why a new family and an unqualified ordinary disposal receipt were rejected during design.

Independent semantic review accepted the complete reconciled authority with no remaining blocker,
after correcting Redo branching, same-session checkpoint retirement, foreign-thread refusal and
closed build/header/frontier discrimination. The review inspected source acceptance/disposal fields
and exact schema relationships. This accepts architecture readiness only; implementation and
mounted workflow qualification remain separate.
