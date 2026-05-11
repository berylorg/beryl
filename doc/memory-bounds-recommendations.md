# Beryl Memory Bounds Recommendations

# Summary

The likely post-task growth from a clean roughly 70 MiB startup toward the observed 99 MiB to 129 MiB live sessions is not one allocator bug. It is several retained projections, queues, image resources, and warmed GPUI caches that currently have count-only, lifecycle-only, or no explicit memory budgets.

The highest-impact Beryl-owned fixes are activity pruning, active-turn payload budgets, byte-bounded media/composer image retention, and bounded backend/worker queues. Dependency-owned growth should be wrapped at Beryl call sites when possible and patched in the GPUI fork only when lifecycle APIs or cache budgets are missing.

Backend-owned conversation history must remain authoritative. Beryl should only discard presentation-only state, reconstructable projections, or explicitly non-authoritative diagnostics unless an accepted design change says otherwise.

# Recommended Bounds

## Activity Projection

Owner: `ToolActivityProjection` in `crates/beryl-app/src/shell/tool_activity.rs`.

Growth trigger: long turns and subagent-heavy turns append activity records, rows, labels, runtime metadata, parent-child maps, visible indexes, and reasoning summary parts. Rendering is viewport-windowed, but retained activity history is not.

Recommended bound: keep all running records; retain completed activity by root turn with both a global row budget and a display-byte budget. A reasonable starting policy is 2,000 retained rows or 8 MiB of display payloads globally, with at least the latest 200 completed rows protected for the selected thread. Truncate large per-field display payloads at ingress, for example 16 KiB for labels/arguments and 64 KiB for reasoning summaries. Prune nickname and runtime maps when no retained row or active ownership path references the thread id.

User-visible behavior: older completed activity becomes a summarized or hidden activity-history tail. Transcript history is unchanged. Active/running rows are never pruned.

Implementation and tests: implement pruning inside the projection mutation path and add unit tests around terminal success, error, cancellation, subagent hierarchy pruning, and selected-thread protection. Design change: likely needed if hidden versus summarized old activity is product-visible policy.

## Active Turn Details

Owner: `ExecutionDetailState` and `TurnExecutionRecord` in `crates/beryl-app/src/shell/execution_detail.rs`.

Growth trigger: streaming assistant text, reasoning content, command output, file-change output, generated-image inline results, backend user fragments, and error text can append throughout one active turn and remain after terminal states.

Recommended bound: split transcript-visible text from operational detail. Use per-item and per-turn budgets for live presentation, such as 1 MiB per transcript text item, 8 MiB per turn for transcript-visible resident text, 256 KiB per command/file-output item with head/tail retention, and 2 MiB per turn for raw reasoning detail after terminal state. Match active generated-image inline payload caps to the existing 256 KiB history policy. Over-budget content should retain enough identity to reload or show an exact truncation marker.

User-visible behavior: transcript-authoritative text remains reloadable from the backend after terminal state; non-authoritative command/reasoning detail may be shown as head/tail with an omitted-byte marker. Copy/quote should either operate on loaded exact text or require/retry a backend reload rather than copying synthesized truncated text as if exact.

Implementation and tests: enforce budgets at stream-ingress and terminal-finalization points. Test successful completion, turn error, interruption, reload after truncation, quote/copy semantics, and generated-image payload caps. Design change: needed for exact copy/quote behavior when a user selects over-budget unloaded text.

## Backend And Worker Queues

Owner: `ManagedBackendSession::pending_messages`, `ShellView` worker receivers such as `turn_receiver`, active-turn steering queues, pending input queues, title/inventory workers, and `WorkspacePersistenceQueue`.

Growth trigger: backend streams, UI lag, stalled persistence, slow background clients, or retry/error paths can accumulate queued messages and cloned state.

Recommended bound: replace unbounded channels or vectors with bounded count and byte budgets by queue class. Suggested starting points are 1,024 stream events or 4 MiB per active thread after coalescing, 64 protected dynamic tool requests, 1 MiB of pending text fragments plus an accepted image-byte budget, one coalesced persistence state per workspace, and explicit global concurrency limits for title and inventory workers. Coalesce stream deltas by thread/turn/item/kind wherever order permits.

User-visible behavior: foreground turn streaming remains protected. Over-budget pending user input should be rejected visibly before enqueue. Background title or inventory work can drop stale requests and refresh later.

Implementation and tests: add bounded queue wrappers with byte accounting and coalescing tests for consumer stalls, backend disconnect, turn error, cancellation, and workspace switch. Design change: likely needed for user-facing rejection messages and stale background-work policy.

## Transcript Media And Composer Images

Owner: `TranscriptMediaCache`, transcript image preview/menu state, composer draft payloads, composer clipboard, and accepted draft history in `crates/beryl-app`.

Growth trigger: transcript images and pasted composer images retain compressed bytes, `gpui::Image` handles, previews, clipboard payloads, and draft-history payloads. Several current caps are entry-count-only or per-lane-only.

Recommended bound: make all image retention byte-budgeted. Suggested starting policy is 64 MiB compressed transcript media cache, 128 MiB estimated decoded-image budget, 32 MiB composer clipboard budget, 64 MiB global composer-history budget, and per-draft limits of 20 images or 64 MiB total. Reject or downscale preview-only images that exceed decode limits, and prefer durable image asset references over retained byte copies after an accepted paste is written to workspace storage.

User-visible behavior: evicted transcript media shows a placeholder and reloads if the source is still available. Over-budget pasted images are rejected before they enter the draft. Clipboard/history eviction loses only GUI convenience state, not accepted transcript authority or durable workspace assets.

Implementation and tests: add compressed-byte accounting, decoded-size estimates, eviction callbacks, per-draft validation, and clipboard/history global budgets. Test many large images, animated images, missing files, accepted paste persistence, draft history eviction, and thread switching. Design change: needed for exact default image limits and rejection wording.

## GPUI Image Lifecycle

Owner: GPUI asset loading and render image resources, especially `App::loading_assets`, `gpui::Image`, `RenderImage`, and per-window sprite atlases in the GPUI fork.

Growth trigger: distinct transcript or composer images can leave completed asset tasks and decoded/uploaded image resources alive without a Beryl-owned byte lifecycle tied to cache eviction. Animated GIF/WebP paths can decode all frames.

Recommended bound: Beryl should call an explicit image-release path whenever media cache entries, composer previews, or preview popups are evicted. If current GPUI public APIs cannot drop uploaded atlas resources deterministically, patch the fork with an asset-cache byte budget, explicit drop handles, and optional animated-frame/pixel limits. Suggested decode admission limits are 32 megapixels, 128 MiB decoded bytes per image, and 32 frames or 128 MiB decoded bytes for animations.

User-visible behavior: evicted images reload on demand if their source remains available. Images exceeding decode policy show a stable unsupported-too-large fallback.

Implementation and tests: first add Beryl-side release calls, then add GPUI fork changes only if resources remain live after eviction. Test asset eviction, popup close, cache churn, animated images, and device/window teardown. Design change: needed if GPUI fork public lifecycle APIs are added for Beryl.

## Transcript Projections And Markdown Cache

Owner: `TranscriptPresentationState`, `TranscriptHistoryWindow`, `TranscriptStreamProjection`, and `TranscriptMarkdownCache`.

Growth trigger: loaded or visited history pages, row projections, stream entries, and parsed/rendered Markdown structures duplicate transcript data beyond the render-frame window.

Recommended bound: keep render-frame work viewport-windowed and add resident projection budgets. Suggested policy is to pin the viewport, selection, edit/branch targets, active turn, and latest tail; release older page metadata to placeholders; retain stream entries only for active/current-frame keys; and budget Markdown cache by both source bytes and estimated parsed/render bytes.

User-visible behavior: released transcript pages remain visible as reloadable placeholders or are reloaded before interaction. Transcript authority remains backend-owned.

Implementation and tests: add page-release invariants and cache byte accounting. Test selection pins, branch/edit pins, cold-page reload, active streaming, large Markdown blocks, and copy/quote after reload. Design change: needed for placeholder presentation and reload-on-interaction behavior.

## Text Input Undo And Layout State

Owner: `gpui-text-input::TextInputState` as used by composer and settings fields.

Growth trigger: default undo/redo stacks retain full-buffer snapshots with a count limit of 128 per stack but no byte limit. Large pasted drafts can multiply memory across snapshots.

Recommended bound: configure text inputs with byte-aware undo limits. Suggested policy is 16 snapshots and 2 MiB total undo/redo bytes for the composer, smaller limits for settings fields, and undo clearing after successful submit or when a draft is persisted into accepted pending input.

User-visible behavior: very old undo history disappears sooner for large drafts. Current text remains intact.

Implementation and tests: add configuration at Beryl text-input construction sites or extend `gpui-text-input` if it lacks byte-budget hooks. Test large paste, repeated edits, submit, draft restore, and settings windows. Design change: probably not needed unless undo history guarantees are made user-visible.

## Workspace Inventories, Graph, And Checklist Projections

Owner: `WorkspaceConversationState`, `known_threads`, member-thread inventory snapshots, thread selector projection, graph overlay/mutation state, and checklist sidebar projection caches.

Growth trigger: backend thread count, workspace graph size, closed selector projections, graph mutation gaps, and checklist expansion can grow resident metadata even when only a small subset is visible.

Recommended bound: separate durable pinned references from bounded presentation metadata. Keep exact durable graph/thread-ref state, but window inventory snapshots, virtualize selector/checklist rows, age-cap closed selector projections, and cap graph mutation queues by count, bytes, and elapsed time with full-reload recovery on revision gaps. Suggested starting policies are a selected-workspace inventory window, 512 closed selector rows, and 256 pending graph mutations or 4 MiB of pending mutation payloads.

User-visible behavior: stale inventory or selector details may refresh when reopened. Durable graph state, manual titles, bindings, token snapshots, and thread refs must not be discarded as cache.

Implementation and tests: add cache/durable-state boundaries before pruning. Test manual-title preservation, rebind-needed state, graph refs, mutation failure, inventory refresh, selector reopen, and checklist expansion. Design change: likely needed to define durable versus cache thread metadata precisely.

## Protocol Input And Dependency Parsing Limits

Owner: Beryl WebSocket transport around `soketto`, accepted `serde_json` values, stdio readers, and image decoding via `image`.

Growth trigger: handshake reads, accepted JSON payloads, full-frame messages, stdio lines, and image dimension probes materialize complete buffers before higher-level handling.

Recommended bound: keep the existing 64 MiB runtime WebSocket frame cap, add a handshake response cap such as 1 MiB plus timeout, bound stdio line/message buffers, reject accepted JSON payloads above endpoint-specific budgets before converting them into long-lived state, and set image-reader allocation/dimension limits before decode.

User-visible behavior: oversized protocol or media inputs fail with explicit bounded-resource errors. Normal foreground streams remain unaffected.

Implementation and tests: wrap handshake/read/decode call sites and add tests for oversized handshake, oversized JSON, oversized stdio output, and oversized images. Design change: needed only for the exact public error policy.

# Implementation Order

Start with Beryl-owned caps that do not require dependency changes: activity pruning, active-turn detail budgets, media/composer byte budgets, and queue coalescing. Then add GPUI image lifecycle hooks and text-input undo byte limits. Treat workspace inventory/graph/checklist caps as a design follow-up because they require a precise durable-state versus cache boundary.

Before any behavior change, add diagnostics for retained activity rows/bytes, active turn bytes, media compressed bytes, estimated decoded image bytes, queue lengths/bytes, GPUI asset counts where observable, and text-input undo bytes. These diagnostics will make the accepted bounds measurable without reusing unsafe GUI stress on the active Beryl process.
