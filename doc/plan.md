# Scope

Replace Beryl's exact Codex App Server target with 0.146.0, then resume the next bounded slice of
Checkpoint 3 in the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`.

The replacement is one atomic compatibility cutover. Beryl does not carry a dual-version protocol
branch, imitate CAS collaboration through a Beryl dynamic tool, or select a weaker fallback when
the configured runtime is not the exact target. CAS-native `spawn_agent` remains the collaboration
authority and exposes optional model and reasoning-effort selection to the orchestrating model.

The Operator replaced exact process-wide allocation accounting with the risk-based contract in
`doc/systems/bounded-resource-dataflow/design.md`. Hard bounds belong at bulk payload, expansion,
accumulation, queue, cache, concurrency, media, and renderer boundaries that can realistically
consume substantial resources. Ordinary paths, structs, dependency bookkeeping, and returned
values do not require universal capabilities, structural-slot accounting, or exact residency
leases.

Before any GUI implementation begins, stop and obtain the existing Operator gate. Preceding
documentation, storage, backend, and service work may continue.

Functional compilation and correctness tests are separate from performance, stress, benchmark,
and optimization work. Run functional gates normally. Before any performance-oriented or sustained
stress measurement, stop and coordinate with the Operator so the laptop can remain connected to
AC power. This rework must implement and functionally verify required hard resource bounds, but it
does not own performance optimization or tuning beyond correctness; defer that work until the new
Beryl is functionally stable and can be iterated on the Operator's tower PC.

# Phase 85: Cut Compatibility Atomically To CAS 0.146.0 (finished)

Cut Beryl atomically to exact CAS 0.146.0 across runtime admission, generated protocol boundaries,
retained evidence, fixtures, probes, and normalized parsing. Production now owns an exact managed
Host or WSL launch, authenticated loopback transport, strict configuration, token cleanup, process
boundary, and non-forgeable launch provenance. Effective `multi_agent_v2` SessionFlags prove native
`spawn_agent` with optional model and reasoning selection under all four resolution outcomes. The
additive command and thread fields are covered without a second-version branch, compatibility
adapter, external-process attachment, or Beryl-owned spawn imitation.

## Resumable Milestone

Backend package gates passed 36/36 by default and 265/265 with `lifecycle-test-support`; app library
gates passed 220/220 by default and 389/389 with `test-faults`. Production and test-feature checks,
formatting, diff hygiene, source-size and static authority scans, and a fresh independent completion
review all passed. Corrected persistence-fault and token-cleanup behavior is retained in
`doc/failures/active-steering-after-persist-is-health-verification.md` and
`doc/failures/cas-phase85-token-cleanup-after-process-exit.md`.

# Phase 86: Finish The Sole Same-Generation Verification Flight (wip)

Finish the mounted scheduler and provider-worker join on one supervisor-owned verification flight.
Exact same-generation verification success preserves the pointer-identical current service,
publishes `VerifiedCurrent` to every provider waiter under exact process-slot validation, and only
then issues the dedicated scheduler resume wake. Failure, stale completion, and shutdown publish
their typed provider completion before service withdrawal or command-permit drain. No provider or
scheduler worker verifies the home independently, polls, drops its existing frontier, or reuses the
scheduler's consumable wake.

## Tasks

- Run exactly one same-generation verification for a `verifying` generation. Verification success
  preserves the pointer-identical current service and resets no service epoch. Capture the exact
  completed-flight snapshot so a newly started next flight cannot make prior completion appear
  stale.
- Treat exact same-home, same-generation `verifying` scheduler observations as a typed nonterminal
  pause. They signal or join the supervisor flight, retain or safely restart bounded scan work, and
  neither close the master gate nor poll. Successful verification validates the exact current slot
  epoch before issuing a dedicated scheduler-resume wake that reopens every applicable lane without
  granting ordinary retry eligibility; a stale completion or failed verification cannot wake a
  replacement epoch. Nested replay, projection, admission, and publication errors preserve their
  typed health cause through worker settlement; generic strings and later health resampling cannot
  reconstruct this disposition.
- Give mounted service-epoch workers the same sole-verifier boundary. A provider staging or frame-
  build committer that encounters an ambiguous command registers against the exact notification
  flight, signals or joins it, and waits without polling or holding a gate or process-slot lock.
  The supervisor publishes one monotonic exact-epoch completion to all registered waiters before a
  failure cut can drain their live-command permits. Healthy completion preserves the caller's exact
  batch or build frontier for the existing point-read reconciliation; failed, stale, or shutdown
  completion returns typed authority loss. Exact pre-command `verifying` and a new `verifying`
  observation during the reconciliation read join the corresponding flight without dropping that
  frontier. These workers never call home verification themselves and never reuse the scheduler's
  consumable resume bit.
- Under exact process-slot validation, publish `VerifiedCurrent` to the completed flight before
  issuing the scheduler wake. Elect persistent failure before publishing failed or stale completion
  and before service withdrawal or command-permit drain; publish shutdown or unavailable before
  terminal or ordinary drain.
- Reconcile focused notification, supervisor, scheduler, provider-staging, and frame-build tests,
  including the full checked-user producer path and completion-before-drain race.

## Edge Cases

- Same-generation verification success while an initial or active scheduler scan observes
  `verifying`; verification failure; stale verification completion; direct structural failure; and
  joined duplicate scheduler signals.
- Same-generation verification completion before versus after provider-waiter registration,
  multiple scheduler and provider joiners on one flight, provider shutdown while joined, healthy
  exact-batch or frame-build reconciliation, and failed or stale completion before command drain.
- A next verification flight begins before all prior waiters consume completion; the completed
  flight's snapshot remains authoritative and cannot be reclassified from mutable current state.

## Verification

- Add focused tests for verified completion before scheduler wake; elected failure before
  withdrawal/drain; stale and shutdown completion; provider staging and frame-build join/resume
  without a second verifier or missed completion; exact-frontier reconciliation; and chained
  verification epochs.
- Run focused and full `beryl-app` library suites sequentially with and without `test-faults`; use
  `cargo nextest`, `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=1`, and `-j 1`.
- Run production and `test-faults` library checks, formatting, diff whitespace, forbidden archived-
  source/API scans, and an independent completion review before finishing.

## Resumable Milestone

Authority and existing seams are mapped. No direct service replacement, wrapper-only rebind,
ordinary-startup constructor, or compatibility mount is permitted.

- The scheduler verification-pause implementation is complete and rustfmt/static checked. It
  includes exact nested `verifying`/`failed` provenance, late-worker wake coverage, all scheduler
  lanes, recovered-projection owner restoration, the exact service-slot resume wake, and a
  conservative notification accessor for poisoned-slot disposition.
- The notification/gate multi-waiter implementation, exact completed-flight snapshot, success
  publication-before-scheduler-wake path, typed stale/shutdown completion, and checked-user live-
  permit plumbing compile with `test-faults`; the pre-split focused selection passed 13/13.
- Static review found and correction closed two Major defects: failure election now publishes
  provider completion under the election lock before either recovery or cut-worker signalling, and
  one exact verification witness now covers checked-user activation, target, frame preparation, and
  initial frontier reads without replacing the source permit or prepared frame. The system and app
  package authority state both required orderings explicitly, and the corrected `test-faults`
  library check passes.
- Focused executions subsequently exposed and corrected stale completion-channel, recovery-cycle,
  and broker-acknowledgement expectations. The broker now releases every operation's outer drain-
  counted permit before publishing its acknowledgement, so a returned submit cannot still appear
  live at the public completion boundary. Formatting and diff hygiene pass after that correction.
- The suspected Windows Cargo build stall was a stale-slot lifecycle hang, now corrected by waking
  the cut worker after locked failed-or-stale publication and consuming immutable gate election
  instead of mutable home health. Focused verification passed 33/33, the default library passed
  220/220, and the `test-faults` library passed 393/393 with both checks, formatting, and diff
  hygiene clean. Fresh completion review accepted that correction but found four remaining Phase 86
  defects. Those defects are now repaired: waiter registration observes immutable completion while
  holding the flight lock; one pre-command witness spans provider dispatch and reconciliation;
  checked-user activation, frame, and final live-event publication preserve exact typed authority;
  and scheduler settlement classifies only carried error provenance. Scoped corrective reviews and
  focused regressions passed. The formerly intermittent same-generation supervisor regression now
  forces a deterministic next-turn health-gated scan and passes without restoring mutable-health
  inference. Final verification was safely interrupted after the focused cross-boundary gate passed
  and while the default full library suite was running; rerun that default suite, the full
  `test-faults` suite, both library checks, formatting, diff hygiene, and a fresh independent Phase 86
  completion review. A fresh default-suite rerun was stopped safely during compilation at the
  Operator's request before producing a test result; none of the remaining six gates completed, and
  no task-owned Cargo, nextest, rustc, or test process remains. Resume with the default full library
  suite, then the `test-faults` suite, both checks, formatting, diff hygiene, and the fresh completion
  review. The corrected environment diagnosis remains in
  `doc/failures/windows-cargo-nextest-build-stall.md`.

# Phase 87: Split Oversized Verification And Failure-Orchestration Sources (pending)

Split the oversized accepted-input scheduler, service, provider ingester, persistent-failure
coordinator, and directly coupled recovery/test files along their existing private ownership seams.
Preserve typestates, visibility, behavior, test placement, and public API shape. Pass formatting,
source-size, production and `test-faults` checks, focused and full nextest suites, and an independent
no-behavior-change review before resuming same-home recovery implementation.

# Phase 88: Reopen The Retained Same Home And Construct A Dormant Epoch (pending)

After persistent-failure election, withdraw the pointer-exact service through scoped lease drain,
consume its retained handoff without closing the home, and convert the stable inventory into exact
pending-projection quarantine. Run immediate and scheduled exact same-home forced reopen only on
`RecoveryRetrySchedule`'s 1, 2, 5, 10, and 30 second cadence. After a strictly newer healthy
generation publishes, reacquire complete Beryl and Syndic handle sets and construct one dormant
replacement service from a fresh provider-factory epoch. Shutdown during retry consumes all cut
ownership into nonpublishing terminal disposition while retaining the opened home.

# Phase 89: Adopt The Complete Stable-Connection Set (pending)

Consume the promotable quarantine and dormant replacement authority into one all-or-nothing
adopted-but-unpublished service. Reserve every replacement resource, park stable drivers, join old
ingesters, exchange the complete immutable connection set under the documented stable order, and
retain every old and new attachment in either the successful adoption owner or one inert terminal
owner. No adopted connection, ingester, driver, scheduler, provider observation, or backend command
may execute before publication.

# Phase 90: Converge Startup And Seal Candidate Authority (pending)

Converge recovered durable startup state behind the closed startup gate, reauthenticate every
original quarantined candidate against that stable result, explicitly dispose ineligible
candidates, and seal one exact ledger. Consume the old-cut adoption fence into its retirement
witness only after all old publication sources retire. No retry, disposition, rejected lease or
registry token, rejected worker hold, or connection-quarantine owner may cross the sealed boundary;
accepted dormant provenance retains its exact stable lease and registry token.

# Phase 91: Publish Recovery Atomically And Prove Stable-Core Reuse (pending)

Acquire every stable-connection authority and retirement gate in stable order and hold the complete
set continuously across validation, current-service installation, and startup-gate opening. Arm all
new-ingester and stable-driver tokens on the same closed gate, atomically install the current service
while opening it, then reconstruct executable projection wrappers only from successfully published
dormant provenance and restart complete input preparation. Test both stable-core race orders,
terminal whole-attempt disposition, and two sequential recovery cycles that reuse each exact stable
connection and capacity-one adoption-control slot under strictly newer cuts without changing
transport, process-fact, loaded-session, or lease identity.

# Phase 92: Verify And Close Checkpoint 3 (pending)

Run the complete storage, protocol, concurrency, restart, risk-bound stress, static-boundary, and
independent architectural completion gates. Reconcile package API docs and compact the rework
tracker only after no finding requires changing Checkpoint 3 architecture. Run functional gates
first; perform risk-bound stress separately only after the Operator confirms the laptop is on AC
power. Treat that stress as correctness evidence for configured bounds, not as performance tuning.
