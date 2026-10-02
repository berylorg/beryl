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
separates dispatch-completion allocation from deep staging. Its runtime qualification and the
remaining close/recovery failures are still pending. Subsequent native runs use an external
per-case deadline as well as their GUI deadline, since a blocked GUI cannot enforce its own timer.

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
