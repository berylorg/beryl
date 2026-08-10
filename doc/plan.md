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

# Phase 95: Prove Adversarial Quarantine Topology Rejection (finished)

A crate-private test-only sealed-checkout payload now moves real secondary quarantine owners into
extra and foreign-cut topologies, or ordinarily retires the real retained core, before unchanged
production validation. Each malformed complete set rejects before parking, consumes every reached
owner and both inventories into one nonretryable inert attempt, disarms re-escrow, remains
nonexecuting and unpublished, and settles each core under its exact cut through explicit consuming
disposition. Production construction, validation, and publication remain unchanged.

Formatting, diff hygiene, both library checks, focused `test-faults` Phase 95 3/3, complete default
225/225, and complete `test-faults` 423/423 serialized suites passed. Static boundary verification
and fresh independent architectural completion review passed with no findings.

# Phase 96: Settle The Old-Epoch Command Frontier Before Adoption (wip)

Pause one already admitted, not-yet-dispatched old-epoch command before the next stable-driver
cycle, close its exact persistent-failure command frontier, and prove the driver explicitly returns
one typed cut-correlated nondispatch completion before it parks. The owning scheduler worker must
surrender and join before recovery inventory sealing and adoption, so no live command crosses the
service-epoch boundary.

Implementation tasks:

- Give every stable-driver command one consuming rejection path alongside execution. Exact
  gate-close drain and the defensive admitting-epoch dequeue mismatch must both complete through
  that path rather than silently dropping the operation closure.
- Carry the exact persistent-failure cut through the rejection and classify only that typed cause as
  scheduled-worker persistent-failure surrender. Other worker loss remains fatal and cannot borrow
  the cut's authority.
- Add a deterministic test seam before stable-slot cycle acquisition, enqueue one real scheduled
  ordinary command, close its exact failure frontier, and release the driver so rejection unblocks
  scheduler quiescence and inventory sealing.
- Continue the same fixed owner set through successful stable-core adoption behind the closed
  replacement startup fence, proving the rejected command and old scheduler do not cross adoption.

Edge cases and negative evidence:

- The command is fully admitted and queued before gate closure but has issued no provider request;
  a pre-admission command or already-dispatched operation does not satisfy this phase.
- Rejection settles exactly once under gate-close drain. Defensive epoch-mismatch rejection uses the
  same consuming completion, but the test does not manufacture a post-adoption live command.
- The replacement startup gate stays closed throughout the proof, with no service publication,
  replacement scheduler pass, ingester start, or stable-driver release.
- Observe zero provider requests for the rejected command, zero new-epoch execution, zero
  publication, and no retry wake or durable lifecycle successor caused by rejection.
- Preserve the immutable accepted-input receipt, pending turn, registry token, stable-core,
  connection, and worker ownership through scheduler surrender, adoption, and explicit consuming
  attempt disposition; do not use cancellation or Drop as join proof.

Verification: formatting and diff hygiene; both serialized `beryl-app` library checks; a focused
`test-faults` Phase 96 test; complete serialized default and `test-faults` library suites; static
boundary inspection; exact process and task-owned residue check; fresh independent architectural
completion review. Performance and stress work remain outside this phase and require the Operator's
AC-power gate.

Resumable milestone: the typed consuming command-rejection boundary, matching-cut scheduler
surrender, deterministic pre-cycle pause, and real-owner Phase 96 proof are implemented and
formatted. The focused proof reaches exactly one queued command with zero provider dispatch,
closes and drains its exact frontier, joins the scheduler, seals one connection and one retained
candidate, recovers the same home, preserves durable route, gate, loaded-registry, and stable-core
identity, and successfully adopts behind the closed replacement startup fence.

Blocking finding: the proof then exposes an architectural gap in the explicit whole-attempt
recovery-failure disposition. `AdoptedUnpublishedProjectionConnectionService::
dispose_after_recovery_failure` makes the attempt inert and shuts down the replacement service, but
it disarms and drops the old retained inventory without consuming an old-epoch retirement path that
calls `shutdown_old_service_epoch`. The old scheduled-execution provider therefore misses its
required explicit `shutdown()` boundary. Do not weaken the test or rely on provider `Drop`. Resume
only after the Operator chooses an owning, exactly-once old-inventory terminal-retirement design;
then require old-provider shutdown count zero before disposition and exactly one after disposition,
rerun the focused proof, and continue the remaining Phase 96 verification and independent review.

# Phase 97: Converge Startup And Seal Candidate Authority (pending)

Converge recovered durable startup state behind the closed startup gate, reauthenticate every
original quarantined candidate against that stable result, explicitly dispose ineligible
candidates, and seal one exact ledger. Consume the old-cut adoption fence into its retirement
witness only after all old publication sources retire. No retry, disposition, rejected lease or
registry token, rejected worker hold, or connection-quarantine owner may cross the sealed boundary;
accepted dormant provenance retains its exact stable lease and registry token.

# Phase 98: Publish Recovery Atomically And Prove Stable-Core Reuse (pending)

Acquire every stable-connection authority and retirement gate in stable order and hold the complete
set continuously across validation, current-service installation, and startup-gate opening. Arm all
new-ingester and stable-driver tokens on the same closed gate, atomically install the current service
while opening it, then reconstruct executable projection wrappers only from successfully published
dormant provenance and restart complete input preparation. Test both stable-core race orders,
terminal whole-attempt disposition, and two sequential recovery cycles that reuse each exact stable
connection and capacity-one adoption-control slot under strictly newer cuts without changing
transport, process-fact, loaded-session, or lease identity.

# Phase 99: Verify And Close Checkpoint 3 (pending)

Run the complete storage, protocol, concurrency, restart, risk-bound stress, static-boundary, and
independent architectural completion gates. Reconcile package API docs and compact the rework
tracker only after no finding requires changing Checkpoint 3 architecture. Run functional gates
first; perform risk-bound stress separately only after the Operator confirms the laptop is on AC
power. Treat that stress as correctness evidence for configured bounds, not as performance tuning.
