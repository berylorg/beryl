# Reason For Investigation

Replacing CAS makes Beryl responsible for the agent loop and effects, not only HTTP decoding.
Determine what must be owned durably before dispatch, what a response proves, and how interruption,
storage failure and restart can preserve accepted input without silently repeating actions.

# Outcome

Direct function/custom/opaque/compaction continuations establish that Beryl can construct successive
subscription requests itself. They do not establish exactly-once inference, resumable streams,
remote cancellation, durable server retrieval or safe replay of arbitrary tools.

A candidate runtime can make local admission, context selection, tool intent and result custody
explicit in Beryl storage. The important simplification is one owner of accepted input and
execution, replacing synchronization with CAS-native thread state. Distributed effect uncertainty
remains and must be represented honestly. This note proposes architecture; production remains held.

## Baseline Requirements That Survive Replacement

Composer acceptance and draft clear are one durable outcome. An indeterminate mutation cannot
be turned into a second submission. Accepted fragments remain distinct, ordered and visible;
pending, dispatched and delivery-unknown are different states. A lost response does not authorize
resending the same fragment.

Current recovery authority exposes only exact soft interruption. It does not authorize a new
hard-stop button or arbitrary child termination. Replacement-owned local-tool cancellation and
process settlement need an explicit new implementation contract; they are not supplied by dropping
CAS. Preserve readable history/drafts and localized failure rather than changing runtime/root.

Lifecycle continuation intent is process-local until ordinary turn acceptance. Stop, shutdown and
accepted user input can defeat that intent; restart must not reconstruct it from a yield record.
Nonfinal window close does not own execution lifetime. Titles and other maintenance requests
remain separate purposes with their own instruction and notification eligibility.

## Candidate Durable Units

- **Accepted input fragment:** exact draft/input resources, immutable origin and acceptance
  identity, queue order and intended turn/steering eligibility. Existing admission reconciliation
  remains useful; HTTP delivery is a later fact.
- **Local turn:** owns accepted input, context lineage, selected execution environment, scheduling
  occupancy, stop state and eventual visible outcome. It may contain several model requests.
- **Model attempt:** exact account/model/dialect, immutable context/tool-schema/instruction snapshot,
  purpose, request identity, dispatch uncertainty, observed response ID, capture progress and
  semantic closure. A retry is another attempt, not replacement of historical observations.
- **Tool invocation:** request/item/call identity, exact completed arguments, selected handler and
  execution policy, durable dispatch intent, started/unknown/finished outcome and result resource.
  Its local identity must not rely on a tool name alone or reuse across different attempts.
- **Context revision:** references completed input/output/result resources and compaction lineage;
  published atomically after validation. It is distinct from rendered narrative and partial capture.

These units are a responsibility proposal, not a demand for five new universal frameworks.
Reuse existing bounded storage/jobs/brokers where their authoritative semantics fit. Do not force
these records through CAS projection/native-lineage types solely to preserve old code.

## Candidate Request And Effect Sequence

1. Reconcile accepted input and freeze the exact context, policy, tools and instruction snapshot
   for an eligible step. Hold the owning thread's dispatch authority, not a window's focus token.
2. Commit model-attempt intent before sending bytes. A separate durable dispatch-claim transition
   can distinguish prepared work from work that may have sent bytes, provided no transport starts
   before that transition is confirmed. After a crash, a claimed attempt remains possibly sent
   even if no response was recorded. A local write completion is not server acceptance.
3. Stream observations into owned resources and record actual completed items. Keep response
   closure separate: the directly observed terminal output array was empty.
4. Before admitting a tool effect, validate complete arguments, exact call identity, tool schema,
   permissions, current stop/authority state and durable storage health. A conservative first
   design can wait for successful response closure before dispatching any generated local calls.
   Starting a tool on item completion before response closure is a different latency/uncertainty
   tradeoff requiring explicit approval in design, not an assumed streaming requirement.
5. Commit tool intent, execute through its exact handler, and durably own the result before
   assembling another inference request. Parallel effects require bounded workers and separate
   per-call outcomes; preserve original output order when reconstructing context.
6. Publish the next context revision only after required calls/results and response closure are
   coherent. Determine local turn completion from the selected protocol's turn signal and pending
   work, not merely HTTP 200 or one `response.completed` event.

Input arriving during a model request cannot rewrite already-sent bytes. Candidate steering is
acceptance into a local ordered turn mailbox consumed at an eligible subsequent step. Interrupting
the current request and issuing a new request with that input has different quota, partial-output
and completion semantics. This choice must be reconciled with the current composer steering
contract, rather than labeled immediate service steering without evidence.

## Concrete Failure Cases

**Crash before request dispatch.** A durable intent can remain pending, but the implementation
must prove no bytes were sent before classifying it safe to dispatch. A generic intent record
alone cannot prove that after a crash; process-local evidence is gone. Admission and delivery
reconciliation need different boundaries.

**Connection lost after inference starts.** The request may have consumed quota or performed
hosted actions. Preserve the captured prefix and uncertainty. Full-context reissue may be a
deliberate new attempt, but it is not continuation of the same remote execution and must not be
an automatic replay of delivery-unknown user input under current authority.

**Tool call completed, terminal response lost.** Under the conservative sequence, no local tool
was executed yet. Preserve the call observation as incomplete response context and refuse an
automatic effect. A faster design that already dispatched it must retain that effect even though
the enclosing response never closed.

**Command changed a file, then Beryl crashed before result commit.** Re-running the command can
apply the change twice. An invocation UUID does not make arbitrary shell/file/network effects
idempotent. Mark the outcome unknown and require a tool-specific reconciliation or explicit later
action. Do not synthesize a successful result from the absence of a running process.

**Result committed, continuation response lost.** The stored tool result must never cause tool
re-execution. Reconstruct context from that result and any known completed prior items, while
retaining uncertainty about the later model attempt. Do not regenerate the result by replaying
the handler merely because a new transport was created.

**Stop races with dispatch.** One owner arbitrates stopping versus admission of the next request
or tool. Once stop wins, no new effect is admitted for that turn. Already-started local work needs
handler-specific cancellation and settlement; closing an HTTP stream alone does not prove that
remote generation stopped. Late results must be attributed to their original operation and cannot
restart lifecycle continuation or escape into a replacement turn.

**Durable store fails while work runs.** Freeze further effect/request dispatch and cancel or
settle owned work under bounded policies. Do not let successful network delivery imply successful
capture. If the result cannot be persisted, keep an explicit incomplete/unknown outcome when
storage authority returns; do not recover by accumulating an unbounded memory/disk backlog.

**Two parallel tools finish out of order.** The first durable completion cannot stand in for both.
Join exact call IDs in request order for context; a failed/unknown sibling keeps its own status.
Cancellation and resource release must handle each worker without duplicating another's result.

**Compaction finishes after accepted input or stop changes eligibility.** Keep the exact old
context revision usable until replacement commits. A completed compaction response is evidence
for its captured context, not authority to replace a newer one or start a continuation. Compare
revision and stop/lifecycle generation at publication.

## Retry, Recovery And Cancellation Limits

No direct probe established subscription idempotency-key semantics, response retrieval with
`store:false`, stream replay across connections or server cancellation acknowledgement. Do not
depend on those facilities for correctness. Model/request IDs are correlation values until a
stronger service contract is observed.

Explicit full context already works for small cases and can serve as the baseline input mechanism.
WebSocket previous-response reuse, if adopted later, should be an optimization whose loss has a
defined context path; it must not be Beryl's only durable memory. A network reconnect does not
prove that a possibly sent request was not executed.

Separate automatic retries by purpose and failure certainty. Proven pre-dispatch failures can be
handled without claiming remote execution. After possible dispatch, conservative current-product
semantics preserve uncertainty and require an explicit new action unless an owning design proves
safe deduplication. Under current recovery authority, a later user-requested retry or continuation
is a new durable submission; the prior incomplete turn and any effects remain unchanged.
An image/search request can have cost or server state even when its result is
lost. Local tools with external effects need stronger handling than a title-generation attempt.

Restart reads durable ownership and published context; it does not automatically resume arbitrary
in-flight effects. Already accepted distinct queued input can proceed only when predecessor context
and exact binding are eligible. Unaccepted lifecycle intent does not survive. Replacing CAS's
repair snapshots requires an explicit incomplete/recovery design: absent native replay must not
be disguised as complete history reconstructed from whatever was rendered.

## Pinned Client Loop Evidence

Source inspection of Codex commit `6b9826e3aa83b1a5947db50f4332cb9c65f1b340`
provides implementation precedents, not additional direct service observations.
Its SSE decoder forwards optional `response.completed.end_turn`; the sampling loop treats an
explicit `false` as requiring another step. A true or absent value does not cancel a follow-up
already required by a dispatched tool or recoverable tool-call error. Pending input and stop hooks
also participate in loop termination. Consequently, neither a completed response nor assistant
message phase alone is a sufficient Beryl turn-completion predicate.

The client rejects stream EOF before completion. Its retry path can rebuild input from session
history and executed-tool context; this is client reconstruction, not proof of remote stream
resumption. Retry limits and transport fallback are policy: a WebSocket-to-HTTP switch can reset
the retry counter, and a feature-gated connection retry branch can retry indefinitely with capped
delays. Beryl should not inherit that policy as an exactly-once or bounded-recovery guarantee.

WebSocket incremental input requires matching non-input request properties and an exact prefix
relationship to previous input plus recorded output. The client also requires cached response
state and a nonempty response ID. Missing cache prevents incremental reuse; switching fallback
transport clears WebSocket state. These guards support treating connection state as an optional
optimization over explicit context, not as durable conversation ownership.

Cancellation interrupts local stream waiting and settles in-flight tool futures. Dropping a
WebSocket consumer discards the connection and aborts its pump. None of those source paths proves
that the subscription service acknowledged cancellation of generation or hosted effects.

Root inspection independently checked the turn follow-up predicate and incremental-input/cache
guards. The remaining client paths were read by the source investigator. No cancellation, crash,
retry or WebSocket service experiment was performed for this assessment.

## New Work, Reuse And Decision Gates

Retain exact accepted-input identity, Syndic history/resources, bounded queues, home-generation
health, app tool brokers, process-owned jobs and shutdown barriers. Remove candidates include CAS
turn/start/steer projection mirroring, native-session injection and late route normalization.
The new runtime owns request snapshots, loop termination, tool result pairing, bounded dispatch,
effect custody and recovery classification previously delegated to CAS.

Decide steering timing, response-closure gating for tools, tool-specific cancellation/unknown-effect
UI, automatic retry policy and context eligibility after partial turns in the proper feature/system
authority. Preserve current V1 automatic approval denial; this investigation does not add an
approval UI or broaden execution permissions.

Required implementation evidence includes admission/dispatch crash cuts, result-before/after-effect
cuts, stop versus queued-input races, storage failure, parallel outcomes, restart without lifecycle
intent, and closure without a final local turn signal. These are local deterministic verification
cases, not reasons to repeatedly disrupt live subscription requests. Remote uncertain outcomes
should be represented, not resolved through unbounded retries or experimental destructive actions.

# Sources

Independent completion review accepted this research boundary on 2026-09-17 with no blocking
findings. Partial-context eligibility, steering timing and handler-specific cancellation remain
design/verification gates. Acceptance does not release the implementation hold.

- [Direct streamed continuation](direct-stream-continuation.md),
  [streaming compaction](subscription-streaming-compaction.md),
  [web/image results](subscription-web-image-results.md): direct service acceptance only for the
  exact requests described; no crash, remote-cancel or exactly-once claim.
- Local `doc/features/composer/design.md`, Submission And Queuing;
  `doc/features/backend-runtime-recovery/design.md`, Disabled Paths and Connection Loss Recovery;
  `doc/features/lifecycle-yield/design.md`, Continuation Behavior; inspected 2026-09-17.
- [Responsibility inventory](cas-responsibility-inventory.md) and
  [streaming assessment](ordering-and-bounded-streaming.md): retained baseline and explicit gaps.
- Pinned Codex [SSE decoder](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/sse/responses.rs),
  [turn loop](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/session/turn.rs),
  [item dispatch](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/stream_events_utils.rs),
  [retry policy](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/responses_retry.rs),
  [client reuse guards](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/core/src/client.rs)
  and [WebSocket lifecycle](https://github.com/openai/codex/blob/6b9826e3aa83b1a5947db50f4332cb9c65f1b340/codex-rs/codex-api/src/endpoint/responses_websocket.rs),
  inspected 2026-09-17. These are client policies, not subscription protocol guarantees.
- Crash/effect scenarios and proposed ownership above are architecture reasoning from these
  requirements, not observations of live service behavior.
