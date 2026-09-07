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

- Marker sealing consumes the single injected home-generation service owned by the
  [process service graph](design-shell-lifecycle.md#process-service-graph-and-windows); a host or
  window never constructs independent flight capacity.
- One selected host opens one exact editor-candidate session from the durable selector and exposes
  revision-bound bounded text and zero-width-marker pages to the app-neutral widget. It never
  reconstructs or retains the complete draft.
- Widget range and marker requests map to typed Syndic ranges without widening, merging, or
  reinterpreting scopes. Resident text, marker, geometry, edit, IME, and history facts remain
  within configured page, byte, and per-frame budgets.
- Host request numbering has one allocation owner for each live host/session. Publication, ordinary
  editing, history adoption, and other binding advances within that lifetime preserve its monotonic
  sequence. A new sequence belongs only to a fresh host generation; stale generations and reused
  request identities remain rejected, and exhaustion never wraps. This request sequence is distinct
  from the widget mutation/history operation allocator used across activation generations.
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
- Before durable mutation admission, the host consumes the widget's bounded evidence pass from
  the captured immutable producer. It preserves the exact operation and predecessor selection,
  transports insertion-bearing evidence through Syndic readiness, and retains only bounded pages,
  fixed lane closures, source handles, and opaque custody. Exact evidence EOF precedes proof
  consumption by storage `MutationBegin`. After admission the same producer restarts for staging;
  both complete lane closures and intended successor positions must match before finish/commit.
  Late evidence responses never enter staging. Cancellation before durable begin releases evidence
  and readiness through their typed owners; ambiguous admission retains ordinary reconciliation
  custody. Neither an insertion-only special case nor a complete edit buffer supplies this boundary.
- Marker-changing edits use one opaque Syndic label-readiness operation bound to exact home,
  thread, draft, session, candidate, predecessor, and destination authority. The app transports
  bounded pages and opaque commands or receipts; it never chooses labels, builds a registry,
  compares dependency-private proof facts, scans the draft, or substitutes another operation.
- Fresh image effects supply the ordinary admitted AssetId selector and the separate opaque Asset
  witness factory to Syndic. Existing candidate, cut, and accepted references retain their typed
  source selectors. Syndic derives preservation or allocation, including mixed edits; the app
  supplies no caller-selected label, assignment group, source treatment, or fabricated historical origin.
- After marker-aware begin, insertion-bearing staging translation obtains each final marker from
  Syndic's exact admitted-target point resolver and transports it unchanged into the proposal.
  The lookup is bound to active staging custody and the original predecessor and generation. It
  neither consumes targets nor retains a marker map. Fresh widget payloads remain AssetId-only
  through both passes; the assigned label enters only the storage proposal via Syndic resolution.
- The host preserves Syndic's fixed-profile `OperationTooLarge`, temporary `CapacityUnavailable`,
  and storage-failure distinctions through the composer result. It does not infer a public marker
  count from internal association or byte ceilings, raise the profile, automatically split one
  edit into partial adoptions, or treat a passed staging check as future capacity reservation.
- Cancellation discards unadmitted work only when typed reconciliation proves noncommit. After
  admission, exact custody drains or transfers until terminal. Stale, conflicting, exhausted,
  missing, or corrupt readiness publishes no marker, caret, selection, candidate, or history change.
- Every admitted edit uses its typed public settlement. Indeterminate remains reconciliation
  custody; only proven committed adoption advances the widget and candidate session.

## History, Publication, And Submission

- Syndic owns durable edit-history roots and frontiers. The app exposes only exact undo/redo
  availability and transports one opaque resolved historical target; it retains no inverse text,
  root graph, or history-sized collection.
- Autosave and flush capture one immutable adopted candidate and matching frontier. Autosave permits
  newer edits, but a completion clears only its captured generation.
- An unchanged opening is clean through Syndic-owned exact correspondence to the current durable
  checkpoint, including a nonzero inherited candidate generation. Flush authenticates that
  relationship without publishing the private history fork. It retains the exact live candidate
  identity separately from the durable selector/root/history checkpoint. A pending real publication
  still requires exact settlement; an already-durable opening cannot bypass its custody.
- Ordinary-close preparation fences new composer mutations while already admitted edits settle,
  then flushes the newest eligible candidate. Its exact attempt remains bound to the home, host,
  and mounted editor through later close obligations. Draft readiness alone neither disposes the
  editor nor releases the close gate; the resident editor continues coherent read-only interaction.
- Final close authorization disposes only the exact ready editor. Failed close settlement releases
  only that attempt's gate, preserves current caret, selection, scroll, and history authority, and
  cannot lift an independent unavailable state. Pending publication retains ordinary exact custody;
  stale settlement cannot release or dispose another attempt or replacement editor.
- Foreground release, worker release, and mount-retirement cleanup use the same exact gate-release
  decision. Their scheduling differs: foreground work cannot wait for storage-held locks, and
  background cleanup remains bounded. The mounted interaction gate, admission reservation, and
  durable flush barrier retain their distinct responsibilities.
- Marker-changing publication streams one immutable root's authenticated pages through typed Syndic
  and Asset contributions and publishes atomically. The app retains bounded pages, cursors, opaque
  proofs, and custody and constructs no cross-domain proof mapping. Marker-unchanged publication
  performs no marker scan.
- Submission starts only after a flush proves one immutable candidate is the current durable draft.
  The app drives bounded materialization and reference preparation and passes opaque typed
  contributions into one atomic acceptance command.
- Submission may use the authenticated unchanged opening relationship. It materializes the durable
  root while fencing the exact captured live candidate and durable selector independently; later
  adoption, selector drift, or replacement invalidates that capture. Final ordinary session disposal
  uses Syndic's exact disposal command, including authenticated opening normalization, and preserves
  the normal disposal receipt and reconciliation obligations.
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
