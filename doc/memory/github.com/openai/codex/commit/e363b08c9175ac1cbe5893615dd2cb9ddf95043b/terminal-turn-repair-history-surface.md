# Reason For Investigation

The CAS-live Syndic transcript design authorizes one separately named, release-pinned adapter to
repair one already-correlated terminal turn after a proven or conservatively suspected live-capture
gap. The investigation had to determine whether exact Codex App Server 0.146.0 offers a history
surface narrow enough for that adapter, what identities and terminal facts the response actually
proves, and how generated-image data must cross the repair boundary.

This note records evidence and implementation constraints only. It does not authorize ordinary CAS
history browsing, change the controlling design, or add implementation sequencing to `doc/plan.md`.

# Outcome

## Evidence Boundary

Exact pinned evidence consists of the retained `codex-cli 0.146.0` executable, its SHA-256, fresh
stable and experimental schema generation from that executable, and the already retained
commit-identity note in this memory scope. Those generated schemas are exact for the admitted
executable but do not by themselves prove runtime processor behavior or the reducer's identity and
status semantics.

A separate local OpenAI Codex checkout at commit
`495da4564337099f064b25f7e2f00436f56bc076` was inspected only as newer-checkout corroboration. Its
source must not be attributed to the pinned source commit
`e363b08c9175ac1cbe5893615dd2cb9ddf95043b`. Exact older release notes for 0.144.1 and live 0.137
evidence are useful lineage evidence, but they also are not substitutes for 0.146.0 source or live
behavior.

## Exact Pinned Stable And Experimental Schemas

Stable schema exposes `thread/read`. `ThreadReadParams` requires `threadId` and optionally accepts
`includeTurns: boolean`. `ThreadReadResponse` contains one `thread`; when `includeTurns` is true,
`thread.turns` is an unpaged array of materialized `Turn` values. The request has no turn selector,
cursor, limit, sort direction, or item-view control. It is therefore a whole-thread history read and
cannot satisfy the bounded one-turn repair contract.

Stable schema does not expose `thread/turns/list`, `thread/turns/items/list`, or
`thread/items/list`.

Experimental schema adds `thread/turns/list` with this exact request shape:

- required `threadId: string`;
- optional nullable `cursor: string`;
- optional nullable `limit: uint32`;
- optional nullable `sortDirection: "asc" | "desc"`, documented to default to descending; and
- optional nullable `itemsView: "notLoaded" | "summary" | "full"`, documented to default to
  summary.

Its response requires `data: Turn[]` and also carries nullable `nextCursor` and
`backwardsCursor`. Each `Turn` requires `id`, `items`, and `status`; `itemsView` defaults to `full`
when omitted from a serialized turn, while `error`, `startedAt`, `completedAt`, and `durationMs` are
optional or nullable.

The exact executable's experimental schema exposes `thread/items/list`, not
`thread/turns/items/list`. Its params require `threadId`, optionally filter by nullable `turnId`, and
optionally accept cursor, limit, and sort direction; its response pages `{ turnId, item }` entries.
No 0.146.0 live probe was run here, so schema presence does not prove that route works. Retained
0.137 live evidence and exact 0.144.1 source evidence found the item-list implementation
unsupported, while the separate checkout hard-codes JSON-RPC method-not-found for the renamed
`thread/turns/items/list` route. The spelling and runtime-support discrepancy is an exact-source
refresh requirement, not permission to depend on either route.

## Bounded One-Turn Repair Shape

The only schema shape that can bound the returned turn collection is experimental
`thread/turns/list` with `limit: 1` and `itemsView: "full"`. A repair adapter must reject zero turns,
more than one turn, a returned `itemsView` other than `full`, a turn id different from the durable
CAS turn correlation, or any nonterminal or incompatible status.

The request does not accept a `turnId`. Without a previously authenticated cursor, descending
`limit: 1` addresses only the current last materialized turn. It is safe for an exact correlated
repair only if the adapter separately proves that the target is that last turn and that no successor
can race the read. It cannot safely target an older repair turn after a successor exists.

`nextCursor` is documented for continuing after the last returned turn. `backwardsCursor` is
documented for reversing `sortDirection`, re-including the anchor turn so updates to it can be
observed. Cursors are opaque protocol values: Beryl must neither synthesize them from a turn id nor
treat one issued for a different page, direction, thread, process, or source instance as authority.
The returned response does not echo `threadId`, direction, cursor, release, process, or runtime
identity, so the adapter must retain and digest the exact request/response inside its already
authenticated runtime boundary.

`nextCursor` may legitimately be non-null when the one returned target is complete and only older
turns remain. It must never authorize following that cursor during this repair. The adapter is a
single-target projection, not permission to complete adjacent pages. The schema also does not place
a byte or item-count maximum on `Turn.items`, so Beryl's fixed incoming response, item, field, and
media limits remain mandatory even with `limit: 1`.

The separate checkout corroborates a default page size of 25, maximum 100, clamping of a supplied
zero to one, descending and summary defaults, and a cursor internally anchored by turn id plus an
include-anchor flag. It also corroborates that `itemsView = "full"` leaves the complete
materialized item vector for that turn, while summary keeps only the first user message and final
agent message. These processor details are not exact pinned proof and must be rechecked at the
pinned source commit before implementation relies on them.

## Terminal Status And Identity Limits

The exact pinned schema admits four statuses: `completed`, `interrupted`, `failed`, and
`inProgress`. Only the first three are terminal-shaped. `failed` may include `Turn.error`; the
schema does not require `error`, completion time, duration, or any terminal-event sequence number.
The repair candidate's durable terminal outcome must therefore pre-exist the read and agree with
the response. The history status cannot independently establish why, when, or in what live-event
order the turn terminated.

A `Turn` carries its own string id but not its thread id. Every public `ThreadItem` variant carries
an item id, but the schema provides no namespace, source-event position, cryptographic binding, or
statement that every id came from a live notification. Retained 0.144.1 source evidence and the
separate checkout both show history reconstruction can synthesize turn ids and `item-N` ids when
persisted events lack canonical ids, can coalesce changing operational items, and does not recreate
delta boundaries. Consequently:

- exact durable CAS thread and turn correlation authorizes the read; visible content never does;
- the returned turn id and every required item id must be present and pass the pinned adapter's
  identity rules;
- a generated or previously unseen public id is not evidence that Beryl captured the corresponding
  live lifecycle;
- the response is semantic final-item state, not notification replay; and
- missing identity, status disagreement, unknown variants, malformed required fields, or
  unrepresentable content makes the repair incomplete rather than partially publishable.

The separate checkout also normalizes reconstructed `inProgress` to `interrupted` when no live
active thread remains. That is corroboration of a materialized-history hazard, not exact pinned
proof. Until the pinned processor and reducer are inspected or probed, an `interrupted` history
status must not be treated as exact interruption-cause or ordering evidence.

## Exact Pinned Item Union And Generated Images

The exact 0.146.0 generated `ThreadItem` union contains these 18 discriminants:
`userMessage`, `hookPrompt`, `agentMessage`, `plan`, `reasoning`, `commandExecution`, `fileChange`,
`mcpToolCall`, `dynamicToolCall`, `collabAgentToolCall`, `subAgentActivity`, `webSearch`,
`imageView`, `sleep`, `imageGeneration`, `enteredReviewMode`, `exitedReviewMode`, and
`contextCompaction`. The adapter needs a closed disposition for every variant; it must not silently
drop an operational or unfamiliar item while publishing the turn complete.

For `imageGeneration`, exact pinned schema requires `id`, `status`, `result`, and discriminator
`type`; `revisedPrompt` and `savedPath` are nullable. Schema types `result` only as a string and
does not declare its encoding. Retained image-generation source evidence identifies the standalone
value as the provider image's base64 payload, and the separate checkout maps both `result` and
`savedPath` into the public item. Beryl's controlling rule is stricter than schema acceptance:

- structurally consume and discard `result` before it can enter retained typed state, diagnostics,
  logs, spill, or durable storage;
- require a nonempty `savedPath` for successful generated-media handoff;
- read that exact path promptly through the authenticated repair runtime under fixed byte and type
  bounds, then admit the bytes to the Beryl-home image store;
- retain snapshot, CAS thread, turn, item, runtime, and path provenance, but never treat the runtime
  path itself as durable media authority; and
- make the whole repaired turn incomplete when the path is missing, empty, changed, unreadable,
  unsupported, oversized, or unauthenticated. Inline base64, a URL, similar file, or prior transient
  bytes is never a fallback.

## Design And Plan Impact

The retained evidence supports the controlling design only as a narrow, fail-closed adapter:

- stable `thread/read(includeTurns = true)` is excluded because it reads the whole thread;
- experimental `thread/turns/list` must be called with one authenticated thread, `limit: 1`, and
  `itemsView: "full"` under fixed response bounds;
- the same-thread repair barrier must ensure the correlated repair target is still the last turn,
  or the adapter must possess a source-backed authenticated cursor that can address it without
  reading adjacent turns;
- response admission must prove exactly one matching terminal turn, a full item view, the closed
  item union, required identity and content, and usable generated media before atomic publication;
  and
- the adapter must never follow history cursors, enumerate threads, fill other turns, or expose CAS
  history as a catalog or transcript read surface.

The stable/experimental method-name discrepancy and lack of exact 0.146.0 processor/reducer source
in the local checkout mean implementation may not assume an item-list hydration route or exact
cursor/status semantics from the separate checkout. If exact targeting cannot be established under
the last-turn barrier, the repair must converge incomplete rather than use whole-thread read or
page through history.

`doc/plan.md` Phase 105 owns exact processor/reducer evidence as an independent acceptance
boundary. Phase 106 may implement the adapter only if that evidence proves the required last-turn,
full-item, identity, and terminal semantics. Its acceptance tests need exact stable/experimental
schema fixtures, method registration and unsupported-route coverage, one-turn cursor adversaries,
terminal/status disagreement, every item discriminant, oversized response rejection, and
`savedPath`-only generated-media admission.

## Refresh Triggers

Refresh this note in a new commit scope when the supported CAS release or source commit changes.
Refresh the current scope before implementation if any of these occur:

- the admitted `codex.exe` SHA-256 changes or regenerated stable/experimental schemas differ;
- an exact local snapshot of commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b` becomes available;
- exact pinned processor or focused test evidence establishes the 0.146.0 result for
  `thread/items/list` or `thread/turns/items/list`;
- exact pinned processor source changes the list defaults, cursor construction, active-turn merge,
  status normalization, or unsupported route;
- the item union, required item fields, generated-image `result` or `savedPath` shape, or history id
  synthesis changes; or
- Beryl's repair eligibility, last-turn barrier, incoming limits, identity rules, or generated-media
  handoff changes.

# Sources

## Exact Pinned Release Evidence

- Canonical remote: `https://github.com/openai/codex`. Requested and retained source identity:
  commit `e363b08c9175ac1cbe5893615dd2cb9ddf95043b`, associated with `codex-cli 0.146.0` by the existing
  sibling note `native-spawn-and-thread-response-0.146.0.md`. Accessed 2026-08-10.
- Admitted executable: `codex-cli 0.146.0`, SHA-256
  `D52EFA1D816B305C84C525335F451AAFC56398A7E8515B6C6DB095C4E4FB0D1D`. The same digest was already
  retained in the sibling note and was reverified with `Get-FileHash -Algorithm SHA256`.
- Stable schema was regenerated with
  `codex.exe app-server generate-json-schema --out <ephemeral-stable-output>`; experimental schema
  used the same command with `--experimental`. Relevant generated files were
  `v2/ThreadReadParams.json`, `v2/ThreadReadResponse.json`,
  `v2/ThreadTurnsListParams.json`, `v2/ThreadTurnsListResponse.json`,
  `v2/ThreadItemsListParams.json`, `v2/ThreadItemsListResponse.json`, and the stable and
  experimental `codex_app_server_protocol.v2.schemas.json` request unions. The task-owned outputs
  were inspected and then removed.
- Exact upstream source locations corresponding to this protocol surface are
  `codex-rs/app-server-protocol/src/protocol/common.rs`,
  `codex-rs/app-server-protocol/src/protocol/v2/thread.rs`,
  `codex-rs/app-server-protocol/src/protocol/v2/thread_data.rs`,
  `codex-rs/app-server-protocol/src/protocol/v2/turn.rs`,
  `codex-rs/app-server-protocol/src/protocol/v2/item.rs`,
  `codex-rs/app-server-protocol/src/protocol/thread_history.rs`, and
  `codex-rs/app-server/src/request_processors/thread_processor.rs`. Their exact e363b08 bodies were
  not present in the local checkout during this investigation; source-level behavior beyond the
  generated schema is deliberately not attributed to them here.

## Retained Lineage And Local Corroboration

- `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/reconnect-notification-replay.md`
  records exact 0.144.1 source evidence for whole-history reconstruction, unsupported item listing,
  materialized-state rather than notification replay, active-turn merge, and synthesized ids.
- `doc/memory/topic/codex-app-server/transcript-history-itemsview-0.137.md` records stable versus
  experimental schema exposure, a live unsupported item-list result, and the distinction between
  `notLoaded`, `summary`, and `full` in 0.137.
- `doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/image-generation-result-payload.md`
  and `image-generation-wire-order.md` record the base64 `result`, `savedPath` handoff, and
  discriminator-first discard lineage.
- Separate local checkout of canonical remote `https://github.com/openai/codex`, exact inspected
  commit `495da4564337099f064b25f7e2f00436f56bc076`. Inspected 2026-08-10 only as newer-checkout
  corroboration: the seven protocol/processor paths listed above plus
  `codex-rs/thread-store/src/store.rs`. This checkout exposes
  `thread/turns/items/list`, returns hard-coded method-not-found for it, reconstructs and paginates
  full turn history, uses opaque anchor cursors, normalizes stale `inProgress`, synthesizes missing
  ids, and maps both image-generation fields. None of those source details is exact e363b08 proof.
- Beryl authority and impact sites inspected:
  `doc/systems/cas-live-syndic-transcript/design.md`,
  `doc/systems/backend-runtime/design.md`, `doc/plan.md`, and
  `crates/beryl-backend/src/protocol/response/initialize.rs`.
