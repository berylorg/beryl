# CAS Phase 63 Terminal Is Not Recovery-Complete

## Invalidated Assumption

Treat every proven terminal turn either as fully recovery-complete or as wholly unusable for a
fresh projection.

## Evidence

Phase 63 restart convergence correctly changes a possibly dispatched predecessor to the exact
source-less `Incomplete(AuthorityLost)` outcome before promoting queued work. The predecessor's
sealed user input can still be nonempty, fully finalized, free of provider issues, and exactly
representable. Blanket incomplete-prefix rejection then prevents the distinct pending successor
from establishing any CAS projection, while omitting the predecessor would lose context and
replaying it would duplicate uncertain work.

Terminal publication alone also does not finalize already captured provider items. Skipping the
existing bounded terminal-history convergence can therefore leave an otherwise exact echoed user
message captured but ineligible, even though live terminal handling would finalize the same
evidence.

The first Phase 63 implementation called terminal publication and history convergence in sequence
while terminal publication atomically changed the sole input-gate recovery source to idle. A crash
between those calls left no startup-discoverable work. Scanning idle state later is not equivalent:
a successor can replace the committed tail, and legitimately incomplete, open, or pending-resource
history cannot distinguish a completed convergence pass from an interrupted one.

The first gate validator then made the opposite temporal mistake: it required a stale binding's
abandoned predecessor to remain the current `FinalizingHistory` gate forever. Exact completion
therefore produced a valid idle gate that failed reopen validation, and a successor could never
legitimately replace that old current-gate state.

A temporary follow-up also let the parallel active-steering worker consume the history obligation
after it won source-less target-loss publication. That worker owns no same-thread projection
flight, while target invalidation already wakes the ordinary capture owner that does. Both workers
could race through item/transcript convergence, duplicate the scheduler wake, and make the loser
reject an already released gate as no longer finalizing.

The single-owner regression then paused capture after transcript convergence but before gate
release and admitted one valid queued input. Admission advanced the broad thread, summary, and
finalizing gate without changing the selected tail, digest, or completed transcript. Completion
nevertheless required the summary revision to equal the older build source revision and rejected
the valid descendant. The older path-neutral admission rule was also unsound for an active build:
it superseded and restarted unchanged selected-history work, so repeated queued sends could starve
terminal finalization.

## Correction

Keep general recovery-complete eligibility strict. The pending-parent recovery scope alone may
authenticate its immediate predecessor as authority-lost tail context when the latest source event
is the matching source-less terminal, every capture frontier is complete and unblocked, every item
passes the ordinary recovery proof, and all earlier ancestors remain recovery-complete. Injection
loads only that durable sequence as context; it does not alter the predecessor's incomplete
outcome or issue its old `turn/start`.

Every ordinary terminal publication instead enters a durable `FinalizingHistory(turn)` input-gate
state in the same commit. The existing bounded ordinary terminal-history pipeline resumes from that
source and may finalize only already captured complete evidence. One exact completion mutation
proves the same terminal turn, current selected transcript, and an explicit item-convergence fixed
point before changing the gate to idle. Missing, open, blocking, or otherwise unresolved history
remains ineligible even when it is a valid fixed point.

Validation distinguishes the current obligation from its completed historical cause. While the
abandoned turn remains the committed tail, `FinalizingHistory(turn)` proves pending work and an idle
gate is accepted only by re-proving the exact terminal-history fixed point. Once the committed tail
advances, the abandoned terminal is historical and ordinary ordering validates the newer current
gate. A historical binding must not pin a replaceable current head to an obsolete workflow phase.

Keep one live convergence owner. Active steering ends after loss publication and target
invalidation. Ordinary capture retains the same-thread flight, observes the resulting closed target
or proven terminal, and consumes `FinalizingHistory`; startup recovery takes over only after process
loss. A non-polling steering capability never becomes an alternate history worker.

Treat transcript source authority semantically. The committed tail plus selected-path digest are
the build identity, while the captured broad thread revision is a monotonic lower bound.
Path-neutral draft rotation and queued admission preserve active and completed builds, advance the
history summary to the current thread revision, and preserve its derived completeness. Only an
actual selected-path or canonical source change supersedes the build.

Treat the completion request's observed finalizing gate as a lower-bound proof as well. At writer
serialization, completion may consume the current compatible finalizing descendant created by
queued admissions, provided the thread, turn, selected route, zero-steering fact, and monotonic
accounting remain exact. It releases only that current gate and preserves all concurrently admitted
route state. Ambiguous-result reconciliation applies the same descendant relation.

This gate protocol applies to normal provider terminals and source-less loss convergence. A
secondary recovery queue or a best-effort idle scan would create competing authority and is not an
acceptable repair.

The same distinction applies to the later exact terminal-repair path. An explicit incomplete repair
disposition is terminal but not presentation-finalized: it enters `FinalizingHistory`, selects no
replacement snapshot, rebuilds and publishes one coherent incomplete transcript generation, and
only then releases the repair-required gate. Releasing directly from the terminal disposition would
repeat the invalid assumption that terminal authority is already recovery-complete.

## Test-Fault Boundary

An older submitted-input test simulated post-dispatch source revision drift and read failure by
overwriting or deleting the turn's sealed composer manifest through an intentionally inconsistent
fixture batch. Once terminal-history convergence correctly validated that same durable backing,
the test no longer modeled a transient source failure: it modeled database corruption, which must
leave `FinalizingHistory` unreleased.

Post-dispatch source failures are now injected once at the request-local page handoff after a
successful durable page read. This preserves the typed completion-unknown transport cause without
mutating Syndic authority. Raw missing or mismatched sealed backing remains a distinct fail-closed
corruption case and must never be used as a substitute for a transient broker or source fault.

## Affected Authority And Proof

The CAS-live system contract owns the shared distinction. Backend-runtime-recovery owns its visible
consequence, package docs own their local validation and orchestration boundaries, and Phase 63
must prove rejection of every broader incomplete case plus one fresh injection followed by exactly
one successor start.
