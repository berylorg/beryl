# Accepted Scheduler Generation Drift Is Service Retirement

## Scope

Phase 62 ordered next-turn scheduling across same-home recovery.

## Invalidated Approach

The first generation-drift test assumed that a scheduler worker paused before its promotion
reservation could observe a newer healthy Beryl-home generation, park, and leave the same
connection service ready for a later wake. A later implementation repeated the same assumption
by classifying every home-generation mismatch as expected scheduler drift.

## Evidence

Deterministic recovery at that cut left the accepted input unpromoted and sent no CAS request, but
the scheduler failed closed before the paused worker resumed. Its retained Syndic handle belonged
to the old store generation, as did the connection, same-thread flight, execution lease, and every
projection proof. A worker-specific invalidation barrier later proved that the promotion worker
must reach the same fail-closed classification even when another scheduler path retires the
service first.

## Why It Failed

Same-home recovery does not restore the prior generation. It publishes a strictly newer generation.
Parking the old scheduler for later reuse would therefore treat readiness as authority to relabel
old domain handles and remote-execution capabilities. That contradicts the existing explicit
cross-generation rebind contract.

## Required Course Correction

- Treat a newer healthy home generation as retirement of the entire old connection service.
- Distinguish a newer healthy generation from same-generation shutdown or unhealthy-state drift;
  do not classify all generation mismatches as parkable.
- Fence issuance, cancel and join old-generation workers, and report the obsolete scheduler
  fail-closed.
- Leave an unpromoted accepted input in its durable next-turn route without a CAS request.
- Let only explicit new-generation recovery reopen that work; no worker wake, timer, or ordinary
  readiness signal may resume the old service.

## Service Retirement Is Not Blanket Connection Destruction

Phase 76 reconnaissance invalidated using ordinary service shutdown as the running-session
retirement primitive. Ordinary shutdown closes every connection and the retained home. Persistent
store failure instead has two earlier authority-sensitive cuts: one guarded volatile exact
interruption for an eligible active turn, and preservation of the explicitly authorized
still-pending loaded projection whose unchanged lease may cross generations only through the
consuming rebind proof recorded in `doc/failures/cas-phase13-preflight-projection-consumption.md`.

The scheduler, workers, admissions, and all other old-generation service authority still retire.
The active-turn cut and pending-projection handoff must be implemented as explicit bounded
predecessors before a supervisor may replace the service without closing the home. Treating every
old connection as reusable would be just as invalid as destroying the one narrowly transferable
capability.

## Affected Authority

- `doc/plan.md` Phase 62
- `doc/plan.md` running-session same-home recovery work
- `doc/systems/cas-live-syndic-transcript/design.md`
- `crates/beryl-app/doc/design.md`

## Wrapper-Only Rebind And Singular Selection Are Also Invalid

Phase 78 reconnaissance found two further consequences of the same service-retirement rule. One
completed persistent-failure cut can retain several complete pre-activation projection wrappers,
because each already-admitted scheduled worker contributes its own bounded surrender child. The
failure cut has no causal fact that would authorize choosing one wrapper and discarding the other
still-live capabilities. Treating that bounded set as one implicitly selected projection would
therefore invent authority after the cut.

Changing only a retained wrapper's Beryl-home generation is also insufficient. Its exact
connection still carries the old service's command authorizer, event router, provider broker,
failure retainer, stop and compaction attachments, and worker ownership. The old master gate is
permanently closed. A wrapper relabeled onto the recovered home would consequently retain a live
lease but remain unable to register or publish through the replacement service.

The accepted correction is a consuming sequence with separate proof boundaries:

- A finished cut first becomes one inert recovery inventory, then one consuming grouped
  quarantine, while preserving the entire worker-bounded candidate set and every exact retained
  barrier. Neither boundary performs selection, recovered reads, backend requests, durable
  mutation, or old-gate reopening.
- A retained connection may cross services only through an explicit all-or-nothing service-epoch
  adoption that preserves stable connection, process, transport, loaded-session, registry, and
  lease identity while replacing or fencing every service-generation attachment.
- Only after adoption may bounded stable reads classify and rebind each quarantined pending
  projection. Rejection preserves the exact capability for explicit disposition; it never falls
  back to arbitrary selection or wrapper-only relabeling.

## Operational Fatality Is Not Ownership Quiescence

The first Phase 78 inventory implementation reused the accepted scheduler's single `fatal` bit as
its recovery-eligibility decision. That bit intentionally combines unrelated invariant failures
with ordinary reads rejected after the scheduler's exact Beryl-home generation has already failed.
The latter is a normal consequence of the persistent-failure cut: the old scheduler must stop, but
its successful join still proves that every child worker has settled before retention is sealed.

Recovery inventory therefore requires a typed scheduler exit. Exact failed-generation rejection is
cut-correlated quiescence and does not poison an otherwise stable inventory. A worker panic,
poisoned boundary, or unrelated fatal condition remains non-promotable and must retain its owning
inventory. Tests must force the exact read-versus-home-failure ordering; accepting either result as
a fixture race would erase the architectural distinction again.

The causal classification must travel with the rejecting operation. In particular,
`ProjectionCoordinatorError::HomeNotHealthy` must retain the coherent generation observed with its
health state. Sampling current home health later would allow recovery or another state transition to
relabel an old failure and is not an acceptable substitute for preserved provenance.

## Earlier Local Failure Must Dominate A Later Cut

The scheduler cannot infer a clean or cut-correlated exit merely because its command gate is
closed. An unrelated service-local fail-close may already have invalidated the gate before the
same home generation enters persistent failure. The later failure election does not erase that
earlier evidence or make the retained service safe to promote.

Gate inspection for recovery must therefore preserve the combined state: poison or an earlier
local failure is fatal, a failure-observed or persistent-failure election without local failure is
expected cut quiescence, and ordinary shutdown is clean. A conservative Boolean `is_open` or
`is_persistent_failure_cut` query is insufficient for this ownership decision.

The local-failure fact is sticky on either side of election. A later local fatality does not replace
or erase an already elected persistent cut, because that cut still owns the recovery handoff, but it
does make the joined scheduler and resulting inventory non-promotable.

Scheduler exit is not the last causal observation. Inventory conversion must reconcile the exact
retained gate after joining the thread, because a local fatality or gate poison can arise after the
scheduler records a cut-correlated exit but before the retention seal.

## Recovering A Poisoned Guard Does Not Prove Stable Ownership

The first inventory cut used poisoned-lock recovery for scheduler ownership and retained-capability
state. Recovering the inner value is appropriate when the only goal is to keep exact resources
owned and joinable, but it does not prove that the interrupted mutation reached a coherent
boundary. Treating a subsequently clean join or readable vector as promotable would turn panic
recovery into invented stability evidence.

Inventory conversion may recover a poisoned guard only to preserve and quiesce its owners. A
poisoned scheduler-owner boundary, coordinator-retention boundary, connection registry, or escrow
checkout remains explicit: it either preserves the original handoff or yields an owning sealed
non-promotable inventory. Successful best-effort cleanup never clears the poison fact.

## A Panicked Scheduler Parent Must Still Join Its Children

The first Phase 78 panic classification treated a failed join of the scheduler-main thread as
sufficient evidence for an owning non-promotable inventory. That preserved the parent failure but
not the child-worker ownership proof. The scheduler runtime owned its child `JoinHandle` values;
unwinding and dropping that runtime detached every still-running child. Normal shutdown also moved
all handles into a temporary iterator, so a later bookkeeping panic could detach the unvisited
workers. Retention could then seal while one of those workers still held or could still surrender a
projection.

Panic containment must keep the runtime outside the scheduler-main unwind frame. Child handles and
their kind metadata belong in one preallocated runtime-owned record set, and an unjoined handle
must remain there until immediately before its non-panicking join. Any owner-side launch barrier
must survive the unwind and open during outer containment, after sticky local failure and both
cancellation families are established, so a child cannot remain parked behind preparation
performed by the panicked parent. Containment then joins every remaining child without applying
fallible scheduler disposition bookkeeping. Only afterward may it resume the parent unwind so
inventory conversion classifies `SchedulerPanicked` and seals the complete retained set.

Deterministic coverage must pause a scheduler-owned child after the runtime gives it a genuine
loaded projection and worker admission, panic scheduler main, and prove that inventory conversion
cannot seal or return until that child is released and joined. The child's retained projection must
appear in the sealed counts; merely observing the parent panic or a non-promotable flag is not an
ownership proof.
