# Opening History Is Not Unsaved Work

Phase 302's real fresh-host tests exposed a mismatch between editor opening and publication:
opening forks the durable history for the private live session, so its history identity differs
without a new edit. The host treated that difference as dirty and attempted publication, whose
captured-adoption check demanded an edit journal. Merely admitting generation zero would not fix
the boundary: opening can inherit any durable candidate generation, while publication correctly
requires an advance. Submission and ordinary disposal also required equal live/published histories.

The Operator approved recognizing an exactly authenticated unchanged opening as already saved.
The contract lives in [composer behavior](../features/composer/design.md),
[the app adapter](../../crates/beryl-app/doc/design-catalog-and-composer.md), and
[Syndic draft storage](../../crates/syndic-storage/doc/design-draft-storage.md). Phase 308 separately
corrected the [missing submission disposal receipt](submission-editor-disposal-receipt.md).

Phase 304 implemented and independently accepted the storage boundary. One shared checkpoint proof
authenticates exact opening, ordinary-edit, or historical-adoption authority in preparation and
transactional validation. An opening uses its immutable durable frontier and exact session fork,
at any inherited generation. Saved queries double-observe mutable anchors and distinguish concurrent
change from stable invalid authority. Captured publication proves its captured checkpoint even after
the live editor advances; a historical root need not originate in the current session.

Ordinary disposal authenticates the opening relationship and atomically normalizes its final history
with the existing exact receipt. Its request names the published pair; unpublished-target
abandonment names the private pair and retains separate admission rules. Canonical request bytes,
byte-equal terminal pairs, publication generation advancement, bounded reads, and exact replay remain
enforced. Submission status validates the receipt's exact source candidate and normalized terminal
state, including after the current draft is replaced.

All twelve `phase304_editor_checkpoint` tests and eight selected Phase 308/167/176 regressions passed
through locked nextest with `test-faults`. They cover empty and populated nonzero-generation openings,
saved/dirty distinction, stale and substituted authority, ordinary disposal and replay, atomic fault
cuts, both opening submission routes, empty rejection, capture followed by newer edits, and actual
Undo/Redo publication. The locked storage library check, formatting, and independent adversarial
review passed. There is no dedicated sealed-import baseline fixture; imported-origin behavior is
covered by the shared provenance reasoning rather than a claimed dynamic import test.

The host's clean-state, flush, and submission integration is implemented but remains unaccepted in
Phase 307. Its native-lineage adapter must use background authentication and advancement rather
than fabricating a clean-disposal state or calling storage on the GUI thread. The Operator paused
work at the compiled checkpoint with focused test and final adapter-review gaps recorded in the
[plan](../plan.md). Storage acceptance does not establish mounted close behavior or resolve the
separate request-sequence and gate-release work before Phase 302 acceptance.
