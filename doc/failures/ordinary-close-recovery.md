# Ordinary Close After Committed Session Removal

## Readiness Finding

On 2026-10-02, phase 595 inspection found that the accepted interrupted-Exit recovery cannot
directly cover ordinary window close after a committed removal reports a later storage failure.
No production changes were made during this inspection.

`RemoveSessionWindow::contribute` in
`crates/beryl-state/src/session/mutation/window.rs` deletes the window record and its paired claim
indexes, then publishes the new session header. Home command outcomes explicitly include committed
with later failure and indeterminate outcomes subsequently proven exact-new.

The [ordinary-close feature](../features/main-windows/design.md#ordinary-window-close) requires
failed or unproven draft/session obligations to preserve the open window, resident and claim, with
fresh activation after coherent recovery. The
[home-failure feature](../features/beryl-home/design.md#persistent-store-failure-during-a-session)
also prohibits completing a close when its restore-set write fails and exposes no operation-level
rollback or resubmission command.

In contrast, [interrupted-Exit recovery](../systems/backend-runtime/design.md#interrupted-exit-during-same-home-recovery)
preserves existing committed records and claims. Its
[`ResumeSessionAfterExit` transition](../../crates/beryl-state/doc/design-runtime-session.md#resume-a-committed-exit-session)
changes only the header from OrderlyExit to Running, requires 1–256 existing window references,
and explicitly performs no claim restoration. Ordinary removal leaves Running intent and fewer
records; final ordinary close leaves none. The app's `exit_session/validation.rs` likewise requires
the complete original member count. These are intentional dedicated-Exit contracts, not defects
to bypass with an empty placement list or relaxed validation.

Independent read-only inspection confirmed the gap. `CreateClaimedWindow` is not an existing
recovery substitute: it initializes record and claim revisions, updates fallback and does not
authenticate the prior removal. Repeating `RemoveSessionWindow` also cannot settle a fresh close
after deletion because its preparation requires the original member, window and expected claim.

## Accepted Direction

The Operator approved exact same-window recovery on 2026-10-02. Reusing startup restoration,
treating missing records as success, or
silently completing the cancelled close does not implement the current preserved-window contract.

The accepted direction defines a separate same-home recovery transition for the
exact removed window and paired claim. Retain bounded immutable original facts and exact removal
outcome outside the replaceable graph; validate the exact committed post-removal state through
fresh candidate authority before restoring that same live window's durable membership. Give the
transition its own revision checks, ordinary outcome/reconciliation custody and duplicate guards.
Preserve native identity, resident content and placement; release interaction only after complete
coherent recovery, and require fresh close activation. Never choose a substitute thread/window or
overwrite conflicting claims. Resolve record/claim revision continuity explicitly in State authority.

The owning feature, backend-runtime system, storage system, State and app contracts now define
this transition. Phase 704 establishes its typed persistence boundary before phase 595 composes
ordinary routing and complete recovery. It applies to nonfinal and final ordinary close.
Dedicated Exit remains a distinct accepted path.

## Typed Persistence Acceptance

Phase 704 passed independent semantic and persistence review on 2026-10-02. State now captures
bounded immutable canonical-home/header/window/active-claim facts, authenticates the removal at
the writer, classifies original/removed/recovered/collision through fresh candidate access, and
restores only that exact member with advanced window and paired-claim revisions. The recovery
contribution has its own ordinary outcome and reconciliation closure. No schema or durable journal
was added. Home-store supplies only a borrowed canonical-path metadata accessor.

- Local `cargo +stable --config .cargo/local.toml check --locked -p beryl-state --tests --features test-faults`
  passed.
- The unchanged related targets `exit_session`, `recovery`, `session` and `session_schema` passed
  all 22 cases in nextest run `eb6960cb-f4eb-49fb-b529-419fba3d1e2d`. That run also contained failed
  new test fixtures; only those fixtures changed afterward.
- The final `window_removal_recovery` target passed all seven cases in 6.791 seconds, run
  `59fa12e0-b28a-4ed3-b390-ba6ef53a445e`. Both commands used `--locked -p beryl-state --features test-faults`,
  explicit test targets, `--test-threads 1 --no-fail-fast`, and the process-local Windows error-mode
  wrapper. Together these results cover final/nonfinal/threadless and capacity sets, identity,
  placement/fallback, renewed claims, stale/foreign/conflicting/duplicate refusal, revision
  exhaustion, candidate reopening, and independent removal/restoration fault outcomes.
- A clean isolated checkout without `.cargo/local.toml` passed locked metadata and
  `cargo +stable check -p beryl-state --tests --features test-faults --locked` in 9.93 seconds.
  All nine final source/test copies matched SHA-256 hashes; the tracked lockfile stayed unchanged.
  Validation used one job, LLVM, debug disabled and incremental compilation disabled.
- Scoped formatting and diff checks passed. The independent reviewer accepted the complete typed
  boundary and final test fixtures without unresolved findings. The isolated checkout was removed
  after exact-path and reparse-point checks; no verification worker remains active.

Phase 595 must still retain the original command outcome and cancelled intent, compose this
transition with full service replacement and renewed resident binding, and mount ordinary native
close/Exit. This acceptance grants no GUI close, interaction release or automatic retry authority.

## Ordinary Command Integration Findings

During phase 595 on 2026-10-02, focused app-session tests exposed a distinction between reading
an existing reconciliation result and explicitly retrying a failed reconciliation attempt.
Reopening the candidate does not clear a cached failed flight. The app must use the existing
`retry_reconciliation` operation after a failed result, retaining the exact original handle and
independent removal/restoration outcomes. The corrected eight-case app-session group passed.

Independent production review also rejected routing every unsuccessful removal into failed-home
recovery. A revision conflict or admission refusal can prove noncommit while the home remains
healthy. Failed-home recovery cannot reopen that healthy home, so this route could retain the
cancelled close's gates indefinitely. Such a result needs qualified coherent release without an
inverse write or service replacement. Persistent home failure still requires whole-graph recovery;
committed or unproven removal retains its separate exact restoration/reconciliation protocol.
The correction and native verification remain in the active phase; this is not phase acceptance.

Mounted native verification also caught a success/failure routing error: the extended failure
reporter treated any retained ordinary-close session as a cancelled close, including successful
`SessionReady`. Run `bd1e7d72-ab3d-49d0-b24c-c222e2c1ed84` showed successful removal readiness
immediately followed by `The reported failed Exit is cancelled`, with healthy home and retained
shutdown custody. The ordinary-close recovery arm now additionally requires an error outcome.
Retained session evidence alone does not authorize reporting failure or cancelling a successful close.

Native content-fixture verification exposed a separate startup failure-cleanup stack problem.
First-chance debugger evidence placed the overflow in restored-composer disposal, through
`InitialComposerCandidate::drive_retirement` and fresh-thread abandonment to Syndic session-head
decoding. The disposal caller reserved roughly 399 KiB and unboxed large retained custody on the
fixed Windows worker stack. Keeping that custody boxed while retirement advances and separating
the disposal stages removed the observed overflow without changing the worker stack budget or
discarding failure custody. The original qualification failure came from the shared fixture
requesting eight pages even for a three-page draft, producing an invalid marker range beyond its
extent. Deriving the bounded page count from that extent corrected the fixture; the unchanged
small-draft cancellation case then passed. This correction alone does not accept the ordinary
command boundary.

New Window verification found another fixed-worker-stack overflow during pristine-thread
qualification in the first background creation advance. Initial boxing of the work/result handoff
and in-place advancement did not clear it. Preserve this distinction when evaluating the next
correction: focused creation publication and subsequent native command delivery must pass before
the broader suite can supply acceptance evidence. Further debugger evidence found roughly 191 KiB
of outcome assembly, 230 KiB of state extraction and 134 KiB of activated preparation live above
history authentication. The next correction separates those stages and retains the internal
prepared step boxed, so deep validation returns before assembling its result. Focused run
`231d2f55-ead6-4b4c-94ff-10dfa4ad21f3` crossed the previous failure boundary and published the new
process-owned window on the unchanged stack. Its subsequent native-close deadline failed;
publication alone does not accept created-window lifecycle behavior.

Broader run `920ea987-f0e4-4005-a484-f7a27365d9cd` passed 19 of 28 cases, including all ten shared
composer-activation cases, and failed nine native close/recovery cases. The dirty case hung on the
GUI's composer-slot mutex and required termination of its exact test child. A fresh first-chance
debugger capture proved a worker-stack overflow during actual edit staging, before the ordinary
close failure hook, while that mutex was held. The failed worker stranded the guard; treating the
foreground wait as a logical lock cycle would target the wrong cause. The bounded app correction
separates dispatch-completion allocation from deep staging. Subsequent private transfer boxing and
frame separation progressed genuine editing to build advancement. A fresh first-chance capture
in `.tmp/dirty-stack-correction/current-worker-stacks.txt` places the remaining overflow in build
decoding below `authenticated_build_from_store`, `prepare_draft_piece_build_advance` and
`prepare_staged_draft_piece_advance`; their observed frames total 577,600 bytes before decoding
and the outer app worker. Large private by-value preparation records remain the next correction
boundary. This evidence does not justify increasing the native worker stack or changing codecs,
validation order or publication semantics. Runtime qualification and remaining close/recovery
failures are still pending. Subsequent native runs use an external
per-case deadline as well as their GUI deadline, since a blocked GUI cannot enforce its own timer.

The subsequent seven-path private boxing correction passed 34 affected staging/outcome cases in
107.274 seconds (`cc755f83-ec1c-4be7-9634-f12443cfc6c4`). Independent review found no changed
record bytes, public signatures, validation/admission order, outcome custody or unbounded retention.
Remaining record unboxing and compiler-generated construction temporaries are diagnostic targets
if the rebuilt native case still overflows; source review and these tests do not prove native stack
safety. Evidence remains in `.tmp/dirty-stack-correction/advance-staging-regressions.txt`.

Rebuilt native run `4fd05208-05d9-417b-be35-6f960ab46fda` still overflowed and timed out at
60.048 seconds. The next exact first-chance trace in
`.tmp/dirty-stack-correction/advance-worker-stacks.txt` gets beyond advancement into durable-window
preparation: `prepare_next_durable_draft_piece_window` holds 417,424 bytes and its caller
`prepare_staged_draft_piece_window` holds 245,744 bytes while staging-head decoding exhausts the
remaining stack. Continue bounded private allocation/frame separation at this proven boundary;
retain codec and validation semantics. Both exactly identified test/debugger fixture homes were
removed after path and reparse checks; the debugger and test child ended.

The durable-window correction passed the same 34 affected cases in 108.390 seconds
(`cbb4f4ee-fbe0-4578-99f6-ff07eb25376d`). Independent six-path review found no semantic blocker;
all reviewed hashes matched after verification. Its private prepared records use bounded heap
ownership, and deep acquisition runs outside later result construction frames. Native proof is
still pending. The separate app session/clean-native run `af622868-5d92-494e-9dad-b1b67f22dfd7`
passed two of four cases: exact healthy restoration and removal-noncommit handling passed, while
the native survivor case reached its fixture deadline and a crossover fixture expected an error
from a fault that could be reconciled successfully. Neither failure is evidence of another stack
overflow; app runtime diagnosis and fixture correction remain required.

Rebuilt dirty run `1be00e8d-57e4-4568-ae9c-d7a7acb37326` aborted after 20.700 seconds when
the edit-settlement fixture deadline expired. It logged no worker stack overflow. Logical extent
remained zero with no reported admission/dispatch error and no active flight; the native callback
then aborted because the fixture panic could not unwind. This does not establish successful native
editing or justify another storage boxing change. Investigate real input/dispatch settlement;
retain the unchanged-stack requirement. Log: `.tmp/dirty-stack-correction/window-dirty-native.txt`.
The exact aborted fixture home was removed after checked process exit, path and reparse validation.

A subsequent native diagnostic run beginning `2451828a` did report worker overflow and timed out
at 60.054 seconds. Fresh first-chance evidence in
`.tmp/dirty-stack-correction/window-worker-stacks.txt` locates receipt decoding below
`sequence_advance::prepare` (329,056 bytes), with its ordinary and staged preparation callers
holding another 97,616 and 116,560 bytes. The earlier no-overflow log was inconclusive, not native
acceptance. Continue the private frame correction at this measured sequence-preparation boundary.
The same app diagnostic run identifies the clean restored-window failure as reversible mount
custody validation after both durable commands committed; correct that app binding independently.
The exact failed-run and debugger fixture homes were removed after checked process exit and
path/reparse validation.

Run `ea90cd04-497e-406a-b99d-679ef25d73b1` passed all three healthy app session cases and
the two-real-marker-resident capacity-one regression. Its native case exposed a new restoration
check that required optional native-lineage recovery control to exist, although healthy mounts
initialize that control as absent and retain it optionally. Removing only that invalid requirement
preserves the captured value and the exact service, selection, editor and adapter checks.
Targeted review confirmed the correction against mount construction and detachment custody.
Native run `69a0f320-426b-4307-869c-be93875de3aa` then passed in 3.497 seconds: same surviving
window/editor/history, healthy graph and background-work permit, renewed claim release, fresh
successful close and subsequent Exit. Logs remain in `.tmp/ordinary-recovery-integration`.

The sequence-preparation correction passed 34 cases in 112.355 seconds
(`0835d9a2-f9c7-4c9b-9824-26066d05910c`) and independent semantic review with matching hashes.
Native run `c27a4231-6048-410e-aef3-4a741c8f3ff6` still overflowed and timed out at 60.078 seconds.
Fresh `.tmp/dirty-stack-correction/sequence-worker-stacks.txt` evidence progresses into the writer's
staged settlement: build decoding exhausts the stack below active-custody authentication
(128,416-byte frame), settlement preparation (88,416 bytes) and staged capture settlement
(147,712 bytes). Preserve all writer-time authentication and outcome checks while reducing these
private temporaries. This is not evidence for revisiting the already corrected sequence frame or
enlarging the native stack. Exact failed-run/debugger fixture homes were checked and removed.

Combined native run `5f8f7376-a575-4a84-b3c4-16b5f64de107` passed three of 12 in 122.419 seconds:
healthy conflict, healthy Survived restoration and app Unresolved custody. Nine failed-home routes
still failed. Some report stale recovery state; others log worker overflow. Fresh first-chance
`.tmp/ordinary-recovery-integration/recovery-worker-stacks.txt` evidence locates the latter in app
candidate construction: `MainWindowFailedResidentCandidateSource::new` holds 428,928 bytes,
`PreparedRecoveryServiceGraph::failed_resident_source` 158,304 bytes and its preparation worker
77,584 bytes above ordinary history-frontier decoding. Reduce those private candidate temporaries;
do not change the decoder or infer that stale-state failures share this cause. The exact debugger
fixture was checked and removed after its process ended. The source and native boundary remains
unaccepted pending these corrections, combined verification and canonical acceptance.

Writer-settlement changes passed 38 regressions in 122.306 seconds
(`57ea420c-e2eb-4dbc-baee-81cb71330e78`) and independent semantic review. App candidate correction
heap-owns only the private failed host in `MainWindowFailedComposerRetirement`, preserving all
retained access, outcome and reconstruction operations; targeted review found no semantic change.
Native run `f39a7121-dc73-4d1f-86df-c129449f7f62` passed the storage-failure crossover and
Unresolved consumer cases with no reported overflow. Genuine dirty editing now reaches the close
flush wait but does not settle, and pre-draft recovery still reports stale state. Two of four cases
passed in 31.857 seconds. Continue those distinct app diagnoses; do not infer full stack or phase
acceptance from this partial run. Log: `.tmp/ordinary-recovery-integration/boxed-retirement-native.txt`.

Diagnostic run beginning `8da449cf` confirmed worker overflow during dirty flush after editing,
before a session outcome exists. The first debugger replay hit an edit fixture deadline without
overflow; the second `.tmp/dirty-stack-correction/flush-worker-overflow.txt` captured first-chance
overflow in history-transition decoding below publication. Observed enclosing frames include
151,408 bytes in storage publication and 102,624 plus 166,704 bytes in app publication/capture.
Map source carefully because adjacent symbols can share optimized addresses; retain all history
authentication and reduce only proven private temporary allocations. Separately, the stale route
refuses the initial failed-service-graph retirement validator before candidate preparation. All
three exact dirty/debugger fixture homes were checked and removed after process termination.

Publication/history changes passed 38 cases in 101.586 seconds
(`57871fd3-8d72-400e-9a81-bb4e4a6fdc0c`) and independent review. App publication boxing also
passed independent review, including its two failed-resident consumers. Native diagnostic run
`e1120926-603f-469b-83f7-094b46eaa9cc` failed all three selected cases in 37.282 seconds, but dirty
editing and flush preparation progressed to a truthful reconciliation-collision failure before
overflowing during failed-home recovery. Fresh `.tmp/dirty-stack-correction/dirty-recovery-overflow.txt`
shows candidate saved-state validation under app frames of 132,656 and 136,480 bytes plus process
preparation of 50,848 bytes above settlement decoding. Continue that private app allocation
correction; the first debugger replay only reached an edit fixture deadline. Exact failed-run and
both debugger fixture homes were checked and removed.

## Healthy Pre-Native Indeterminate Removal

The same diagnostic run proves both stale routes reach failed-service-graph retirement with
Healthy generation 1, a published graph and no failed-close or retirement custody. The pre-draft
test seam requires an actual state-dependent read to observe its retained maintenance failure;
correct that fixture rather than weakening the Failed validator. The indeterminate-removal case
is a separate production lifecycle gap: an uncertain command outcome need not fail the home, but
the current ordinary command route attempts failed-home recovery immediately.

The existing feature requires preservation of the same window, claim and editor after a failed
close. The app's approved healthy-restoration route currently requires GPUI's settled surviving
native-attempt proof; it did not grant pre-native reopening authority. The following extension was
proposed at the checkpoint and subsequently approved in the continuation recorded below:

- Retain exact request, gated shell/resident and original removal outcome before any native
  destruction admission. A distinct pre-native proof must establish that this request never began
  destruction; absence of a handle or an arbitrary error is insufficient.
- Reconcile the original command through ordinary admission while the original home stays Healthy.
  Proven noncommit validates the exact original state without an inverse write. Proven commit
  restores the exact member and claim through the existing State route, retaining separate
  restoration outcome custody. Uncertainty or conflict retains fenced custody.
- Authenticate and rebind the same retained editor and renewed claim before coherent gate release.
  Preserve the healthy graph and background work. Reported failure permanently cancels the close;
  another close requires fresh activation.
- If storage becomes Failed, transfer both retained outcomes and resident custody through the
  established failed-home route. Never manufacture home failure or begin/retry native destruction
  to obtain restoration authority. Verify both outcome branches, repeated/stale completion,
  crossover and fresh close, with independent lifecycle review.

This sub-boundary was paused under the repository instruction to stop when a planned step cannot
technically work. The Operator subsequently approved the recommended extension with “go with
recommended”. Feature and app lifecycle authority now define its exact pre-native proof and
ordinary reconciliation/restoration route. Implementation and acceptance resume under phase 595.

The pre-draft observation correction passed a native case in run
`de124854-f643-436c-a56b-b862de5a18ea`; its dirty case still overflowed. Fresh
`.tmp/dirty-stack-correction/candidate-saved-overflow.txt` showed candidate publication progressed
beyond the first saved-state guard, with 136,352 bytes of app preparation above settlement decode
frames of 167,296 and 62,656 bytes. The app now privately boxes retained source custody and separates
capture, preparation and command execution without changing their order. Independent review
accepted that delta. Exact known failed-run and debugger homes were checked and removed.

The decoder addresses map to `decode_settlement_closure`, not the adjacent encoder symbol names
shown by the debugger. Splitting its private committed/noncommit reads and final assembly with
fixed temporary heap ownership preserves field/tag/error precedence and the unchanged outer
finish/re-encode/digest/closure validation. Independent review accepted both codec paths; 60 focused
cases passed in 121.797 seconds (`ca55590a-f276-4199-86d0-4206e8d12d1f`), with one previously
qualified long continuation case excluded. Evidence:
`.tmp/dirty-stack-correction/settlement-decoder-regressions.txt`. Native qualification remains pending.

Native run `9aa6e212-6387-4b4f-88e7-42de08f0ef22` completed recovery without reported overflow,
then failed a test requiring the dirty Session history key to equal the saved Publication key.
The existing publication promotion changes that key while preserving its frontier revision and
undo/redo availability; same widget, selection, member identities, content and root assertions had
passed. The proposed replacement compared exact history metadata and captured transition heads
using existing read-only publication preparation. That API works for the initial dirty checkpoint
but refuses the already-saved publication, so it is not a valid post-recovery inspector.

The final authorized close/toolbar run `2923fe69-e71b-4b19-9d77-fcebb3516e8d` failed both tests
at that helper in 6.610 seconds after recovery settled, with no reported worker overflow. Preserve
the meaningful history and fresh-close checks; replace the invalid helper through supported
authenticated evidence or exercised history behavior, not a weaker key or availability assertion.
No production change was made for this test failure. Work paused at that checkpoint with the
separate healthy pre-native policy approval pending. That approval is now recorded above;
combined/canonical verification and phase acceptance remain outstanding.

The corrected inspector selects the original operation ID from a saved Publication frontier.
Existing occupied-receipt preparation authenticates that exact saved frontier; the helper asserts
the returned reference equals the current binding. Before recovery, fresh read-only preparation
provides the corresponding publication snapshot for comparison. This preserves full journal,
undo/redo, retention-floor and accounting assertions despite the legitimate identity promotion.
Independent source review accepted the fixture correction; native verification remains pending.

The common nonfinal-close failure was subsequently localized to detached-install qualification:
the shell's ordinary-close flag was set, but the exact resident's interaction fence remained open.
The existing validator correctly refused that unsettled resident. Nonfinal close now installs the
existing interaction fence on the invoking shell and resident before preparation, and releases
both only after qualified draft cleanup. The nonfinal path still preserves unrelated background
work. Separately, healthy-conflict feedback was rejected because notice admission ran inside
`window.update` and attempted another update of the same window. Obtaining ingress and identity
inside that update, then admitting outside it, preserves the commandless failure report.
Focused run `d43d4258-7570-4cd4-9171-6e57b97d4eca` passed both cases in 4.059 seconds.

Independent review found that the resident fence exposed an existing presentation shortcut:
the toolbar treated every shutdown interaction fence as an admitted application-wide barrier.
Nonfinal close must retain its fence while showing the closest window-close gate; the feature's
`Exiting…` loading presentation remains reserved for admitted barriers. That GUI correction and
the remaining native recovery cases still require qualification before phase acceptance.

## Failed-Home Resident Recovery Readiness

Phase 595 encountered this blocker on 2026-10-02; its current implementation remains unaccepted. Thirteen of
21 mounted cases have passing per-case evidence. Created-window close and Exit passed in run
`4fb36d4f-6d98-4323-b1b2-fb96081af62a`, but seven interrupted-recovery cases failed. The corrected
nonfinal presentation passed in run `603f23c4-2637-490a-9a88-4f64ea148775`. That run's recovery
diagnostic identifies `Unavailable(conversation composer close home is unavailable)`.

Independent bounded source review confirmed two missing boundaries:

- `add_recovery_window` invokes ordinary close admission after home failure. The service close
  gate requires a healthy home, so an uncaptured surviving resident cannot enter recovery.
- Mount recovery fencing requires `Ready`; host retirement requires exact successful flush
  readiness; reconstruction authenticates only a saved checkpoint. None represents an active
  unsaved candidate after failed flush. Pending-activation reconciliation handles a different
  lifecycle and supplies no substitute.

The [app composer authority](../../crates/beryl-app/doc/design-catalog-and-composer.md) defines
clean retirement and clean-checkpoint attachment. Syndic permits borrowed candidate root/history
reads and saved-checkpoint validation, but publication capture/preparation currently requires
ordinary healthy `HomeStore` access. Accepting a failed home in the ordinary gate or relabeling
dirty custody as clean would bypass these guarantees. The Operator approved the prerequisite
on 2026-10-02. Its target contracts now reside in the owning recovery, app and Syndic authorities;
phase 705 establishes the capability before phase 595 resumes ordinary-command integration.
This evidence record does not own architecture.

The recommended prerequisite is a separate failed-home resident recovery protocol:

1. Fence the exact failed-generation resident and drain admitted adapter work. Retain bounded
   resident/input-protection facts, selection binding, exact live candidate/root/history,
   previous durable selector and original flush/publication outcomes and reconciliation handles.
   Preserve native identity, claims and reservations; retire old service references explicitly.
2. Through fresh same-home candidate access, authenticate canonical home identity, fresh
   generation, restored or surviving State membership/claim, and the exact unpublished session,
   candidate/root/history. Reconcile the original publication outcome before admitting any write.
3. If the captured checkpoint is already saved, validate it. If noncommit is proven and it remains
   unsaved, use explicitly scoped candidate-access publication capture/preparation/submission with the same
   typed source, marker/Asset qualifications and publication command semantics. Retain the new
   result and reconciliation independently; uncertainty never establishes successful save.
4. Reconstruct and attach the same protected editor only after authenticated save and complete
   fresh graph/resident binding. Release interaction coherently and require fresh close activation.

This requires distinct app failed-resident capture/retirement and reconstruction custody, plus
Syndic candidate-access counterparts for publication source capture, preparation and submission, and exact
unpublished-session qualification. It must share existing publication algorithms, preserve
bounded ownership and outcome handling, and leave normal healthy close, clean interrupted-Exit
retirement, saved-checkpoint validators and final teardown proofs unchanged. Update the owning
app, Syndic and recovery contracts before creating the prerequisite implementation phase.

Dirty editing has an additional unresolved stack issue. The earlier responsive debugger run was
an edit rejected while the widget was noninteractive; it did not prove the dispatch allocation
correction. The corrected fixture waits for an enabled, current, quiescent editor before typing.
Run `a61c87e0-0594-475d-9a0c-d4f617a35535` passed five existing dispatch/presentation regressions,
but genuine editing overflowed the WinRT worker stack and stranded the composer mutex; its external
60-second deadline terminated the exact child. No actual failed-dirty-flush recovery runtime
evidence exists yet. Keep stack qualification separate from the missing recovery protocol.

All production/test edits and bounded debugger/native logs are preserved. Production tracing was
removed, no phase commit was made, and no owned Cargo, native test or debugger process remained
active at the worker handoff. Final combined acceptance remains outstanding.

The preserved checkpoint passed local and isolated canonical
`cargo +stable check -p beryl-app --features test-faults,gpui/windows-manifest --all-targets --locked`
on 2026-10-02 (local mode additionally selected `.cargo/local.toml`). Canonical locked metadata
passed without local overrides; all 81 owned source/test copies matched final SHA-256 hashes, and
the tracked lockfile remained unchanged. Both checks used one job, LLVM, debug disabled and
incremental compilation disabled. The isolated checkout was removed after exact-path/reparse
checks; 48 logged expired native/debugger fixture homes were also removed. Bounded logs remain in
the ignored task evidence directories; unidentified homes remain preserved as recorded in
`ENV.md`. Compilation does not resolve the missing recovery protocol or dirty-edit stack failure.

## Protected Successor Admission Correction

Independent phase 705 review rejected releasing the predecessor protection and allocating its
successor after checked widget adoption. A valid final protection generation makes allocation
fail after the widget has already changed, violating the all-fallible-before-publication contract.
The corrected owned-toolkit operation qualifies the successor generation and exact restoration
seed first, shares ordinary checked adoption, and installs the prepared protection infallibly.
The app must use its returned opaque cut rather than perform a second protection allocation.

The same review required exact retained predecessor selection/checkpoint and restoration matching
before pairing the fresh source with the protected widget; equal home/window and positions alone
could pair another valid editor session. Both corrections preserve existing clean admission and
the ordinary toolkit adoption boundary. Verification and final prerequisite acceptance remain
separate gates.

The toolkit correction passed 45 focused protection/prepublication tests locally in
`f3fd22c1-99c8-4303-9deb-066c1a5ddc98` and against isolated canonical dependencies in
`d628cb83-4493-43d5-8ffe-bb89aa788610`. Local and isolated all-target checks and independent
semantic/resource review passed. Accepted text-input revision `803126e` and Settings pin-only
alignment `af66f41` were pushed; Settings locked metadata and its isolated all-target check passed.
Beryl's updated locked metadata passed in both modes and its canonical dependency tree contains
one text-input package. The focused local app check and analyzer restart passed. This accepts
the widget prerequisite, while resident integration tests and complete phase 705 acceptance remain
outstanding.

Review also rejected dropping publication adapters before checking original seal custody. The
host's Sealing/Releasing refusal does not cover a terminal collision: the marker service removes
that flight, but the publication lane can still retain its original staging authority without a
prepared publication. Therefore a zero flight count alone cannot authorize retirement. Refuse
before detach/drop while either kind of original custody remains; the next candidate-preparation
boundary must explicitly retain and settle it rather than reconstruct it from identifiers.

The bounded audit identified phase 706 as a separate prerequisite: candidate seal begin/advance/
status/terminal preparation and authenticated source/cursor readers, State Building/completion
reads with the original staging capability, and retained process seal-flight outcomes. Existing
typed atomic contributions suffice; no schema or publication-algorithm blocker was found. Phase
705 consumes supplied real typed evidence and must retain/refuse unfinished original seal work;
it does not accept full fresh marker preparation or ordinary-command recovery mounting.

## Failed-Resident Capability Acceptance

On 2026-10-02, phase 705 passed independent persistence/lifecycle completion review and isolated
canonical verification. The accepted primitive captures and retires a failed-home resident,
qualifies its exact original selection and checkpoint through fresh candidate access, publishes
with supplied real typed Asset/marker proofs, and reconstructs the same protected editor/history.
Immutable original publication custody remains separate from recovery outcomes and the mutable
qualified checkpoint. An older autosave may advance session generation without saving newer
resident edits; storage-owned correspondence proves that transition without weakening saved proof.
Only proven noncommit permits a new save attempt. Uncertainty retains exact reconciliation custody.

State's 16 candidate-read/asset tests passed in `7e55daf8-a2e7-42cd-865b-c55c69c95c75`.
Syndic's 41 retained-publication tests passed in `a1ee48f1-071f-416b-9473-775999086186`, with the
reopened-candidate retry regression also passing in `3480cef4-919c-4917-a711-502dd9985c94`.
The app's 17 local recovery cases passed in `e3b41768-4c9e-48fe-baad-bd27aa3e3de5`; the combined
older-autosave/newer-save case passed all four permutations in `9d8c7dda-8120-45fd-9803-aebb16e5afce`.
All 80 affected ordinary publication/lifecycle/worker regressions passed in
`7dea37e5-01a7-4043-a44d-3c588907dc06`. The final complete 18-case recovery target passed against
isolated canonical dependencies in `701f7dd2-b1dd-4d16-9f0b-8d29cd57ca1d` (45.025 seconds).
Local and canonical app all-target checks passed with `test-faults,gpui/windows-manifest`.
These checks used one Cargo job, LLVM, disabled debug/incremental compilation, and the fixed
native stack. All 42 canonical compilation inputs matched the reviewed source/manifests; all
31 app source/test hashes matched the frozen review inventory. No phase 595 source was included.

The accepted toolkit and Settings revisions are respectively
`803126e53d2b2fb499f63bab12a7b950497588b0` and `af66f41c64c8648fd9a1154161d093819d5a7b35`.
The 45-case local/canonical toolkit runs and dependency qualification above remain applicable;
canonical Beryl resolves one text-input package. No persistent schema was changed.

This accepts supplied-proof resident recovery only. Retirement refuses before dropping adapters
or resources whenever unfinished original seal work or terminal unprepared staging authority
remains, including zero-service-flight collision custody. Phase 706 owns retaining that authority
and preparing fresh candidate marker evidence; phase 595 owns production ordinary-command
integration. Genuine enabled dirty editing still has the separately recorded WinRT stack failure.
Bounded logs and frozen inventories remain under the ignored `.tmp/failed-resident-evidence` and
`.tmp/failed-resident-app-*` paths. The isolated checkout is removed after exact-path/reparse checks.

## Candidate Marker Preparation Acceptance

Phase 706's candidate marker preparation passed independent persistence/lifecycle review and
combined canonical verification on 2026-10-02. State and Syndic share ordinary authenticated
Building/completion and seal preparation algorithms. The app retains original staging capability,
source/frontier, command outcomes and reconciliation under the existing process flight capacity,
then admits, drives or releases through fresh candidate access. Exact captured custody permits
host retirement; missing or foreign capture and terminal collisions preserve refusal. Fresh and
resumed preparation compose with exact save and same-session reconstruction. Ordinary-command
routing and complete process graph mounting remain phase 595 work.

Review found that ordinary disposal could validate the failed home before checking the caller's
generation, retiring the newly rebound shared recovery ledger. The corrected entry acquires the
same generation/recovery-ownership guard as other ordinary operations. Its regression proves
stale disposal cannot consume the retained flight. Candidate-origin begin/page/Asset-seal
noncommit and uncertainty, retry/rebinding, and Cancelled/Failed/Superseded terminal cuts passed
focused tests and the final review. The State/Syndic scoped-review inputs remained unchanged.

All 133 selected tests passed in isolated canonical run
`9e998656-8285-4725-bd4e-3afb96659753` (278.529 seconds), covering State Asset reads/completion,
ordinary and candidate Syndic seals, app marker service/lifecycle, resident recovery, composer
lifecycle and publication. Both real composed candidate-marker cases also passed in local run
`23186433-4713-49a2-a645-e80cdd2f99e9`, alongside all 32 marker service/lifecycle cases.
The canonical run used one build job, LLVM, disabled debug/incremental compilation, a 60-second
per-case deadline and process-local `RUST_MIN_STACK=33554432`; native WinRT worker stacks were
unchanged. Five existing resident fixtures overflowed the default Rust test-thread stack both
locally and on untouched accepted app baseline `b700caa1` in run
`f89817b0-1455-41fa-a4a4-4fb887666f04`. This evidence does not claim default-stack acceptance
for those fixtures or resolve phase 595's separate enabled dirty-edit worker-stack failure.

Canonical locked metadata and final app all-target checks passed with
`test-faults,gpui/windows-manifest`. All 28 phase source/test copies matched SHA-256 inventories;
the tracked lockfile remained unchanged. Copying source with preserved timestamps after baseline
compilation initially reused a stale test library. Refreshing the isolated source timestamps
forced recompilation before the successful combined run and final all-target check; failed-build
output is retained, not acceptance evidence. Bounded logs and inventories remain under
`.tmp/candidate-marker-evidence`. The isolated checkout is removed after exact-path/reparse checks.

## Nonfinal Native Destruction Failure

Complete phase 595 review on 2026-10-02 invalidated routing every reported nonfinal close failure
through failed-home recovery. In `running_owner/ordinary_commands/nonfinal.rs`, durable removal
precedes `complete_nonfinal_close`, which installs detached sources and retires residents before
`begin_final_native_cleanup`. Windows destruction can then fail. The surviving shell is retired
and durable membership absent, but the home remains Healthy. Failed-resident capture and failed
graph retirement correctly reject that generation; they cannot restore coherent interaction.

Acquiring the native observer earlier can move preflight refusal before retirement, but cannot
eliminate subsequent destruction failure. Destroying the window before durable settlement would
violate the existing close ordering. Do not manufacture home failure or reuse process Quit Anyway
authority for a nonfinal close. Main-window feature and app lifecycle authority must select an
exact healthy-home restoration path or a separately specified nonfinal blocked outcome. The
recommended restoration path must retain exact member/claim/editor custody, cancel the failed
request permanently, and require fresh close activation. The Operator approved this direction on
2026-10-02. Updated main-window, State and app lifecycle authority defines original Healthy-
generation restoration with reversible resident custody and exact native-survival settlement;
implementation and acceptance remain outstanding.

Independent review covered the complete pending ordinary-command app boundary. Three source
corrections are present but await integration evidence: release each completed saved marker flight
before admitting the next resident under capacity one; filter restored-removal evidence by exact
window; and retain/start failed-home recovery for toolbar Exit's `UnremovedWindows` draft failure.
The seven-case native recovery run `77cc512b-0025-4aa5-b054-d25d463dffd9` passed zero cases,
exposing worker stack overflow and a restored composer-slot identity mismatch. The genuine dirty
run `e8857034-3bd5-411a-a36a-f2fa81adbbdf` timed out at 60 seconds after WinRT worker overflow.
Logs and review inventories remain in `.tmp/ordinary-recovery-integration`.

Bounded boxing and frame separation in five Syndic mutation/staging files passed independent
semantic review and 34 focused regressions in `c1b3791a-8056-4fdb-94dc-d77ebb353ed7`; this does
not establish native stack safety. Evidence remains in `.tmp/dirty-stack-correction`. Production
worker stacks were not enlarged. Phase 595 remains unaccepted with all pending source preserved;
the approved native failure policy permits resumption; finish its implementation, stack/identity fixes,
toolbar dirty-failure and multi-resident capacity-one tests, and combined canonical acceptance.
The last review and claim-adoption corrections remain uncompiled; formatting is pending. The
worker's 33-path inventory and hashes are retained in `.tmp/ordinary-recovery-integration`.
All eight exactly identified fixture homes were removed after path/reparse checks; no owned test,
Cargo or debugger process remains at the checkpoint.

## Recoverable Native Destruction Acceptance

On 2026-10-02, phase 707 accepted GPUI revision
`1664626feb9e4904b7d8bf4016cbed42da558536`. The owned attempt retains the exact logical window,
root and platform wrapper through native settlement, distinguishes settled surviving failure from
uncertainty, and permits a fresh attempt only after settled failure. Partial destruction fences
native methods and removes physical handles from global dispatch without discarding unresolved
logical custody. Foreground dispatch avoids native calls under the cross-thread registry lock;
default quit also waits for unresolved logical cleanup. The [fork evidence](../../../zed-fork/doc/failures/recoverable-native-destruction.md)
records these invalidated assumptions and their corrections.

Independent lifecycle review accepted the complete source and native boundary. Corrected native
run `310f7f49-01dd-4c56-847c-69e1151b2e5a` passed 12 cases in 6.457 seconds, including exact
failure/fresh success, abandoned controls/receivers, stale completions, confirmation cleanup,
uncertain native settlement with another closing window, and default final-window exit. Local
and canonical shipped-feature checks passed without test support; all 11 reviewed input hashes
matched. Native worker stacks were unchanged. Logs remain in `.tmp/ordinary-native-evidence`.

Aligned and pushed widget revisions are scrollbar `756efe9af67c95eef1eb23bfe71e3c6573e21302`,
text input `5eb31033c95ae2ce3187363ab127d218d7a5413b`, and Settings
`2f8f526dc910b3132b9190014a1aee5764b7d3ab`. Independent pin/lock review, locked metadata and
isolated canonical all-target checks passed for all three and Beryl app. Each graph resolves one
GPUI package. Beryl qualification used committed source baseline `1e1e099b`; pending phase 595
source was excluded. This accepts the native prerequisite and dependency alignment, not the
unfinished healthy restoration, editor/claim binding or existing dirty-edit stack correction.

After validation, the mandatory Serena restart returned a 120-second tools-call timeout.
Implementation stopped under the repository's explicit restart-failure rule; no stale semantic
results were used. Restore the language server before resuming phase 595. The four exact owned
canonical checkouts are removed after absolute-path and reparse checks; retained logs remain bounded.

The Operator restarted Serena; a fresh semantic lookup resolved the accepted GPUI native
destruction API on 2026-10-02 and implementation resumed.

## Exact Original Opening Identity

The first healthy State restoration run `c55255b8-eb3e-4c3d-9cfc-a629d8be7b3f` passed four
candidate cases, then rejected the implementation's assumption that durable home identity plus
`HomeGeneration` identifies an original opening. Closing and reopening the same home resets the
numeric health generation to `INITIAL`, so stale removal evidence incorrectly passed ordinary
classification. Eight remaining cases were cancelled; this is not acceptance evidence.

The existing contract requires original process-opening identity. Qualify it using an opaque
immutable pair of private store-instance identity and admitted generation, captured through
ordinary healthy admission without retaining a store resource. Keep candidate recovery's explicit
same-home replacement authority separate. Add same-home close/reopen refusal to the regression
boundary and independently review the HomeStore/State integration before acceptance.

The corrected local run passed all 16 selected cases (three HomeStore identity cases and 13 State
removal/recovery cases) in 12.538 seconds. The final command selected `generation_identity` and
`window_removal_recovery` with `test-faults`, the locked local graph and one test thread. Focused
HomeStore/State all-target checking passed in 23.86 seconds; locked metadata, exact-file formatting
and diff checks passed. Independent review found no blocking defect across identity admission,
ordinary and candidate qualification, read bracketing, writer reclassification and preserved paired
claim restoration. App lifecycle integration and combined canonical acceptance remain outstanding.

Canonical qualification of the current HomeStore, State and Syndic source subsequently passed all
76 selected cases in 134.087 seconds (`660e7718-8ec8-488b-88b0-d03d74ddf331`). This combines
the 16 identity/removal cases with 60 publication/history/checkpoint/staged-outcome regressions;
the previously qualified long continuation case remains excluded. All-target checks for all three
crates passed in 65 seconds. The isolated checkout has no local Cargo overrides, all 31 copied
source/test/document paths match the retained SHA-256 inventory, and copied source timestamps
were refreshed before compilation. Evidence is retained under
`.tmp/ordinary-recovery-integration/canonical-storage-*`. This qualifies the storage boundary;
current app/native integration and full phase acceptance remain outstanding.

## Final Ordinary Recovery Integration

The pre-native route passed independent lifecycle review after a final-window gate mismatch was
corrected. Final ordinary close owns a shutdown gate, while native-survival reattachment also
requires the nonfinal ordinary-close gate. Setting that second gate during final close would hide
the admitted barrier's `Exiting…` presentation. The corrected pre-native entry instead validates
the exact retained proof before entering the private shared reattachment body. Native-survival
admission retains its strict gate; common draft, window, controller, resident and claim checks
remain in force.

Focused native run `ff40a135-bb24-46d5-928e-db1a3bb46fe2` passed eight cases in 26.763 seconds,
including dirty close/toolbar recovery with full authenticated history checks and three pre-native
proof/restoration cases. Combined run `17f6becb-da13-49cb-8c9d-d89cbeb85e77` passed 54 of 57
cases in 185.217 seconds, including the added final-window `Exiting…`/fresh-close case. It exposed
a created-window Exit delivery timeout, a final-close test incorrectly retaining its old execution
permit, and an actual clean-editor recovery worker-stack overflow. These failures prevent acceptance.

The frozen binary's first-chance debugger trace is retained in
`.tmp/ordinary-recovery-integration/final-recovery-worker-stacks.txt`. Clean slot reconstruction
held a 205,648-byte construction frame above saved-candidate authentication and settlement
decoding. Moving that private construction into a non-inlined helper keeps its frame out of the
authentication call chain without changing public APIs, validation order or error custody.
Worker stacks remain unchanged. The exact debugger fixture was removed after path/reparse checks;
focused verification and independent correction review remain required.

Independent review accepted the constructor extraction. Focused run
`9e45cf5d-cd84-4a7d-a2df-ed5c92c4f512` then passed both final-close cases, including the former
worker overflow, in a three-case run lasting 38.241 seconds. The remaining created-window Exit
failure now reports an exact placement identity mismatch. Placement capture zipped sorted window
IDs with insertion-ordered shells; a newly created ID can sort before an existing member. Replace
that positional pairing with exact identity lookup while retaining complete-set and per-window
validation, and add a deterministic reversed-shell-order regression before acceptance.

The exact lookup correction and deterministic reversed-order fixture passed independent review.
Run `89bce977-5b32-42e1-bd58-fc54fcb371c5` passed both that regression and the original
created-window Exit case in 5.011 seconds. All prior failed cases now have passing focused evidence;
the final combined canonical run and current app checks remain the acceptance gate.

## Native Staging Preparation Regression

On 2026-10-08, ordinary unsaved-editor recovery qualification reproduced a native worker stack
overflow at accepted baseline `1c7b8b5a`. Reverting the pending frozen-read wrapper did not remove
the failure, and restoring all tracked App/Home/State/Syndic source to the accepted baseline still
reproduced it. The pending Catalog coordinator is therefore not a necessary cause.

First-chance CDB evidence in
`.tmp/coherent-catalog-source-evidence/baseline-native-first-chance-stack.log` shows staging
preparation retaining approximately 261 KiB (267,296 bytes) and its generic typed participant
preparation approximately 193 KiB (197,328 bytes) above candidate-session decoding. The callback runs on the existing WinRT
thread-pool worker; no recursive Catalog call appears in the captured stack. Large by-value private
prepared custody crosses both frames. Phase 745 qualifies bounded heap custody while preserving
public APIs, authentication and original outcomes on the unchanged production stack.

The first boxed diagnostic reused a pending Home artifact from the shared target directory: its
stack contains `execute_guarded_read`, absent from the canonical baseline source. This run is not
isolated-baseline acceptance evidence. Refresh every affected crate entry point after switching
overlays, including unchanged dependency source, so Cargo rebuilds the complete intended input.
The corrected qualification rebuilds Home, State, Syndic and App before executing the native case.

Fresh isolated-baseline qualification with the three-path staging correction completes dirty
editing, then exposes failed-resident reconstruction stack pressure. CDB evidence in
`.tmp/coherent-catalog-source-evidence/staging-box-baseline-first-chance-stack.log` shows the
unchanged ordinary Home read path and an approximately 114-KiB reconstruction frame retaining
slot-construction storage above deep saved-candidate authentication. Keep authentication in the
reconstruction body and move slot assembly into a private non-inlined helper, following the same
frame-separation rule already qualified for the other recovery constructor below. Runtime
acceptance remains pending; the diagnostic is evidence of remaining failure, not a passing check.

The first isolated all-target check also discovered eighteen task-owned untracked Catalog test
files left by the pending overlay. Restoring tracked baseline files does not remove automatically
discovered integration tests. Their references to pending frozen APIs made that check fail without
invalidating the scoped native or staging inputs. Move those exact owned files to bounded evidence
custody, then rerun the baseline all-target check; restore them before source-readiness qualification.

The complete four-path correction passed independent semantic review, 84 canonical staging and
history cases, all twelve ordinary failed-Home native recovery cases and isolated App/Syndic
all-target checks on 2026-10-08. The original two-unsaved-editor case passes on unchanged production
Windows worker stacks. [Qualification](../audits/native-editing-recovery-stack-qualification.md)
records exact inputs, diagnostic exclusions and evidence reuse; Catalog source acceptance resumes
separately on this prerequisite.

Resumed Catalog qualification on the accepted correction passes the ten coordinator and nine
lifecycle cases but still overflows during native resident save publication. First-chance evidence
in `.tmp/coherent-catalog-source-evidence/catalog-resumed-first-chance-stack.log` shows large
by-value `PublicationMutation::prepare` and generic typed participant frames above exact checkpoint
decoding. Qualify bounded private publication heap custody under the active source integration
boundary, preserving public commands, authentication, replay, writes and original outcomes.

Publication boxing preserves writer semantics but the resumed native packet still fails during
saved-candidate qualification. Fresh `publication-box-first-chance-stack.log` shows approximately
171 KiB in the async task polling/result frame above the recovery-service future and deep decoding.
The worker returns service owner, original session and settlement by value. Heap-own that single
private result until the GUI continuation consumes it; preserve original cancellation and disposal
branches. A boxed future alone does not bound the executor's by-value output frame.

With that result boxed, unsaved-editor recovery passes and the remaining uncertain-enrollment
native case times out during final cleanup without overflow. Its helper seeds a real populated
Syndic thread without registering that fixture's Runtime 48/Root 49 binding. Correct the helper
and all seven callers to publish the exact RuntimeRoot authority before canonical thread creation,
retaining original indeterminate enrollment assertions and active coordinator execution.

That correction exposes the fixture's second inconsistency: it uses a threadless window after
configuring a runtime. The conversation-thread invariant requires every visible window to select a
thread once any runtime exists; threadless recovery explicitly requires no configured runtime.
Use one selected window for the uncertain-enrollment fixture and retain its complete original
outcome and resident checks. Keep the separate zero-runtime threadless recovery case unchanged.
Do not relax the production threadless validator to accommodate this invalid fixture envelope.

## Catalog Source Integration Regression Findings

The 2026-10-08 complete App packet distinguishes native worker stack failures from older fixture
and continuation mismatches. The HomeBusy creation fixture reused onboarding's canonical executable,
so State rejected the otherwise valid runtime/root command before its `AfterCommitBeforePersist`
barrier. Give this separate blocker its own executable identity; preserve the Busy fence and retry
of the same publication. Do not suppress Catalog work to make the barrier reachable.

Older interrupted-Exit fixture expectations also disagreed with existing exact binding behavior.
Repeated binding validates and reuses the installed same-generation command binding. Qualification
must prove unchanged appearance, binding count and command identity with interaction still fenced.
An intentionally failed Home can additionally leave a drained Catalog source read failure during
teardown. Accept only the exact Failed-generation HealthGate classification and separately prove
the attempt is Retired with no graph or retained close; other teardown errors remain failures.

Frozen query integration exposed another original-custody fixture omission. A joined query service
remains in its bounded retirement slot even when no collection was opened; work drainage alone
does not settle that original owner. The recovery-publication helper recovered a candidate and
prepared services without the production route's `settle_retired_process_work` step. Publication
correctly refused the retained slot. Reacquire State/Syndic from that same candidate and settle
through its exact recovery access before every replacement preparation; preserve the publication
guard and all original fault/fence assertions. The complete corrected fixture family passes in
[query qualification](../audits/frozen-catalog-query-qualification.md).

The same diagnostic packet aborted one ordinary recovery fixture after it compared unrestricted
desktop foreground HWNDs across an eleven-second scenario. Neither HWND owner was recorded, so
the mismatch does not prove recovery changed Beryl focus. The abort followed an assertion panic
crossing a native callback boundary, not the earlier worker stack-overflow signature. Qualify each
preserved window's exact logical focus, native HWND and placement without post-recovery refocus;
keep external desktop transitions diagnostic and do not claim shared-desktop foreground continuity.
The complete corrected ordinary recovery family passes, including original uncertain enrollment.

Abandoned resident preparation and refused current configuration expose an actual continuation
precondition mismatch. Resident preparation owns the original graph while service settlement is
Pending. The outer driver nevertheless performs a fresh New Thread classification that requires
Services, preventing the existing authenticated preparation from resuming. Classify only fresh
entries; retained preparation or attachment resumes its exact existing path with cancellation,
window-set, request, generation and final publication validation unchanged. The existing abandoned
and refused retry cases remain required runtime evidence.

The complete App packet `ab6fb7ad-720b-4b69-b33b-dc3ba876b182` ran 555 cases in 2190.811
seconds: 517 passed and 38 failed. It is completed diagnostic evidence, not a passing regression.
First-chance `ordinary-read-first-chance-stack.log` captures saved-checkpoint startup authentication
on a Windows thread-pool worker with the new ordinary `execute_read` closure and
`execute_guarded_read` result wrapper in the overflowing stack. Restore the direct ordinary read
body. Frozen retention and reads instead keep their admitted request guard outside the operation
closure, drop it after confirmed execution returns on every outcome, and only then propagate the
result or commit a retention reservation. This avoids duplicating generic ordinary results while
preserving confirmation drainage, failure classification, exact snapshots and finite slot charging.
The corrected frozen-read suite passed all ten cases (`21753726-bb69-4807-a749-74afbb861e97`,
0.635 seconds); native stack qualification remains required.

The direct-read correction passes all source, lifecycle, HomeBusy and corrected session/retry
fixtures in the next packet, but does not alone eliminate two native stack overflows (32/34,
166.772 seconds). Fresh `direct-read-first-chance-stack.log` confirms the wrapper is gone and
measures a 136,544-byte async-task polling frame above saved-checkpoint startup authentication.
The private startup worker returns its worker/preparation tuple by value. Heap-own that single
handoff result and unpack it after awaiting on the GUI executor, preserving worker, cancellation,
native preparation, startup fence and original failure custody. Do not claim the read wrapper was
the sole cause or increase production thread-pool stacks.

That handoff correction clears saved-checkpoint nonfinal close and both final-teardown cases.
Additional-window creation still overflows. `additional-window-first-chance-stack.log` measures
227,600 bytes in creation advancement above initial composer claim authentication. Move the
failure-only unpublished-preparation conversion into a private non-inlined helper, preserving the
original validation failure, cancellation, retained preparation and bounded advancement loop.
Both created-window Exit and native-close cases then pass (`0076f718-79a2-4057-8c3c-2f21968c3bb5`).

The combined affected packet `c9a60a03-83cf-45d9-ab3a-6ec571f47a69` passes 93 of 95 cases,
including 37 of the original 38 failing cases. Two native final-close confirmation cases still
time out. The bounded timeout observation reports no startup or selection gate, an enabled Exit
command, no shutdown session, no native-close latch and no recorded ordinary-command failure.
Callback mounting order places the running callback after initial startup release. A test-only
delivery probe confirms first-request admission on both routes. Existing notice projection then
reveals the exact refusal: `a home mutation is in progress`. The unchanged frozen artifact both
passes and fails, distinguishing a race from a repaired route. The newly seeded unviewed canonical
thread legitimately triggers background Catalog repair before work observation. Confirmation
fixtures must establish an exact live certified source after that finite setup before activating
close; observing an older published snapshot is insufficient. Preserve active maintenance, Busy
refusal, fresh activation and the original confirmation deadline. Independent review cleared that
bounded setup correction. The final affected packet `5ee6ce11-46a4-49d5-8797-0c48799563ab`
passes 95/95 in 325.767 seconds, covering every original failed case; both native confirmation
cases also pass an unchanged-artifact repeat. Current four-crate all-target checks pass. Earlier
partial or broad failing packets remain diagnostic evidence, not fresh passing broad regressions.

## Ordinary Command Integration Acceptance

Phase 595 passed completion review on 2026-10-02. Ordinary native close and dedicated toolbar Exit
now consume the process-owned command route, including windows created after startup. Nonfinal
close preserves background work; final close records an empty restore set, while dedicated Exit
preserves the complete exact layout. Failed-home recovery preserves resident editor/history and
marker custody. Healthy native-survival and distinct pre-native cancellation proofs restore exact
membership and claims before reopening, retain unresolved custody, and require fresh activation.

Independent review covered the complete ordinary-command boundary, the healthy restoration and
opening-identity additions, measured private stack corrections, and exact placement lookup. The
final-window gate mismatch and positional placement bug found during review/verification are
resolved without weakening native or persistence validators. Formatting-only reconciliation and
removal of temporary test diagnostics followed those semantic reviews.

Canonical run `49106e21-209a-4e80-99ed-e5bbc86661e8` passed all 58 selected app cases in 145.336
seconds. It covers native close/Exit, created-window commands, confirmation and feature gates,
dirty resident history, marker capacity, native survival/uncertainty, pre-native cancellation,
final-window presentation, session outcomes, activation and mutation feedback. Current canonical
app all-target checking passed in 80 seconds. The previously recorded 76 canonical storage cases
and three-crate all-target check remain applicable; all their inputs are unchanged.

The isolated checkout contained no local Cargo overrides and resolved one GPUI package at the
accepted revision. All 167 copied app/storage paths matched their recorded SHA-256 values after
verification; the canonical lockfile and manifests remained unchanged. Tests used the existing
60-second per-case deadline, one build/test job, LLVM, disabled debug/incremental compilation and
process-local 32-MiB Rust test-thread stacks. Production WinRT worker stacks were unchanged.
All 31 explicitly recorded ordinary fixture homes are absent, including the retained Unresolved
fixture removed after its child exited. The older ambiguous Temp directory remains untouched.
Bounded logs and inventories remain in `.tmp/ordinary-recovery-integration`; executable bootstrap
and crash-reporter process-entry mounting remain separate planned boundaries.
