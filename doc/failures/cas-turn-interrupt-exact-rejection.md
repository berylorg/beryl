# Scope

Pinned Codex App Server 0.144.1 `turn/interrupt` rejection and exact stop
reopening.

# Invalidated Approach

Phase 64 initially treated a machine-readable exact-target rejection as proof
that interruption was not dispatched and that the same selected turn remained
live. The proposed stop protocol would consume the durable stop record and
reopen active steering from that provider response.

# Decisive Evidence

Pinned source inspection is retained in
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/turn-interrupt-semantics.md`.

The handler returns correlated `-32600` without `data` for absent or unloaded
threads, already terminal or non-running threads, and active-turn mismatch.
Its handler-local `-32603` submission failure also enqueues no core interrupt.
Neither response supplies a structured cause or proves that the requested turn
remains current. Diagnostic text is the only distinction and cannot be
authority.

After its precheck, app-server submits an untargeted core `Op::Interrupt`.
Pending responses are thread-scoped and may be drained by any later terminal
event. The provider primitive is therefore exact only while Beryl independently
prevents a successor operation from entering the managed thread across the
request cut.

# Why It Failed

No-core-enqueue evidence and current-target evidence are separate facts.
Treating the former as the latter could reopen steering against an absent,
terminal, or different operation. It could also claim exact child-turn
interruption where Beryl cannot fence internally scheduled child successors.

# Course Correction

Only a local proof that every request byte was prevented, combined with
unchanged exact target authority, may safely reopen the stop gate. A pinned
handler rejection is normalized as no-core-interrupt evidence without a target
verdict; absent an already observed matching terminal, Beryl retires the
uncertain projection and converges through durable stop abandonment.

Selected parent interruption additionally requires an exclusive authenticated
managed listener and a no-successor target-operation fence. Child or subagent
turn interruption is unsupported on CAS 0.144.1. Individual turn-process
termination is also unsupported because its reusable numeric identity is not
exact. Coarse thread cleanup is admitted only from pinned source evidence plus
negotiated experimental capability; Beryl never sends that destructive
request merely to probe support.

# Affected Authority

Phase 64 reconciles the CAS-live, conversation-history, backend-runtime,
status-line, main-window, Syndic storage, backend package, and app package
designs. Phase 65 and later implementation must preserve the release-scoped
rejection classifier, target-abandonment disposition, and no-successor proof.
