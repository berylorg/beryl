# Status Line

## Compaction Acceptance Is Not Completion

During Phase 5 live testing on May 2, 2026, the status-line `Compact` action briefly rendered `compacting`, immediately returned to `ok`, and left context size unchanged.

The invalid approach was treating any `thread/status/changed` idle notification for the selected thread as compaction completion after `thread/compact/start` returned. App-server returns from `thread/compact/start` when the request is accepted, not when compaction is done, and the managed session can already contain deferred idle notifications from earlier activity.

The course adjustment is to keep the compaction worker active until it has observed compaction-specific stream activity for the selected thread, then finish only when the selected thread reports idle after that observed compaction work.

## Compaction Worker Must Subscribe Before Waiting

During Phase 2 live testing on May 3, 2026, a composer submission accepted during compaction stayed visible but later became a failed turn, and the status popup reported that Beryl timed out waiting for context compaction to finish.

The invalid approach was starting compaction from a fresh status-operation backend client and then waiting for stream completion on that same client without first subscribing that client to the target thread. App-server can accept `thread/compact/start` while notifications remain scoped to subscribed client sessions, leaving Beryl waiting on a stream that may never receive the compaction item or final idle transition.

The course adjustment is for the compaction worker to metadata-resume the target thread on its own client before `thread/compact/start`, and for the completion detector to treat a post-request target-thread active transition as compaction activity while still refusing idle-only completion.

## Account Limits Must Keep Bucket Identity

During May 5, 2026 live testing, the Context status cell showed only `Weekly`, omitted the short-window limit, and could show the Spark weekly bucket while the active Beryl model was a main Codex model.

The invalid approach was merging every `account/rateLimits/read` snapshot into one daily/weekly pair and recognizing only the 1440-minute daily window. Current app-server responses can include multiple `rateLimitsByLimitId` buckets, including a general `codex` bucket and a model-specific Spark bucket, and the short-window bucket can be 300 minutes rather than 1440.

The course adjustment is to preserve `limitId` and `limitName`, select the bucket matching the active status model with `codex` as the non-Spark fallback, and render the exact short-window label such as `5h` when that is the window app-server reports.
