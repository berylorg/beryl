# Existing CAS Regression Failures At Idle-Maintenance Acceptance

## Scope And Evidence

On 2026-09-10, broad beryl-app verification ran the library tests and `connection_work`,
`runtime_demand_custody`, `process_scheduled_sessions`, `runtime_interest`,
`managed_runtime_interest` and `runtime_execution_sessions` with `test-faults` enabled.
Of 300 cases, 283 passed and 17 failed. Three runtime-interest tests counted all notifications as
preparation readiness; the new maintenance wake exposed that ambiguity. Their corrected test seam
observes the exact `ExecutionReady` bit, and all 13 runtime-interest tests pass.

The other 14 failures were reproduced from an isolated `git archive` of committed prerequisite
`df7c70b4`, using the same local owned-fork configuration, lockfile, stable toolchain and one-job
build settings. The baseline selection ran 18 cases: four passed and the same 14 failed with
matching failure details. This comparison establishes that they predate idle-maintenance changes;
it does not establish their underlying causes or excuse an unmet production guarantee.

Bounded logs and process summaries are retained under
`C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908`, with prefixes
`idle-maintenance-controls-all-20260910` and `idle-maintenance-baseline-20260910`.
Both guarded jobs reaped their root and all children. The baseline source copy, archive and
job temporary directories were verified and removed.

## Failure Families

- One active-steering delivery case expects zero total free-space observations but observes one:
  `lifecycle_before_exact_response_still_completes_delivery`.
- Five marker/replay cases fail in `tests/unit/submission_fixture.rs` at mutation-page staging
  with `MutationStaging(Invalid)`: repeated-image delivery, real image-bearing provider replay,
  both marker-aware accepted-input cases and the marker-free replay factory case.
- Five provider compaction-marker cases fail while mounting the lifecycle test operation with
  `AuthorityMismatch`, before their publication assertions.
- Three checked-user terminal-failure cases disagree with asserted health, authority or cleanup
  outcomes: before-commit terminal failure, after-persist terminal failure and persistent activation
  publication panic.

The baseline filter is:

```text
test(lifecycle_before_exact_response) | test(repeated_image_labels_dispatch) |
test(compaction_marker) | test(real_image_bearing) | test(after_persist_terminal) |
test(before_commit_terminal_failure) | test(persistent_activation_publication) |
test(input_replay::accepted)
```

Run it with `cargo +stable --config .cargo/local.toml nextest run --locked -p beryl-app
--features test-faults --lib --jobs 1 --no-fail-fast -E '<filter>'`.

## Correction Boundary

Reconcile each test against current composer, service-generation, control-custody and terminal
failure authority. Correct obsolete setup and assertions while retaining meaningful failure
coverage. Do not broaden the accepted idle-maintenance implementation or weaken runtime guarantees
to make this selection pass. A demonstrated production defect requires its own implementation
boundary. The active plan tracks this work before process composition resumes.

## Reconciliation Blocker

On 2026-09-10, reconciliation established a production defect in
`persistent_activation_publication_panic_cuts_cleanup_and_drains_before_acknowledgement`.
An activation-publication panic fails the home, but `publish_active_turn_once` settles its permit
before returning the publication failure. `LiveCommandHealthFence::settle_after_operation`
classifies the failed home as generic closed authority. `Ingester::run_loop` then disposes
authority-lost work and acknowledges it before reaching exact persistent-failure notification.
The assertion observes drained permits but command admission still open. It remains unchanged.

This violates the CAS-live failed-write fencing contract and the activation-panic consequence
already recorded in [retention and retirement authority](cas-phase77-retention-and-retirement-authority.md).
The test-only reconciliation approach cannot repair this path. Production changes stopped under
the repository technical-plan-failure rule. The initial proposed correction was exact failed-home
notification before generic authority-loss disposal and acknowledgement. The Operator subsequently
rejected assuming in-process panic recovery and selected fatal reporting as described below.

The before-commit and after-persist terminal-storage tests had obsolete expectations. Their raw
injected I/O failures are structural failures, not indeterminate outcomes requiring reconciliation.
Corrected assertions preserve the fault cuts and require failed home health, observed failure,
closed command admission, zero active permits, retained target custody and no terminal reason or
proof. Both pass and independent read-only review accepts these corrections. The adjacent terminal
publication-panic test also passes.

The latest bounded selection, `cas-fixture-corrections-20260910`, ran 19 cases: nine passed and ten
failed. The remaining failures are the production activation-panic defect, four image-bearing cases
with `MutationBuildPreparation(Build(InvalidRoot))`, and five compaction-marker cases whose attempted
lifecycle-yield setup is not accepted. The active-steering observation-delta correction and bounded
large marker-free replay now pass. Image and compaction fixture edits remain unaccepted working
material; the selection does not establish phase completion or a green regression suite.

Logs and process summaries use the retained evidence directory above, with prefixes
`cas-fixture-reconciliation-20260910` and `cas-fixture-corrections-20260910`. The final guarded job
reports `root_exit=100`, `root_reaped=true` and `remaining_job_pids=[]`; its verified temporary
directory was removed. No production fix was made and these partial test corrections remain
uncommitted while the separately authorized fatal-reporting boundary is implemented.

## Fatal Panic Reporting Direction

On 2026-09-10 the Operator selected a terminal panic GUI with only `Copy to clipboard` and
`Exit Beryl`, no ordinary interaction or background work, and direct termination after Exit without
saving, flushing or ordinary shutdown. Failure to safely present the report permits immediate
termination. This supersedes the proposed repair of panic recovery, not existing handling of
ordinary returned storage errors.

The initial claim that an in-process report would be a simple substitute for recovery was
unsupported. `FailClosedOnWriterPanic::drop` in `crates/beryl-home-store/src/writer.rs` calls
`HealthGate::signal_failure` in `health.rs`, which changes health and notifies waiters; neither stops
existing threads. `spawn_status_model_list_worker` in
`crates/beryl-app/src/shell/status_operation.rs` demonstrates independent work without that health
gate. `execute_terminal_shutdown` in `cas_projection/connection/lifecycle.rs` takes shared locks,
uses poisoned inner state, requests cancellation, joins workers and drops attachments. It is not a
state-independent halt boundary. Independent read-only architecture assessment confirms that
disabling admission does not establish whole-process quiescence while the existing GUI runs.

The Operator subsequently approved immediate termination of the failed application process and
a separate report process that retains only the requested two actions, explicitly requiring
restraint against runaway complexity. The accepted timing change is captured by the
[feature](../features/crash-reporting/design.md) and
[system](../systems/crash-reporting/design.md); no in-process recovery repair remains authorized.
Keeping the failed process alive instead requires a separately justified execution-isolation
design; freezing arbitrary threads is not an established substitute. External effects already
dispatched before failure cannot be undone by either reporting choice.

The target docs distinguish ordinary returned-error recovery from fatal panic reporting. The
current `crates/beryl/src/main.rs` remains a target-bootstrap `compile_error!` placeholder, so any
end-to-end mount must account for that existing rework boundary rather than claim a component test
delivers the GUI. The [Windows/Rust assessment](../memory/topic/windows-fatal-reporting/independent-report-lifetime.md)
records inherited-job, channel-lifetime and panic-hook limits. No production code or tests were
changed for the architecture assessment.
