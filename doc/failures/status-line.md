# Status Line

## Compaction Acceptance Is Not Completion

During Phase 5 live testing on May 2, 2026, the status-line `Compact` action briefly rendered `compacting`, immediately returned to `ok`, and left context size unchanged.

The invalid approach was treating any `thread/status/changed` idle notification for the selected thread as compaction completion after `thread/compact/start` returned. App-server returns from `thread/compact/start` when the request is accepted, not when compaction is done, and the managed session can already contain deferred idle notifications from earlier activity.

The course adjustment is to keep the compaction worker active until it has observed compaction-specific stream activity for the selected thread, then finish only when the selected thread reports idle after that observed compaction work.

## Compaction Worker Must Subscribe Before Waiting

During Phase 2 live testing on May 3, 2026, a composer submission accepted during compaction stayed visible but later became a failed turn, and the status popup reported that Beryl timed out waiting for context compaction to finish.

The invalid approach was starting compaction from a fresh status-operation backend client and then waiting for stream completion on that same client without first subscribing that client to the target thread. App-server can accept `thread/compact/start` while notifications remain scoped to subscribed client sessions, leaving Beryl waiting on a stream that may never receive the compaction item or final idle transition.

The course adjustment is for the compaction worker to metadata-resume the target thread on its own client before `thread/compact/start`, and for the completion detector to treat a post-request target-thread active transition as compaction activity while still refusing idle-only completion.

## Compaction Observation Deadline Is Not Backend Failure

The status worker's 180-second post-acceptance deadline can expire while CAS still works: the inspected CAS unary compaction path allows four times the default 300-second provider idle timeout for one request. Beryl then drops its observer, forces local idle, and fails queued input without cancellation or terminal backend evidence (`crates/beryl-app/src/shell/status_operation.rs`).

The activity-then-idle detector in `context_compaction.rs` also cannot recover from a missing idle event and does not distinguish successful from failed compaction. The correction is the status-line feature's warning, exact terminal-outcome tracking, bounded read reconciliation, and queued-input preservation. This source analysis does not prove that a notification was lost in any observed incident.

Live observer integration must replace the matching shell transitions together: `finish_context_compaction` forces idle, ordinary terminal-event application also infers idle, and the stopped-worker handler fails queued input. Interruption also requires a held queue that the next explicit submission resumes; generated lifecycle continuation may already be in that queue. Merely adding new observer outcomes to those paths would misapply them. Phase 4 therefore verifies the observer engine independently, and Phase 5 switches the live worker, idle proof, exact stop identity, and queue behavior together.

## Summary History Cannot Recover a Missed Compaction Identity

Phase 4's feasibility check invalidated the assumption that bounded turn-summary reads can recover a requested compaction when all identifying events were missed. In the scoped CAS fork, `app-server/src/request_processors/thread_processor.rs::apply_thread_turns_items_view` retains only the first user and final agent messages for summary pages, dropping `ContextCompaction`. Paginated reads default to summary. `ThreadCompactStartResponse` in `app-server-protocol/src/protocol/v2/thread.rs` is empty, so acceptance provides no turn id either. Beryl's `ThreadTurnsListOptions` currently exposes no items-view selection.

The same CAS processor's `normalize_thread_turns_status` can synthesize `Interrupted` from `InProgress` when a thread is inactive. Reconciliation must distinguish this projection from observed terminal evidence.

Summary-only recovery was stopped before implementation under the Operator's planned-design rule. The accepted correction is the status-line feature's exact process-local CAS receipt, registered before compaction enqueue and updated from raw lifecycle events before client delivery. It requires both a completed compaction item and a successful terminal event; Core can otherwise emit an error-free terminal after swallowing a compaction failure. Missing or invalidated receipt evidence remains unknown. The CAS prerequisite must be implemented before Phase 4 resumes. See [investigation evidence](../memory/github.com/berylorg/codex-fork/commit/51eef61b78bb1a054f181e15690849194d73dc71/compaction-observation-evidence.md).

## Listener Identity Does Not Prove Observer Liveness

During the CAS receipt implementation, review found that a retained weak thread reference could still match after an abnormal listener-task drop. Reusing that identity could admit compaction against a dead observer. A shared liveness witness now participates in listener reuse and receipt reservation; listener drop invalidates unfinished receipts for its exact generation while preserving terminal proof. Reservation checks liveness under the registry lock, and a death racing after reservation leaves an observation gap that later acceptance cannot overwrite. The focused drop-before-reservation regression and independent review cover this boundary.

## Empty Receipt States Must Reject Contradictory Fields

Phase 11's malformed-receipt test showed that Serde's internally tagged unit variants accepted extra fields even with `deny_unknown_fields`: a `completed` state accompanied by an `error` field decoded as completion. Beryl now decodes through empty struct variants before normalizing public receipt states. The regression verifies rejection over both WebSocket and stdio, alongside identity, missing-evidence, and byte-bound checks. Receipt validation must enforce the state contract rather than assume a derive attribute rejects every contradictory shape.

## Dequeued Idle Does Not Prove Fresh Idle

Phase 4 review found that the managed session can buffer notifications while awaiting an RPC response. A completed receipt followed by an active metadata response could then be followed by a previously buffered idle notification. Treating dequeue order as observation freshness would report success against the newer active status. The observer therefore confirms idle through a fresh metadata read after terminal proof; queued idle notifications do not complete the operation. Regression cases cover both receipt-derived and stream-derived terminal proof followed by an active read and an old idle event.

## Local Preparation Can Precede a Rejected Subscription

Phase 5 review found that the observer allocates an operation identity before subscribing, while the shell learns that identity through a later prepared update. Subscription failure can therefore produce an exact-target rejected outcome with an operation identity before that prepared update exists. Rejecting every such update as an identity mismatch loses the definite setup failure and leaves the UI falsely unconfirmed. The shell must accept this narrow setup-rejection case without allowing an unannounced operation to establish success or a stop target.

## Diagnostic Reads Must Preserve Reachable Evidence and Live Timing

Phase 6 review found that slicing the diagnostic ring before applying `afterSequence` hid later retained events. The tool now filters the full bounded snapshot before applying its public result limit. Operation timing uses per-observation monotonic anchors so quiet, unconfirmed work continues aging; old cleanup cannot change the latest summary's timing. Producer handles retain only the collector and numeric generation, avoiding identity retention beyond the ring's byte budget. Initial workspace and thread identities are validated or omitted before capture. Focused ring/tool and actual-observer tests cover pagination, bounds, stale cleanup, live timing, and content omission.

## Account Limits Must Keep Bucket Identity

During May 5, 2026 live testing, the Context status cell showed only `Weekly`, omitted the short-window limit, and could show the Spark weekly bucket while the active Beryl model was a main Codex model.

The invalid approach was merging every `account/rateLimits/read` snapshot into one daily/weekly pair and recognizing only the 1440-minute daily window. Current app-server responses can include multiple `rateLimitsByLimitId` buckets, including a general `codex` bucket and a model-specific Spark bucket, and the short-window bucket can be 300 minutes rather than 1440.

The course adjustment is to preserve `limitId` and `limitName`, select the bucket matching the active status model with `codex` as the non-Spark fallback, and render the exact short-window label such as `5h` when that is the window app-server reports.
