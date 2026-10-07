# Runtime And Root Creation Mounting Readiness

Source inspection on 2026-10-06 against Beryl `7172cb83`. This record is evidence for
the remaining restoration/onboarding tracker slice, not design authority or GUI acceptance.
No production code, native window, native picker, backend process or Operator clipboard was exercised.

## Controlling Contracts

The [conversation-thread feature](../features/conversation-threads/design.md#runtime-and-root-configuration)
owns native file/directory selection, exact environment validation, canonical duplicate resolution,
pending commands, unchanged cancellation/noncommit presentation and first-runtime activation.
Its [reconciliation contract](../features/conversation-threads/design.md#visible-mutation-reconciliation)
requires retained exact intent, coherent publication and terminal Unavailable without resubmission.
The [GUI contract](../features/conversation-threads/gui.md#runtime-and-root-configuration)
owns runtime row commands and New Thread confirmation in the shared thread/root picker.

The [storage system](../systems/beryl-home-storage/design.md#runtime-and-root-registry)
owns atomic registry publication; its [claim contract](../systems/beryl-home-storage/design.md#thread-claims-and-empty-thread-acquisition)
requires first-runtime creation to include thread/draft, claim and selected-window/session updates
in the same command. The [backend runtime system](../systems/backend-runtime/design.md#runtime-ownership)
requires exact managed-launch foreground provenance, pinned release/profile and the sole effective
configuration read before runtime admission. A version string or detached executable probe is insufficient.
State's [runtime/session contract](../../crates/beryl-state/doc/design-runtime-session.md)
admits caller facts and performs no filesystem, WSL, clock or process observation.
The [app boundary](../../crates/beryl-app/doc/design-shell-lifecycle.md#process-service-graph-and-windows)
owns services, bounded workers, per-window controllers and ordered retirement.

Applicable production contracts include external side effects for the feature/app/backend and
persistent integrity for State/Home/Syndic. Future implementation must qualify both external
validation cleanup and the cross-domain durable outcome, with independent consequential review.
This read-only readiness record establishes no new persistence or process behavior.

## Actual Consumers And Reusable Capabilities

- [MainWindowShellRoot rendering](../../crates/beryl-app/src/main_window/shell/host/root.rs)
  mounts Running threads, New Window, Exit, status, transcript, composer and Notices. It currently
  has no New Thread split button, setup flyout or Add runtime/Add root handler. The threadless
  shell's New Window explanation points to that still-absent ellipsis command.
- [Threadless shell preparation](../../crates/beryl-app/src/main_window/shell/host/threadless.rs)
  retains the existing durable window and native reservation. Onboarding must attach the first
  selected conversation to this shell rather than acquire a replacement native window.
- [ThreadRootPicker](../../crates/beryl-app/src/thread_root_picker.rs) has bounded revision-bound
  pages, focus, keyboard traversal and scrollbar disposal. Its [model](../../crates/beryl-app/src/thread_root_picker/model.rs)
  uses 32-row pages, at most 24 resident pages, a 24-row realization cap and four-row overscan. Events cover
  query, page request, row activation and dismissal; runtime row commands, runtime scope and
  pending root/Confirm behavior are not yet a mounted contribution. Running threads is an
  existing consumer, whose behavior must remain qualified when extending this shared widget.
- GPUI's `App::prompt_for_paths` and `PathPromptOptions` in the owned
  `zed-fork/crates/gpui/src/app.rs` and `platform.rs` expose single file versus directory selection.
  Windows `platform/windows/platform.rs` implements the native dialog with `IFileOpenDialog`
  and `FOS_PICKFOLDERS`. This proves an available API, not runtime-environment validation,
  an exact-runtime-scoped folder picker or accepted Beryl command mounting.
- [RuntimeRootState](../../crates/beryl-state/src/runtime_root.rs) exposes exact executable/path
  lookup, cursor-paged registries and `create_runtime_with_home_root`/`add_root` contributions.
  `RuntimeRegistration` validates admitted path/mode agreement; it does not establish that an
  executable exists, derive its environment, find its user home or qualify the backend release.
  [Registry tests](../../crates/beryl-state/tests/runtime_root.rs) remain component evidence.
- [ReplaceWindowClaim](../../crates/beryl-state/src/session/mutation/create.rs) accepts an exact
  threadless predecessor with no paired claim, creates the active paired claim and updates the
  existing window, remembered target, header reference and fallback together. The
  [replacement tests](../../crates/beryl-state/tests/window_claim_replacement.rs) exercise the
  threadless source. This contribution can support onboarding; it alone does not create a
  registry, Syndic thread/draft or catalog row.
- [RuntimeBackedWindowAcquisitionService](../../crates/beryl-app/src/window_acquisition.rs)
  joins an already-existing runtime/root with pristine-thread eligibility and claimed-window
  creation. Its initial `catalog_source` read requires persisted registry records. Calling this
  after a separate first-runtime registry commit would violate onboarding atomicity; its
  transaction ingredients are reusable, but its current entry point is not the onboarding API.
- [ManagedBackendServer](../../crates/beryl-backend/src/server.rs) provides managed launch,
  foreground candidate connection with launch provenance and explicit shutdown. Backend session
  `initialize_foreground` and `admit_release` in
  [bounded requests](../../crates/beryl-backend/src/session/bounded_request.rs) provide the
  initialized-profile and same-session configuration admission path. The existing
  [runtime-interest launch](../../crates/beryl-app/src/cas_projection/runtime_interest/managed.rs)
  is selected/execution-bound. It is not an executable-registration validator for an unconfigured
  runtime. Admission must retain and dispose temporary process/session/token custody without
  launching a second concurrent process for the same configured runtime.
- The [process selection lease](../../crates/beryl-app/src/window_acquisition/selection_admission.rs)
  authenticates published membership and excludes acquisition/close while retaining commit and
  publication custody. Existing Running-thread attachment and recovery are relevant integration
  seams, not proof of threadless-to-selected attachment or registration reconciliation.

## Missing Integration And Next Boundary

No production app consumer of `create_runtime_with_home_root`, `add_root` or native path prompting
was found in the current app source. There is no complete selected-path admission coordinator
combining environment/filesystem validation, exact backend admission, canonical duplicate
resolution, writer-side eligibility, first-runtime atomic contributions and exact reconciliation.
Existing startup discovery explicitly rejects a nonempty registry with no session fallback in
[DiscoverySnapshot::read](../../crates/beryl-app/src/main_window/restoration/discovery/probe.rs).
That is a concrete reason to establish the transaction before mounting Add runtime.

The next capability boundary is app-owned runtime/root admission consumed by the future New Thread
controller. It must return exact committed/noncommitted/reconciling/unavailable results and retain
original outcome and cleanup custody. It must validate off the GUI/storage writer, resolve canonical
duplicates, and publish a first runtime with its complete threadless-window transition in one
HomeCommand. Tests must distinguish registry-only atomicity from the full onboarding closure,
including stale/foreign sources, duplicate requests, cancellation, ambiguous/postcommit failure,
same-home retirement and complete managed-process/token disposal. Use bounded injected filesystem
and backend seams; native interactive qualification remains a later explicit gate.

After that capability is accepted, mount the New Thread runtime/root setup contribution, Add
runtime/Add root and first selected-conversation attachment. Verify native-picker cancellation,
command pending/duplicate suppression, exhaustive coherent pages, preserved scope/search/selection,
per-window errors, terminal Unavailable, exact existing-window attachment and disposal. Complete
ordinary root confirmation and thread creation remain an explicit subsequent acceptance boundary.

No conflicting product or ownership authority was found. Filesystem admission mechanics and typed
composition still need implementation; this record does not qualify them by naming existing APIs.
The broad restoration/onboarding tracker item remains open.

Implementation follow-up on 2026-10-06 found an ownership prerequisite this inspection did not
establish: exact WSL path/home probing needs a supervised filesystem helper, while the backend's
public contract currently covers app-server processes and its reusable supervisor is private.
The proposed fixed helper therefore needs owning authority before implementation proceeds.
See [admission failure evidence](../failures/runtime-root-admission.md); the earlier readiness
review does not accept that added process boundary.

The Operator subsequently instructed continuation through the existing `wsl.exe` path. The owning
backend/system authority now specifies that fixed helper and its cleanup contract. The readiness
gap is resolved; implementation and behavioral qualification remain phase 733 work.

Further implementation review found that the reusable supervisor's saved numeric Linux group ID
does not preserve incarnation authority through delayed cleanup. The ownership assignment remains
resolved, but exact Linux signalling and whole-group completion proof are a separate readiness
prerequisite. See [the current blocker](../failures/runtime-root-admission.md#exact-linux-group-ownership).
The unaccepted draft was archived and production source restored; this readiness record does not
accept the draft or a new Linux-side supervisor artifact.

On 2026-10-07 [native supervision qualification](native-wsl-supervision-qualification.md) accepted
the fixed observation and managed-launch ownership prerequisite, including original consuming
shutdown/recovery custody. Admission can now reconsider the archived storage/validation work
against those accepted APIs. The archive itself and the separate admission boundary remain unaccepted.

## Completion Review

On 2026-10-07 [atomic admission qualification](runtime-root-admission-qualification.md) accepted
the selected-path capability and single-command first-runtime transition, including both explicit
launch forms and original cleanup/reconciliation/publication custody. This replaces the earlier
component-only readiness gap; native setup and ordinary confirmation mounting remain separate
consumers with their own qualification boundaries.

Independent source and planning review accepted this record and the derived admission, setup
mounting and ordinary confirmation boundaries on 2026-10-06, with no blocking findings. Exact
relative links, heading spacing and scoped whitespace checks passed. No Cargo or behavioral tests
were needed for this evidence-only change; existing test files above are cited component evidence,
not new test execution. Future implementation requires its own qualification.
