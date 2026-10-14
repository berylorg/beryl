# Restoration And Progressive Bootstrap Readiness

## Boundary And Authority

Inspected accepted Beryl `1c363fd4` on 2026-10-14 for root-plan readiness qualification.
This record qualifies the next acceptance boundary; it does not accept the remaining restoration
tracker item or change production behavior. Controlling contracts are
[main-window startup and placement](../features/main-windows/design.md#startup-surface),
[progressive shell readiness](../features/conversation-threads/design.md#progressive-shell-readiness),
[backend unavailability](../features/backend-runtime-recovery/design.md),
[home startup](../systems/beryl-home-storage/design.md#session-and-window-records) and
[App restore-set ownership](../../crates/beryl-app/doc/design-shell-lifecycle.md#restore-set-startup-ownership).
Main Windows and the home system select `production-application/v2` with no modifiers;
Conversation Threads and the App additionally select `external-side-effects/v2`, and
[State](../../crates/beryl-state/doc/design.md#engineering-rigor)
selects `persistent-state-integrity/v2`. Backend Runtime Recovery additionally protects unaffected
local workflows and same-binding recovery with `availability-required/v1`; it does not promise
uninterrupted backend service. Exact saved identity, original command outcomes, gated partial publication,
bounded preparation and retained cleanup custody remain required.

## Existing Production Boundary

`crates/beryl/src/main.rs::start` calls App `bootstrap::run` with fixed-home and immutable typed inputs. The startup
owner prepares the complete private graph on a worker, publishes its service generation, then
discovers and prepares the bounded restore set. Service publication alone grants neither a valid
restore set nor a visible window. The complete native-set owner retains prepared members through
hidden construction, placement, real editor readiness, final source validation and native exposure.
Every member stays gated until all native publication calls succeed. Failed publication disposes
the entire attempted set while preserving sealed records; blocked cleanup retains original owners.

App `main_window/restoration/discovery.rs::RestoreSetDiscovery::advance` owns exact session discovery and
original begin-restore/initialization flights; `main_window/restoration/preparation.rs` prepares and validates
all required members. `main_window/restoration/native/flight.rs::NativeRestoreSetFlight` owns the native set.
`startup_owner/worker.rs::Worker::prepare` and `startup_owner/controller.rs::Controller` compose
these boundaries with graph and process ownership.

`main_window/shell/host/startup_readiness.rs::MainWindowShell::startup_readiness` checks exact
window, shell, appearance, placement and selected-editor identity. Its selected readiness delegates
to `conversation_composer_owner/lifecycle.rs::selected_first_presentable`, requiring a live exact
selected service and current interactive draft surface. It does not wait for catalog certification,
transcript records or CAS readiness. Threadless readiness requires no selected editor.

`startup_owner/controller.rs::Controller` releases the original catalog start fence only after
`MainWindowNativeRestoreSetCompletion::Published`, through `Worker::publish_catalog_start`.
The selector's `host/thread_switcher/render.rs::render_command` and ordinary opening separately
require the published catalog reader's readiness. This preserves the intended first shell before
catalog maintenance; catalog readiness is not a whole-set native admission condition.

`host/running_threads/transcript.rs::sync_running_transcript` separately owns bounded selected
construction, attachment work, request/cancellation and measured publication. Its worker advances
the selected transcript and prepares attachment; GUI publication awaits current panel measurement.
Those tasks are absent from first-editor readiness. Shell `host/status_controls.rs` separately
polls selected submission availability and admits observations only for the exact selection,
generation and request epoch. It initially withholds submission authority. These distinct producers
provide seams for real delayed-readiness evidence; inspection alone does not certify their visible
initial dimming or every native notification.

## Qualified Earlier Evidence And Its Limits

- [Complete restore-set coordination](../failures/target-bootstrap-composition.md#complete-restore-set-coordination)
  supplies exact saved editors, changed-member refusal, runtime-backed remembered-target replacement,
  absent/empty/existing threadless sessions, cancellation and original uncertain/committed command
  custody. Its 13 new plus 74 affected passing cases accepted worker coordination.
- State `tests/session.rs::claimed_window_capacity_commits_256_and_rejects_257_without_mutation`
  and `tests/session_schema.rs` exercise fixed-record validity, exact reverse claims and the 257th
  record refusal. `tests/exit_session.rs` covers full-set reopening at capacity. These are existing
  focused checks; this readiness inspection does not claim a fresh run or a native 256-window test.
- [Placement](../failures/target-bootstrap-composition.md#worker-placement-resolution),
  [prepared shell geometry](../failures/target-bootstrap-composition.md#hidden-shell-prepared-geometry)
  and [saved-desktop qualification](../failures/target-bootstrap-composition.md#hidden-desktop-qualification)
  supply reachable geometry, exact monitor/window binding and current-desktop fallback without
  switching the active desktop. Placement fallback does not change another saved member.
- [Complete native-set acceptance](../failures/target-bootstrap-composition.md#complete-native-startup-set)
  supplies interaction-gated ordered success, last-member failure, stale final validation,
  cancellation, partial visibility, sealed-record preservation, reentrant close refusal and
  retained native/transient disposal. Its 122-case acceptance and corrected readiness-hook case
  explicitly do not prove independently held real first-editor loading.
- [Startup owner and cleanup](../failures/target-bootstrap-composition.md#startup-cleanup-failure-presentation)
  supplies fixed-home serialized Retry/Exit, original retained graph/native/home outcomes, fresh
  generation admission only after proven retirement and blocked cleanup presentation. Its accepted
  47-case owner/surface receipt complements the earlier failed-retirement and Quit Anyway cases.
- [Executable bootstrap](../failures/executable-bootstrap.md#bootstrap-acceptance) supplies actual
  process entry, restored selected identity and diagnostic channel-loss shutdown through ordinary
  ownership. Canonical acceptance includes 65 App/native, 10 executable and nine backend cases.
- [Thread Switcher](thread-switcher-mount-qualification.md) and
  [complete transcript](mounted-transcript-workflow.md) now supply bounded real mounted consumers,
  coherent publication, exact source/geometry admission and disposal. Transcript construction and
  captured-state supersession are accepted prerequisites. These receipts do not by themselves
  prove startup under independent catalog, transcript and runtime delays.
- [Coherent catalog readiness](coherent-catalog-source-readiness-qualification.md#completion-review-and-limits)
  supplies guarded source certification and prepublication fencing, including held, cancelled and
  headless initial-fence cases in the accepted 95-case App packet. This supplies the catalog source
  prerequisite rather than the complete native progressive-readiness proof.

These are earlier accepted outcomes, not a newly passing aggregate. Relevant bootstrap, catalog,
composer, transcript, recovery and dependency inputs have changed since the original startup
receipts. Reuse their unchanged guarantees after checking the affected input graph; refresh the
combined startup consumers affected by later work. Source inspection can establish independence
of admission conditions, but cannot replace runtime notification and native input evidence.

## Remaining Acceptance Gap

The target architecture determines the remaining behavior. No material design contradiction or
missing production capability has been demonstrated by this qualification. The missing evidence
is composition through first native shell publication with genuine asynchronous loading held:

- Hold real selected draft readiness and prove native publication waits; release it and observe
  actual readiness notification, exact draft content and usable first editor in every member.
- Independently hold catalog certification, initial selected-history delivery and runtime warm-up.
  Prove the complete ordinary shell can publish, each affected control remains appropriately
  unavailable, and healthy draft input persists without sending user data. Release each source and
  prove only its own readiness gate clears without replacing the window or selected thread.
- Compose runtime unavailability with browsable Syndic history and preserved local draft;
  backend failure must not become startup failure. Submission stays closed until its exact
  prerequisites are ready. No successful model turn is required to prove withheld submission.
- Cancel or fail while real editor/loading work is outstanding and prove original worker/native
  cleanup and command custody settle or remain explicitly retained. Reentrant close and startup
  gates cannot expose a partly interactive set or turn unresolved cleanup into Retry authority.

The next technically ready boundary is complete native restoration with independent progressive
readiness, using production startup/restore ownership and existing native fixture seams. Refresh
affected startup/restore, first-editor, catalog-readiness, runtime-readiness and ordinary-close
checks; add only missing integration cases. Supporting test corrections or bounded production
corrections must remain within that behavior and preserve its controlling contracts. Obtain
independent semantic review before closing the restoration/progressive-bootstrap tracker item.
Broader catalog/search/lineage, Settings, Assets and conditional repair remain separate gates.

## Review

Independent semantic readiness review passed on 2026-10-14 with no blocking findings. It checked
the whole-set/editor admission predicates, native readiness observer, separate progressive producers,
historical evidence limits, capacity and empty-session branches. Root corrected the scope-specific
rigor attribution. This accepts readiness for the next test-focused boundary, not its runtime
behavior; no new executable or native test results are claimed.
