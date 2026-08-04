# Scope

Recovered CAS projection authority after a successful one-time `thread/inject_items` call.

# Invalidated Approach

Beryl treated the complete loaded process/thread generation that performed recovery injection as a
permanent eligibility requirement. That also prohibited an exact fresh-connection capture handoff
while the original same-process CAS subscription still anchored the in-memory thread.

# Evidence

Pinned CAS 0.144.1 source establishes a narrower, stronger fact: while the original subscription
remains live, `thread/resume` from a new connection in the same managed process joins the exact
existing in-memory `CodexThread` before any rollout reconstruction. A focused 2026-07-17 Phase 8
rerun also passed all 59 normal-path checks: after process restart, the same CAS thread's provider
request contained the injected user and assistant records and both ordinary user turns exactly once
and in order.

The restart observation is not a transactional persistence acknowledgement. Pinned source also
shows that a lower rollout-append error can be logged without making injection fail, and public
thread reads do not expose injected raw messages for readback. Cold resume therefore remains
non-authorizing for recovered lineage even though the healthy path works.

The controlling evidence is
`doc/memory/topic/codex-app-server/thread-inject-items-0.144.1.md` and
`doc/rework/beryl-home/probes/cas-phase8-live.ps1`.

# Why It Failed

The full-generation equality rule conflated immutable injection-establishment provenance with the
loaded-thread generation of a later, source-proven capture connection. It made completion/live
narrative mismatch unrecoverable even though Beryl can overlap the old subscription with a new
same-process connection and thereby preserve the exact in-memory lineage.

Removing the entire equality rule would be equally invalid. It would treat process loss, last-anchor
loss, or cold rollout reconstruction as if CAS had acknowledged durable injected-history storage.

# Course Correction

`RecoveredInjectionProof` keeps the original injection generation as establishment provenance. Its
managed-process component remains mandatory. After narrative mismatch, Beryl quarantines the old
subscription as non-execution authority, resumes the same thread from a fresh connection in that
same process, publishes a new loaded-thread generation, and only then releases the old anchor.
`LoadedProjectionLease`, `LoadedCasProjection`, and the next active `ExecutionSnapshotRecord` carry
that current generation.

If the anchor or managed process is lost before handoff succeeds, the capability dies. Beryl does
not cold-resume recovered lineage. Fresh recovery still requires a complete eligible Syndic prefix,
which a narrative-mismatch path does not have.

The corrected authority lives in the CAS-live system, backend-recovery feature, package design,
rework tracker, and Phase 13 plan. Storage tests retain exact process-generation equality while
allowing a different loaded-thread generation; app tests must prove the overlapping subscription
handoff and reject cold/process-loss promotion.
