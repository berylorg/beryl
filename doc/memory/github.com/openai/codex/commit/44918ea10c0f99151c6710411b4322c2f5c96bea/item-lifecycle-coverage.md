# Reason For Investigation

Phase 13 of the Beryl-home rework captures ordinary Codex App Server (CAS) turns from live
notifications and must not query CAS historical transcript APIs. This investigation asks whether
the pinned CAS 0.144.1 `item/started`, item-specific delta, and `item/completed` stream is exhaustive
for every history-relevant v2 `ThreadItem` that Syndic must either preserve or reject explicitly.

The source instance is OpenAI `codex` tag `rust-v0.144.1`, commit
`44918ea10c0f99151c6710411b4322c2f5c96bea`. The older CAS 0.137 transcript-history note was read
first, but its historical `itemsView = "full"` loading path is obsolete under the CAS-live Syndic
cutover.

# Outcome

## Verdict

No. A successfully materialized canonical `TurnItem` normally reaches app-server v2 through
`item/completed`, and that final notification is the authoritative public item snapshot. That is a
strong capture boundary, but it is not an exhaustive two-event history feed in this pinned source:

- Multi-agent-v2 `SubAgentActivity` is deliberately emitted only as `item/completed`.
- A hosted Responses API `ImageGenerationCall`, if injected, is persisted into raw CAS model
  history but is not materialized by the normal response-stream finalizer as a `TurnItem`, so it
  emits neither shared item lifecycle event on that path. CAS 0.144.1 cannot declare the required
  hosted tool and therefore has no supported producer path for this item. Standalone extension image
  generation is a separate admitted producer and does emit both events.
- Shared item events describe public, materialized `ThreadItem` state, not byte-identical raw
  Responses API history. Tool calls/outputs, synthetic context, injected raw items, compaction
  internals, and other raw `ResponseItem` records can be model-history authority without a public
  `ThreadItem` lifecycle.
- Live `turn/completed` cannot repair a missed or absent item notification: the pinned app server
  sends `items = []` with `itemsView = "notLoaded"`.

The current public documentation says to use `item/*` as the source of truth and says all items emit
both shared lifecycle events. That is useful current intent, not proof of 0.144.1 behavior; the two
exceptions above are visible in the pinned implementation.

## Pinned Public Item Boundary

The v2 union has 18 variants at
`codex-rs/app-server-protocol/src/protocol/v2/item.rs:222-396`:
`UserMessage`, `HookPrompt`, `AgentMessage`, `Plan`, `Reasoning`, `CommandExecution`, `FileChange`,
`McpToolCall`, `DynamicToolCall`, `CollabAgentToolCall`, `SubAgentActivity`, `WebSearch`, `ImageView`,
`Sleep`, `ImageGeneration`, `EnteredReviewMode`, `ExitedReviewMode`, and `ContextCompaction`.
`From<CoreTurnItem>` covers the complete core union at the same file's lines 803-973.

Core's common emitters clone a start snapshot into `EventMsg::ItemStarted` and move a final snapshot
into `EventMsg::ItemCompleted` in
`codex-rs/core/src/session/mod.rs:1986-2015`. App server forwards canonical starts and completions
through `item_event_to_server_notification` in
`codex-rs/app-server/src/bespoke_event_handling.rs:939-994`; the protocol conversion preserves the
full public item at `codex-rs/app-server-protocol/src/protocol/event_mapping.rs:401-415`.

The ordinary lifecycle shapes are:

- Beryl-known user input: `record_user_prompt_and_emit_turn_item` persists the user message and then
  emits an immediate start/completion pair
  (`codex-rs/core/src/session/mod.rs:3829-3846`). Syndic already owns the sealed submitted input, so
  these notifications should correlate and validate it rather than create a second provider-authored
  user item.
- CAS-generated user-role hook prompts: stop-hook continuation records a raw message and uses
  `record_response_item_and_emit_turn_item`, which emits an immediate pair when parsing yields a
  `HookPrompt` (`codex-rs/core/src/session/turn.rs:372-391` and
  `codex-rs/core/src/session/mod.rs:3813-3826`). Unlike the original prompt, this content is not
  already Beryl-owned.
- Provider text: agent messages and reasoning start when a streamed response item is added, use
  agent/reasoning delta events, and complete from the finalized response item
  (`codex-rs/core/src/session/turn.rs:2144-2222` and
  `codex-rs/core/src/stream_events_utils.rs:318-387`). Proposed plan items use start, `PlanDelta`, and
  an authoritative completed item (`codex-rs/core/src/session/turn.rs:1443-1496`).
- Operational work: command execution, file change, MCP, dynamic-tool, collab-agent, hosted web
  search, image view, sleep, review-mode markers, and context compaction have explicit canonical
  emit sites. The item-specific delta mappings are agent text, plan text, reasoning summary/raw text,
  command output, and file patch updates in
  `codex-rs/app-server-protocol/src/protocol/event_mapping.rs:359-460`. Final items, not concatenated
  deltas, remain authoritative.
- Standalone extension web search and image generation call `emit_turn_item_started` and
  `emit_turn_item_completed` through `CoreTurnItemEmitter`
  (`codex-rs/core/src/tools/handlers/extension_tools.rs:66-110`).

## Confirmed Exceptions And Other History Authority

`SubAgentActivity` is an instantaneous history item rather than a paired work lifecycle.
`emit_sub_agent_activity` calls only `emit_turn_item_completed`
(`codex-rs/core/src/tools/handlers/multi_agents_v2.rs:44-51`), and spawn, interaction, and interrupt
handlers use it. The legacy-to-v2 mapper likewise maps `EventMsg::SubAgentActivity` directly to
`ItemCompleted` (`codex-rs/app-server-protocol/src/protocol/event_mapping.rs:181-193`). A correct
consumer must therefore accept an authoritative completion without a prior start for this variant.

Hosted image generation has a different and more serious gap:

- `parse_turn_item` knows how to convert `ResponseItem::ImageGenerationCall` to
  `TurnItem::ImageGeneration` (`codex-rs/core/src/event_mapping.rs:209-223`).
- Normal non-tool finalization only materializes `Message`, `Reasoning`, and `WebSearchCall`; all
  other response items return `None`
  (`codex-rs/core/src/stream_events_utils.rs:422-445`). `handle_output_item_done` emits shared item
  events only when that result is `Some`, but still records the raw response item
  (`codex-rs/core/src/stream_events_utils.rs:358-387`).
- Recording appends the raw `ResponseItem` to conversation/rollout history and can fan out the
  internal raw event (`codex-rs/core/src/session/mod.rs:2828-2845`). No other production call site in
  the pinned tree materializes a hosted `ImageGenerationCall` lifecycle.
- Historical `ThreadHistoryBuilder::handle_response_item` only recognizes user-role hook-prompt
  messages, not image-generation response items
  (`codex-rs/app-server-protocol/src/protocol/thread_history.rs:438-462`). Consequently neither live
  shared lifecycle nor the ordinary public history builder supplies a hosted image item backstop on
  this path.

The parser/materialization gap is confirmed, but the supported producer is unreachable under
Beryl's pinned boundary. Source inspection and the deterministic installed-runtime proof retained in
`hosted-image-generation-reachability.md` show that CAS 0.144.1 cannot serialize the native hosted
tool declaration even when the provider and selected model advertise image capability. A
nonconforming custom provider could inject an unsolicited item, but that behavior is outside the
supported runtime contract. Standalone extension image generation remains distinct and paired.

The internal raw stream is not a stable substitute. `thread/start.experimentalRawEvents` is an
experimental, internal-only opt-in
(`codex-rs/app-server-protocol/src/protocol/v2/thread.rs:138-147`), and the app-server listener drops
`RawResponseItem` events unless it is enabled
(`codex-rs/app-server/src/request_processors/thread_lifecycle.rs:311-321`). Its notification carries
the raw `ResponseItem`, not a public `ThreadItem`. Adopting it would require a separately approved
and proven raw-history design, not a small lifecycle fallback.

There is also a deliberate semantic boundary between public items and exact model history. For
example, final agent-item construction strips hidden citation markup and proposed-plan blocks before
emitting the public item, while the raw response is persisted separately
(`codex-rs/core/src/stream_events_utils.rs:448-472`). Raw function/custom-tool calls and their output
items are persisted for later model requests but are represented publicly, if at all, by synthesized
operational `ThreadItem` forms. `item/completed` is authoritative for the public item; it is not a
byte-identical rollout or replay record.

Finally, live terminal notification construction explicitly sets an empty item vector and
`TurnItemsView::NotLoaded` in
`codex-rs/app-server/src/bespoke_event_handling.rs:1224-1241`. Terminal status proves the turn
outcome, not item-set completeness.

## Local Beryl Impact And Phase 13 Recommendation

Beryl's backend currently has dedicated variants only for `userMessage`, `agentMessage`,
`reasoning`, `commandExecution`, `fileChange`, and `imageGeneration`, plus a sparse `Generic`
fallback (`crates/beryl-backend/src/turn.rs:213-372`). The fallback retains only selected activity
metadata and discards rich payloads such as tool arguments/results, hook fragments, plan text, image
paths, and review text (`crates/beryl-backend/src/turn.rs:288-338`; deserialization is at lines
1262-1295).

Phase 13 live capture persists only agent-message text and command/file operational text. It
explicitly ignores user messages, reasoning, image generation, and every generic item on both start
and completion (`crates/beryl-app/src/cas_projection/ordinary/capture.rs:130-200`). Terminal
reconciliation repeats that same subset over `turn.items`
(`crates/beryl-app/src/cas_projection/ordinary/capture/terminal.rs:16-108`), but actual live
`turn/completed` supplies no items. An unsupported provider item can therefore be silently omitted
while the turn is still published complete.

Phase 13 should not be declared exhaustive in this state. Its completion gate should require:

- Correlating the CAS `UserMessage` lifecycle with Syndic's already-durable submitted input without
  duplicating authorship.
- A closed disposition for every provider-produced pinned `ThreadItem`: preserve it in an exact
  supported typed/resource form, or keep history incomplete with a typed unsupported-history reason.
  Silently ignoring the item and publishing complete is not sufficient.
- Treating `item/completed` as the final public item authority and accepting completion-only
  `SubAgentActivity`; no generic requirement that every completion has a prior start can be sound for
  this release.
- Removing terminal-item backfill assumptions. `turn/completed` should finalize outcome and audit
  already admitted durable items, not prove that the observed item set was exhaustive.
- Exclude hosted Responses image generation from the CAS 0.144.1 supported producer contract and
  make no complete-history claim for unsolicited nonconforming provider behavior. Preserve and test
  the separate standalone generated-media lifecycle. Beryl's accepted normalized boundary
  deliberately discards that item's base64 `result` at incoming JSON ingress and retains
  `savedPath` plus non-binary metadata; upstream field presence does not authorize Fjall binary
  storage. The internal experimental raw stream should not be adopted implicitly.
- Focused exact-target proofs for at least reasoning, hook prompt, plan, MCP/dynamic/collab items,
  standalone media, completion-only subagent activity, and a hosted-image-generation fixture or
  capability rejection. Stream-loss handling must still converge to explicit incomplete history.

This recommendation matches the local target boundary that raw reasoning and unsupported payloads
must not be converted into invented text, while generated media and supported operational events
must update exact Syndic identities (`doc/systems/cas-live-syndic-transcript/design.md:211-236`). It
also preserves Phase 13's count-independent live-state requirement (`doc/plan.md:450-473`).

# Sources

- OpenAI `codex` repository, canonical remote `https://github.com/openai/codex.git`, requested tag
  `rust-v0.144.1`, resolved commit `44918ea10c0f99151c6710411b4322c2f5c96bea`, accessed
  2026-07-16. Commands used: `git rev-parse HEAD`, `git describe --tags --exact-match HEAD`,
  `git remote get-url origin`, and focused `rg -n` plus line-numbered source reads.
- Pinned protocol and conversion sources:
  `codex-rs/app-server-protocol/src/protocol/v2/item.rs`,
  `codex-rs/app-server-protocol/src/protocol/event_mapping.rs`,
  `codex-rs/app-server-protocol/src/protocol/thread_history.rs`,
  `codex-rs/protocol/src/items.rs`, and `codex-rs/protocol/src/protocol.rs`.
- Pinned core emission and persistence sources:
  `codex-rs/core/src/session/mod.rs`, `codex-rs/core/src/session/turn.rs`,
  `codex-rs/core/src/stream_events_utils.rs`, `codex-rs/core/src/event_mapping.rs`,
  `codex-rs/core/src/tools/events.rs`, `codex-rs/core/src/tools/handlers/extension_tools.rs`, and
  `codex-rs/core/src/tools/handlers/multi_agents_v2.rs`.
- Pinned app-server handling sources:
  `codex-rs/app-server/src/bespoke_event_handling.rs` and
  `codex-rs/app-server/src/request_processors/thread_lifecycle.rs`.
- Current OpenAI documentation, [Codex App Server](https://developers.openai.com/codex/app-server),
  accessed 2026-07-16. The current Events/Items sections say `item/*` is the turn-item source of
  truth, `item/completed` is authoritative, and all items emit both lifecycle events. This is current
  documentation rather than a versioned 0.144.1 specification and is contradicted by the pinned
  exceptions documented above.
- Existing local memory consulted first:
  `doc/memory/topic/codex-app-server/transcript-history-itemsview-0.137.md` and
  `doc/memory/topic/codex-app-server/thread-inject-items-0.144.1.md`.
- Follow-up source and installed-runtime proof:
  `hosted-image-generation-reachability.md`.
- Local integration inspected:
  `crates/beryl-backend/src/turn.rs`,
  `crates/beryl-app/src/cas_projection/ordinary/capture.rs`,
  `crates/beryl-app/src/cas_projection/ordinary/capture/terminal.rs`,
  `doc/systems/cas-live-syndic-transcript/design.md`, and `doc/plan.md` Phase 13.
