# Discussion Context Execution Qualification

## Status And Boundary

Accepted on 2026-10-20 after independent complete-boundary source and evidence review.
This boundary implements the [accepted context authority](discussion-context-projection-readiness.md)
through ordinary discussion execution. Discussion status/Retry mounting and complete discussion
handoff product qualification remain separate.

The implementation follows the [CAS context contract](../systems/cas-live-syndic-transcript/design.md#context-aware-lineage),
[Syndic history authority](../../crates/syndic-storage/doc/design-history-storage.md#exact-source-planning-without-input-admission),
[V7 extension](../../crates/syndic-storage/doc/design-schema-v7.md#discussion-context-projection-extension)
and [App establishment authority](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md#discussion-context-establishment).
The Home metadata additions follow the owning [domain API](../../crates/beryl-home-store/doc/design-domain-api.md)
and [opening/recovery contract](../../crates/beryl-home-store/doc/design-open-and-recovery.md).

## Qualified Behavior

The ordinary execution path seeds the exact immutable selected passage as one assistant frame
before the first local input. Same-thread continuation, same-process resume and inherited native
forks retain the original context establishment without resending it. Actual second-input execution
keeps the original proof, loaded generation and ordering floor while advancing represented history.
The current binding-head fence and historical source reservation remain independently authenticated.

Selected immutable turn crossings govern nested contexts and independent descendants without an
owned envelope. Replacement retains the original off-path owner. Parent divergence, excluded
suffixes and equal boundaries preserve exact inclusion and order. Rich native prefixes remain
eligible independently of text replay. Fresh recovery interleaves every required frame with the
complete canonical prefix and excludes the pending input.

Flat tag2 proofs use bounded codecs and fixed commitments; ordinary tags0/1 and recovery V1 retain
their exact golden bytes. Explicit validation checks the actual Home identity and recomputes
context commitments and complete replay counts, bytes and digest. Consistently rehashed foreign
Home proofs, substituted provenance and structurally decodable observation corruption refuse.
No record family, schema version, canonical context item or delivery journal is added.

Ambiguous Unattempted and source-less terminal provider cuts require bounded package-issued
classification, including EMPTY. Ordinary publication preparation retains existing request fields
and writer checks; tag2/nonempty context cannot use that helper. Preparations carry the existing
opaque store-instance-plus-generation identity, rejecting reuse after close/reopen even when the
numeric generation repeats. Domain readers use the validated owning Home header in normal,
provisional, proof and recovery paths without additional I/O or persisted fields.

The 65,536-byte multibyte selection produces a 65,997-byte frame spanning pages with one leased
page. Native admission charges only the missing seed; complete replay charges all expanded bytes
and stops at its byte/model bound. Cancellation, rejection, lost completion, clock failures,
ordering-floor refusal and uncertain publication preserve original failure and reconciliation
custody. Successful-but-unpublished targets retire without authorizing reuse or reinjection;
clock/proof failure retains only known native-cut provenance. Concurrent context reads preserve
the existing ordinary retained-execution conflict retry.

## Verification Receipts And Reuse

Verification used the canonical verification checkout, locked dependencies, Cargo jobs1,
incremental=false, opt-level0, debug0 and the configured rust-lld linker. Nextest used eight
workers except serial App protocol runs, a 30-second slow interval and two-interval termination.
The 32-MiB Rust stack setting applied only to test processes. The normal production check cleared
that setting, the backtrace setting and the retained-fixture ledger environment.

- HomeStore: 27/27 across `domain_reader_identity`, `generation_identity`, `domain_type_identity`,
  `candidate_recovery_access`, `proof_composition` and `exhaustive_domain_validation`.
  Run `35811c17-7073-472a-be5e-04df8c433752`, 1.019 seconds. Five new cases qualify reader/candidate
  metadata, actual Home identity, admission refusal, publication equality and recovery.
- Backend: 14/14 across `thread_injection_session` and `thread_injection_source`, with
  `lifecycle-test-support`. Run `5e4019bf-2a93-45f3-87db-6fbcca09574a`, 0.056 seconds.
- Syndic: 132 distinct passing cases across `discussion_context_projection`, `lineage_proofs`,
  `binding_invariants`, `binding_mutations`, `native_projection`, `recovery_projection`,
  `recovery_projection_faults`, `discussion_creation`, `discussion_source` and `context_temporal`,
  with `test-faults`. Combined run `212f847a-d2d3-49dc-8fed-ba45948ee0c7` passed 131/132 in
  93.913 seconds, including all 21 owning context cases. Its sole failure was a fixture request
  using a different execution identity. After correcting only that fixture to use the actual
  inherited execution, the complete affected `native_projection` target passed 21/21 in run
  `545a3574-f7b4-4c61-bcaf-71492e1ef7bd`, 11.893 seconds. The union qualifies all 132; it is not
  reported as one wholly passing combined invocation.
- App: 135 distinct passing cases, with `test-faults`, using scoped reuse and corrected repeats.
  The 121-case regression run `2b735d15-0313-42f8-bcd3-cd5aeb75e361` passed 115 in 387.381 seconds
  across `recovery_page_residency`, `loaded_thread_metadata`, `normal_terminal`,
  `native_lineage_scheduler`, `runtime_session_preparation` and `recovery_history_stress`.
  Its six failed cases and the retained-preflight control all repeated successfully: coalesced
  native work, retained preflight, permission custody and all four history cases passed in the seven-case selection of
  run `ae801845-20c8-45ee-9f72-b684f2b8ce83`. That run passed 17/19 in 68.249 seconds; its two
  remaining failures were confined to the context fixture being corrected.
- The 12 owning App context cases qualify by union. Run `acd0bb5b-a270-438f-873e-424a1240fccf`
  passed 11/12 in 21.463 seconds, including the real successful-seed ordering-floor refusal.
  Its remaining fixture used a pre-completion timestamp for the inherited child submission.
  After advancing that fixture from the completed source's durable activity timestamp, the exact
  first/same-thread-second/inherited execution case passed in run
  `6be2da75-bb50-4274-a231-dd74b6e755ac`, 2.002 seconds. The other eleven cases do not use the
  changed continuation helper.
- Two selected App library cases passed in run `87e6f15b-439d-4809-a11a-ffb791fa3047`: canonical
  content removal during preflight and original publication acquisition/reconciliation custody.
  Only those selected library cases are claimed; the other library cases were skipped.
- Normal `cargo check -p beryl --bin beryl --locked` passed in 25.54 seconds on the final production
  source, checking the executable and affected App, Syndic and HomeStore libraries with normal
  features. Existing warnings remain. Scoped formatting and `git diff --check` pass.

The accepted union is 308 distinct cases across these four packages. Repeats are not added to
that count. Reused positive paths retain their effective inputs; later production corrections
were confined to context reservation correlation and exact concurrent-change classification,
whose owning and affected consumer cases were repeated. Later fixture-only corrections repeated
their affected target or case. No failing receipt is presented as a wholly passing run.

## Review And Evidence Limits

Independent source reviews cover the complete cross-package context boundary, public metadata
and EMPTY preparation APIs, framing/codec and replay validation, current and historical source
fences, continuation, failure classes and cleanup/reconciliation custody. Material findings were
corrected and owning evidence repeated. The [failure record](../failures/discussion-context-execution.md)
preserves the invalidated assumptions and fixture lessons.

The backend evidence uses deterministic local protocol peers and the existing typed adapter.
The [pinned CAS source evidence](../memory/github.com/berylorg/codex-fork/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/assistant-context-injection.md)
remains source evidence. No provider/model run, provider idempotency/readback, GUI paint,
sustained performance, global RSS bound or universal worker-stack qualification is claimed.
Injected clock errors exercise typed failure custody; the future-floor fixture supplies a valid
prepared source timestamp and tests the actual later injection/refusal, without changing the OS
clock or claiming an earlier live injection.

Independent final reconciliation confirmed all 308 distinct passing identities and the scoped
reuse above, with no remaining blocking guarantee. All 83 changed/new source and test files matched
the canonical verification checkout. After the last acceptance consumer finished, the exact
agent-owned `.aipm/tasks/discussion-context-execution/` scope was removed with `cleanup-dir.exe`
and verified absent. No retained-fixture ledger was produced, and no task Cargo, compiler or
managed-runtime fixture process remained. Shared checkouts/build output and unrelated work remain.
