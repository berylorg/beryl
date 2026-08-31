# Catalog And Composer

This supplement is normative only for its bounded beryl-app catalog and composer role and is
governed by [design.md](design.md). It does not independently declare engineering rigor.

## Catalog And Claims

- The process retains compact catalog revisions, bounded query pages, and cursors, not a complete
  resident catalog. The adapter exposes revision-bound pages and stable row identities without
  owning realization, focus, or visible policy.
- The non-GPUI projection coordinator prepares one named Syndic thread at one stable home revision
  and composes typed summary, runtime/root, claim, and catalog-row contributions as one
  all-or-nothing home command. Exact agreement is a no-op; missing or stale authority is typed.
- The app maps only source-owned facts. It does not choose title precedence, inspect history,
  independently normalize search, enumerate CAS threads, read CAS history, or derive Beryl metadata
  from backend names or working directories.
- Creation, pristine reuse, selection, occupancy, additional-window acquisition, and claim release
  call typed atomic operations, not app-local check-then-act logic. Visible behavior belongs to the
  [conversation-threads feature](../../../doc/features/conversation-threads/design.md).

## Range-Backed Composer Host

- One selected host opens one exact editor-candidate session from the durable selector and exposes
  revision-bound bounded text and zero-width-marker pages to the app-neutral widget. It never
  reconstructs or retains the complete draft.
- Widget range and marker requests map to typed Syndic ranges without widening, merging, or
  reinterpreting scopes. Resident text, marker, geometry, edit, IME, and history facts remain
  within configured page, byte, and per-frame budgets.
- Restoration contains only exact root and extent binding, caret, directed selection, scroll
  anchor or continuation, durable history frontier, and undo/redo availability. It contains no
  text pages, marker collection, piece tree, layout graph, or draft-sized inverse content.
- Capacity saturation retains exact logical draft, selection, history, and extent using bounded
  filler and typed unavailability; it does not become a logical content limit. Product behavior is
  in the [composer feature](../../../doc/features/composer/design.md), and resource policy in the
  [bounded-resource system](../../../doc/systems/bounded-resource-dataflow/design.md).

## Edit, Marker, And Candidate Adaptation

- Typing, paste, cut, deletion, marker operations, undo, and redo enter one exact predecessor-
  qualified transaction session. One logical operation publishes one complete successor or one
  non-mutating terminal outcome; no partial prefix becomes visible or durable.
- Every widget proposal page is nonempty, has at most 256 items, and retains at most 65,536 bytes.
  The host validates exact binding, operation, lane frontier, cursor, ordinal, canonical page and
  cumulative identity, and checked totals before translation or durable admission.
- Accepted payload pages release at their typed frontier. Large operations retain only bounded
  current-page, cursor, digest, endpoint, intent, and custody state.
- Marker-changing edits use one opaque Syndic label-readiness operation bound to exact home,
  thread, draft, session, candidate, predecessor, and destination authority. The app transports
  bounded pages and opaque commands or receipts; it never chooses labels, builds a registry,
  compares dependency-private proof facts, scans the draft, or substitutes another operation.
- Cancellation discards unadmitted work only when typed reconciliation proves noncommit. After
  admission, exact custody drains or transfers until terminal. Stale, conflicting, exhausted,
  missing, or corrupt readiness publishes no marker, caret, selection, candidate, or history change.
- Every admitted edit uses its typed public settlement. Indeterminate remains reconciliation
  custody; only proven committed adoption advances the widget and candidate session.

## History, Publication, And Submission

- Syndic owns durable edit-history roots and frontiers. The app exposes only exact undo/redo
  availability and transports one opaque resolved historical target; it retains no inverse text,
  root graph, or history-sized collection.
- Autosave and flush capture one immutable adopted candidate and matching frontier. Newer edits may
  continue, but a completion clears only its captured generation.
- Marker-changing publication streams one immutable root's authenticated pages through typed Syndic
  and Asset contributions and publishes atomically. The app retains bounded pages, cursors, opaque
  proofs, and custody and constructs no cross-domain proof mapping. Marker-unchanged publication
  performs no marker scan.
- Submission starts only after a flush proves one immutable candidate is the current durable draft.
  The app drives bounded materialization and reference preparation and passes opaque typed
  contributions into one atomic acceptance command.
- Only exact accepted send-and-clear disposes the editor session, closes its history frontier, and
  clears the composer. Other or ambiguous outcomes preserve the coherent editor and exact evidence.
- Composer history retains a fixed-capacity set of compact sealed Syndic input references and
  recalls content through range-backed copy-on-write drafts, never a payload history.
- Every success, cancellation, failure, supersession, session disposal, generation loss, and
  service disposal releases the exact flight, page, cursor, reservation, and custody.

## Activation And Title Adapters

- Selected and pending composers share the sole widget settlement coordinator and host-operation
  allocator across activation generations. The package adds no last-seen sequence authority.
- Activation history updates only after coherent promotion. Failed, cancelled, stale,
  already-selected, or restore-time activation invents no navigation entry.
- The title adapter consumes exact Syndic eligibility and bounded source facts, uses one bounded
  maintenance backend session, validates the typed result, and submits one one-way Syndic attribute
  mutation. It never reads CAS thread names or history or occupies a selected foreground stream.
