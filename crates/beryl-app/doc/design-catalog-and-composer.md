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

- Running-thread claim activation separates worker-side source preparation, predecessor draft-save
  qualification and source publication from GUI editor fencing, widget-release capture and promotion.
  Prepared presentation and final publication are sealed to the exact composer service and receipt.
  No source getter or storage operation runs inside the final GUI publication cut. Claim replacement
  precedes predecessor disposal and widget release only after the predecessor save is known
  satisfied. The canonical composer owns a selection-specific saved-checkpoint boundary over the
  ordinary immutable-candidate/frontier publication protocol. It authenticates the exact current
  durable draft and settled publication while retaining the prior host's active binding, candidate,
  history and fenced editor; it does not report ordinary ThreadSwitch disposal as complete.
  The saved proof is bound to the exact host, flush attempt and selection receipt. Failed save,
  cancelled selection or a known uncommitted claim retires only the target and permits the same
  prior editor to resume after exact settlement. Indeterminate save or claim retains both editor
  and operation custody and excludes competing selection until exact reconciliation.
  Once the claim is known committed, canonical predecessor disposal and widget release complete
  before coherent target promotion. Their failures retain the committed target and predecessor
  cleanup custody; they never roll back the claim or resume the old editor. Ordinary close,
  submission and ordinary ThreadSwitch completion retain their own existing semantics.

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

## Detached Read-Only Composer

- Final shutdown uses the
  [detached immutable draft source](../../../doc/systems/beryl-home-storage/design.md#detached-shutdown-read-sources)
  independently of interrupted-Exit recovery. After exact draft readiness, a worker prepares the
  source from that selected candidate/root under its original close fence. Preparation precedes
  placement and Exit session publication; failure disposes the attempt's prepared set and uses
  ordinary draft/coordinator recovery, retaining the readable original resident and a cancelled Exit.
- At final admission the app revalidates the complete prepared set against the original request,
  close tickets, resident identities, candidate roots and settled source operations. It installs
  those sources at one non-yielding GUI cut before retiring live services. All fallible preparation
  precedes installation. The same editor, binding, caret, directed selection, scroll and coherent
  surface remain; no hidden replacement editor or focus transfer is introduced.
- The detached phase keeps the widget enabled and read-only. It does not use protected predecessor
  recovery, which deliberately disables source-dependent interaction. It admits only bounded
  text/object reads, cancellation and settlement, selection, navigation, scrolling and Copy.
  Editing, cut, paste, undo/redo, submission, steering and marker/image actions remain rejected by
  the app as well as the widget. No read operation may fall back to a retired home/service.
- Each detached resident permits one bounded worker flight with exact non-reused request identity.
  Late or foreign completion cannot update another resident or source. In-flight jobs retain only
  the detached source and bounded request/result custody; disposal cancels and joins them before
  releasing their source ownership. A read failure preserves coherent paint and reports failure
  without globally disabling subsequent read-only interaction.
- The local clipboard writer survives service retirement. Copy uses the unchanged bounded widget
  coordinator and configured contiguous clipboard limit, validating completion against the exact
  detached binding rather than live service identity. Preserve the currently supported provenance
  policy; retaining marker asset facts neither invents private-format eligibility nor enables rich
  paste. Marker text remains the existing label and image fallback representation. Failed or
  cancelled copy releases staging and never mutates the draft or writes a partial representation.
- Verify real selection and Copy beyond the resident page after home closure, marker fallback,
  clipboard-limit refusal, mutation rejection, stale completion and exact disposal. Independently
  review source correspondence, callback lifetime and blocked-shutdown integration.

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
- Post-finish build commands use Syndic's opaque prepared-command submission and retained outcome
  flight. The host retains that flight through local finalization, reconciliation and required
  committed cleanup, transporting typed progress and settlement only as the boundary permits.
  It neither substitutes terminal status reads for completion nor retains or reconstructs the
  original edit fragments. A known committed result remains distinguishable from later cleanup or
  availability failure.

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
- Once exact interrupted-Exit resident fencing and mount-resource detachment succeed, acquired and
  restored shell construction custody releases its duplicate selected-service reference only after
  checking the same service identity and settled activation/preparation state. Pending opening or
  abandonment obligations refuse that handoff. Opening, claim and source records remain owned for
  the separate whole-shell recovery boundary; releasing this duplicate performs no storage cleanup
  and does not prove whole-graph retirement.
- After clean interrupted-Exit retirement, retained host facts may be checked through fresh
  same-home replacement candidate access. Each observation authenticates the exact retained live
  checkpoint and durable selector through Syndic's saved-checkpoint boundary. Foreign homes, old
  storage handles and changed or unresolved evidence cannot establish readiness. This read-only,
  uncached observation neither creates a replacement binding nor releases interaction gates or
  authorizes whole-graph publication.
- Reconstructing a clean retired host consumes its retained facts only after that fresh validation
  succeeds. Failure returns those facts intact. The replacement preserves the live candidate,
  durable selector, original activation checkpoint and bounded settlement capacity, while issuing
  a checked successor host identity in the replacement home generation. It opens no editor session
  and writes no draft or history. Its new close gate starts fenced and requires ordinary fresh
  flush qualification; the old close ticket cannot release it. Widget attachment, replacement
  service publication and coherent interaction reopening remain separate obligations.
- Reconstructing the enclosing retired slot additionally validates the selected window in the
  fresh bounded session set, its exact Active paired claim and the replacement asset handle.
  It preserves the activation counter and installs a drained dispatcher with the fresh host.
  The resident owner and close-request generation remain unchanged, but the replacement close
  ticket names the fresh selection; old tickets cannot release it. Failure returns the original
  retirement facts intact. This creates no durable records and does not publish services, attach
  the widget or establish whole-session recovery readiness.
- Recovery service reconstruction takes its home reference from the same private recovery
  candidate used to reconstruct the retired slot. It installs the fresh exact close ticket in
  the service gate before returning the service. Candidate-access or slot-validation failure
  returns the original retirement facts; success leaves ordinary interaction fenced and starts
  no native-lineage worker. Construction performs no writes and neither attaches the resident
  widget nor publishes the replacement graph.
- Reconstruction also returns the exact fixed-size session window record from candidate claim
  validation. The candidate editor source retains that record and compares it during source
  validation and page servicing. Successful owner attachment returns the record with the candidate
  and fresh close ticket for shell rebinding, without GUI storage access. These immutable facts
  carry no service or publication authority and do not permit interaction release.
- Retained resident retirement facts may be transferred to recovery work only after rechecking
  the exact recovery fence, detached mount resources and live resident quiescence. The transfer
  moves the facts once, retains the presentation snapshot and leaves interaction closed. While
  the facts are outside the resident, its retirement-readiness observation is false. Refused
  reconstruction may return the original facts through the same exact retirement acceptance;
  stale attempts preserve custody. This GUI handoff performs no storage access or rebinding.
- Preserved-resident attachment uses the widget's checked prepublication adoption boundary.
  Before deriving a successor seed, the app freshly authenticates the retained clean checkpoint,
  selector, candidate and history correspondence through the same-home candidate and validates
  paired claims. Equal offsets, extents or revisions alone are insufficient. Only binding and
  opaque source/history authority are replaced; the captured caret, directed selection, inline
  gaps and exact scroll continuation are preserved. Fresh marker/presentation adapters belong to
  that same candidate generation and must satisfy the captured presentation and layout inputs.
- Recovery owns one bounded preparation flight per preserved resident, tagged by the exact ordinary
  home attempt or cancelled lifecycle request, resident identity, predecessor close ticket and fresh
  candidate generation. These ownership forms remain distinct under
  [shell lifecycle authority](design-shell-lifecycle.md#ordinary-running-home-recovery-ownership);
  an ordinary attempt cannot authorize Exit settlement or a Running-session resume. Its
  fresh service dispatches prepublication reads off the GUI thread and returns exact keyed results
  to explicitly scheduled bounded GUI realization steps. Ordinary resident pumping stays fenced;
  assigning a service does not authorize old callbacks or requests. Worker lifetime, cancellation
  and cleanup remain owned until their actual completion, including abandoned GUI delivery.
- The app retains the widget fence, old coherent paint and focus identity throughout preparation.
  It reserves the widget's combined old/new capacity envelope before realization and checks the
  exact flight and live predecessor again before adoption. Changed source/history, environment,
  claim, window or attempt/request evidence refuses attachment without resetting the editor.
  There is no direct-rebind/import fallback, hidden replacement widget or automatic focus transfer. Cancellation
  drains candidate effects and cleanup without releasing interaction or replaying a lifecycle request.
- Fresh service and adapter custody remains recovery-owned until coherent widget adoption and
  exact resident association succeed together in one GUI completion with no intervening callback.
  All fallible validation and admission precede that publication; refused adoption preserves the
  predecessor and returns or retains the fresh resources for explicit cleanup. The app does not
  regain failed-generation resources or treat successful widget adoption as whole-graph readiness.
  Every resident stays fenced until session settlement, complete fresh graph bindings and exact
  draft/work settlement authorize the separate atomic reopening boundary.
- Verify exact fresh attachment with unchanged checkpoint/history and resident identity, stale
  worker completion, candidate failure, environment/capacity refusal, cancellation and complete
  cleanup. Independently review the source correspondence and lifecycle composition; widget-only
  evidence cannot establish service-worker retirement or whole-home recovery.
- Failed-home resident recovery has separate move-only retirement custody for a selected active
  editor whose home has failed, including an unflushed candidate. It installs the exact resident
  input fence, retains widget protection and bounded selection/restoration facts, drains admitted
  adapter work and explicitly retires old services. The custody retains the exact live candidate,
  activation/checkpoint/root/history, previous durable selector and original publication or flush
  outcome/reconciliation. It retains no old home/service capability after completed retirement.
- Healthy nonfinal native-close detachment retains distinct reversible custody of the settled
  resident and its quiescent services through native settlement. It preserves the same protected
  widget, checkpoint/history and presentation while detached reads serve the surviving surface.
  After proven native failure, authenticated restored membership and the renewed exact claim
  permit rebinding to the same healthy generation before interaction release. Old claim authority
  is not revived. Successful destruction disposes custody once; uncertain native settlement cannot
  release it. A subsequent home failure transfers resident facts through the existing failed-home
  capture/retirement boundary, retaining both original and restoration outcomes. Drain detached
  reads and restore capture-compatible fenced custody before that transfer; do not feed a Detached
  resident into ordinary failed-resident admission or briefly reopen interaction. This route does
  not weaken clean saved-checkpoint or failed-home admission validators.
- Fresh same-home candidate qualification authenticates all retained candidate/session provenance,
  State membership and paired claim before reconstruction. Original publication uncertainty must
  reconcile first. Retain an older autosave's captured checkpoint separately from newer live
  edits; its proven commit cannot mark those newer edits saved or be repeated. An authenticated
  saved candidate needs no write; proving that the retained unsaved checkpoint has no committed
  or unresolved publication permits its exact captured publication through candidate access with independent
  result/reconciliation custody. Conflict, unresolved work, corruption or terminal uncertainty
  preserves the resident and cannot establish readiness.
- The candidate-save boundary accepts existing typed marker and Asset publication evidence;
  the recovery caller prepares it through fresh bounded candidate services before invoking a
  marker-changing save. Retirement retains the source facts needed for this preparation, without
  retaining old service capabilities. Missing or mismatched evidence preserves failed-resident
  custody. Ordinary-command recovery owns this preparation routing and complete graph integration.
- The supplied-proof failed-resident primitive must refuse before dropping adapters or services
  while any unfinished marker flight or unretained terminal marker authority remains. Its host
  boundary leaves caller-owned marker-service custody with that caller; the caller preserves it
  before graph retirement. An empty flight count alone cannot prove settlement when collision
  removed a service flight but left original staging authority in the host publication lane.
- Recovery preparation preserves unfinished process-owned seal flights before old graph retirement,
  including original typed staging authority and command/reconciliation custody. It drives fresh
  candidate seal and reference-set preparation under the existing single injected service's
  finite capacity and bounded page protocol. Missing capture or uncertain completion blocks
  retirement/publication; dropping a non-driving flight is not recovery settlement. Ordinary
  Healthy/generation guards remain unchanged.
- This route reconstructs the same editor session and live history after independently proven
  save; it does not open a fresh session or import another unpublished checkpoint. The existing
  checked widget adoption, capacity reservation, stale-completion and cleanup rules apply with
  the newly authenticated saved checkpoint. Caret, directed selection, inline gaps, scroll and
  history remain preserved. Failed-generation custody is distinct from clean retired-host facts;
  clean retirement and saved-checkpoint validators are unchanged. Coherent process recovery,
  rather than a local successful save, releases interaction.
- Verify failed-home admission, dirty and already-saved candidates, original noncommit/commit/
  indeterminate outcomes, fresh save failures and reconciliation, stale/foreign/colliding facts,
  cancellation and retained custody, renewed claims and unchanged editor/input/history state.
  Independently review source provenance, persistence outcomes and native lifecycle integration.
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
- Exact durable submission acceptance hands work to the process scheduler before successor-editor
  activation or presentation completion. Committed, reconciled exact and already-accepted outcomes
  share this handoff; cancellation, noncommit, collision and unresolved outcomes cannot signal it.
  The handoff retains only the originating home/service generation's bounded wake capability and
  rejects another home or generation. Retirement cannot redirect it into a replacement service.
  Idle acceptance wakes ordinary pending execution; accepted input wakes both steering and
  accepted-next inspection, whose existing durable gates determine current eligibility. These
  coalesced notifications grant no dispatch, retry or duplicated provider-effect authority and do
  not broaden the separate idle-maintenance wake. Mounted and unmounted submission settlement use
  the same handoff independently of editor and view lifetime.
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
