# Codex App Server Contract Notes

This note records live `codex app-server` contract observations gathered during backend integration work.

It is an implementation aid, not a design authority. `doc/design.md` remains authoritative.

## Validation Basis

- JSON schema inspection used `codex app-server generate-json-schema`.
- Early live probing used the `stdio://` transport before the managed WebSocket boundary was adopted.
- Initial backend validation was performed against `codex-cli 0.118.0`.
- Later transcript-focused live probing was performed against `codex-cli 0.122.0`.
- Status-line planning schema inspection was performed against `codex-cli 0.125.0` using `codex app-server generate-json-schema --out <temp-dir>`.
- Thread-history pagination schema inspection was performed against `codex-cli 0.125.0` using `codex app-server generate-json-schema --out <temp-dir>`.
- Read-only token-usage read-through was re-checked on April 30, 2026 against `codex-cli 0.125.0` using `codex app-server generate-json-schema --out <temp-dir>`.
- Thread-name schema inspection was performed on April 30, 2026 against `codex-cli 0.125.0` using `codex app-server generate-json-schema --out <temp-dir>`.
- Title-generation surface inspection was performed on April 30, 2026 against `codex-cli 0.125.0` using `codex app-server generate-json-schema --experimental --out <temp-dir>` and local Codex source snapshots.
- Status-line operation schema inspection, source inspection, and live `model/list` probing were performed on May 2, 2026 against `codex-cli 0.125.0`.
- Pending new-thread default probing was performed on May 2, 2026 against `codex-cli 0.125.0` using `config/read`, `model/list`, and disposable ephemeral `thread/start` requests over managed stdio compatibility transport.
- Multi-client transport planning inspected `codex-cli 0.125.0` help output on May 2, 2026. `codex app-server --help` reports `ws://IP:PORT`, Unix socket, stdio, and off listen modes plus WebSocket capability-token and signed-bearer-token auth flags.
- Turn steering schema inspection was performed on May 3, 2026 against `codex-cli 0.125.0` using `codex app-server generate-json-schema --out <temp-dir>` and `codex app-server generate-json-schema --experimental --out <temp-dir>`. `turn/steer` is present in the stable schema, not only the experimental schema.
- Tool activity schema inspection and a live `mcpServerStatus/list` probe were performed on May 3, 2026 against `codex-cli 0.125.0`.
- A May 3, 2026 live long-running shell-command probe observed `commandExecution` activity remain active for the command duration and transition only after the command completed.
- Dynamic tool schema inspection was performed on May 3, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --experimental --out <temp-dir>`.
- Turn hard-stop planning schema inspection was performed on May 4, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --experimental --out <temp-dir>`. The schema includes `turn/interrupt`, `thread/backgroundTerminals/clean`, `command/exec/terminate`, and command-exec `processId` fields.
- Reasoning activity schema inspection and live turn probes were performed on May 4, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --experimental --out <temp-dir>` and disposable turns with high reasoning plus detailed summaries.
- Account rate-limit schema inspection was performed on May 5, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --experimental --out <temp-dir>` and rechecked against the generated v2 schema for `account/rateLimits/read`.
- Subagent activity model/reasoning schema inspection and live metadata probing were performed on May 5, 2026 against `codex-cli 0.128.0` using generated app-server schemas plus read-only `thread/read` and metadata-only `thread/resume` probes on observed subagent thread ids.
- Ordered multimodal user-input probing was performed on May 5, 2026 against local `codex app-server` over stdio with an ephemeral thread and a `turn/start.input` array shaped as text `ALPHA`, `localImage` with a temp PNG labeled `IMAGE-BETWEEN`, then text `GAMMA`. The model replied `ALPHA -> IMAGE-BETWEEN -> GAMMA`, and app-server echoed `userMessage.content` in the same text/localImage/text order.
- Historical local-image probing was performed on May 5, 2026 against `codex-cli 0.128.0` over live app-server WebSocket. `thread/turns/list` and `thread/read` returned path-only `localImage` records, and `fs/readFile` returned `dataBase64` for that path only while the referenced file was still present.
- Remote-control status notification schema inspection was performed on May 6, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --out <temp-dir>` and the generated v2 schema for `remoteControl/status/changed`.
- Thread-branch schema inspection and local source/test inspection were performed on May 7, 2026 against `codex-cli 0.128.0` using `codex app-server generate-json-schema --out <temp-dir>`, `codex app-server generate-json-schema --experimental --out <temp-dir>`, and local Codex source snapshots.
- Thread-edit rollback API inspection was performed on May 8, 2026 against `codex-cli 0.128.0` using stable and experimental generated app-server schemas plus upstream Codex app-server and core source files for rollback reconstruction and request handling.
- Image-generation transcript probing was performed on May 9, 2026 against `codex-cli 0.128.0` using local app-server stdio and `thread/read includeTurns=true` on a thread containing an AI-generated image. The observed `imageGeneration` item carried base64 PNG result bytes, a generated-image `savedPath`, a `revisedPrompt`, and `status = "generating"` even though the result bytes and saved file were already available.

## Transport

- Stdio compatibility transport is newline-delimited JSON, not LSP-style `Content-Length` framing.
- One complete JSON request per line is accepted and responses or notifications arrive as one JSON object per line.
- The GUI should treat protocol readiness as successful `initialize` and follow-up request behavior, not as clean stderr output.
- WebSocket transport is available through `codex app-server --listen ws://IP:PORT` and carries one JSON-RPC message per WebSocket text frame.
- Loopback WebSocket listeners can use capability-token auth through `--ws-auth capability-token --ws-token-file <absolute-path>`.
- WebSocket clients present the capability token as an `Authorization: Bearer <token>` header during the WebSocket handshake.

## Initialize Surface

- `initialize` accepts `clientInfo` and optional client `capabilities`.
- The observed response includes `userAgent`, `codexHome`, `platformFamily`, and `platformOs`.
- The observed response does not include an explicit protocol version or capability matrix.
- Compatibility probing therefore needs to combine the handshake result with targeted request validation of required methods and fields.
- The 0.125.0 schema declares `capabilities.experimentalApi` as the opt-in for experimental methods and fields. Beryl requests this capability so it can receive new stream metadata such as subagent `agentNickname` fields and `thread/started` notifications.
- The 0.125.0 schema also declares `capabilities.optOutNotificationMethods` for suppressing exact notification methods on a connection. Beryl must not suppress `thread/started` on foreground turn-stream clients because Activity panel subagent labels depend on that metadata.

## Thread Discovery And Reload

- `thread/list` returns persisted thread summaries with pagination state.
- `thread/list` accepts optional `cwd`, `limit`, `cursor`, `sortKey`, and `sortDirection` parameters in the 0.125.0 schema.
- Observed thread summaries include `cwd`, `updatedAt`, and optional `name`; Beryl's member-thread inventory depends on `cwd` for exact workspace-member grouping, `updatedAt` for descending per-member ordering, and `name` as a backend-provided user-facing title when present.
- Persisted historical threads discovered via `thread/list` are commonly returned with `status.type = "notLoaded"`.
- `thread/start` creates a new thread and returns thread metadata including runtime policy details such as model, nullable reasoning effort, sandbox, and approval policy.
- `thread/resume` returns the same top-level runtime policy fields as `thread/start`, including model and nullable reasoning effort.
- `thread/resume` accepts `excludeTurns = true` in the 0.125.0 schema, which returns thread metadata and live-resume state without populating `thread.turns`.
- `thread/read` with `includeTurns = false` returns thread metadata only.
- `thread/read` with `includeTurns = true` returns historical turns for an existing thread even when the thread remains `notLoaded`.
- `thread/read` does not expose top-level model or reasoning-effort fields in the observed 0.125.0 and 0.128.0 schemas or live 0.128.0 responses; callers that only have a `thread/read` response should keep those status values unknown unless they were learned from another exact source. Beryl normalizes optional top-level model and reasoning-effort fields on `thread/read` only for protocol versions that expose them.
- `thread/resume` also returns historical turns when `excludeTurns` is omitted or false and can be used to reactivate a persisted thread for further work.
- `thread/turns/list` exists in the 0.125.0 schema and accepts `threadId`, optional `cursor`, optional `limit`, and optional `sortDirection`.
- `thread/turns/list` returns turn pages with `data`, `nextCursor`, and `backwardsCursor`.
- After `thread/resume`, the same thread is observed as loaded with `status.type = "idle"` for an idle historical thread.
- `thread/loaded/list` exposes currently loaded in-memory threads for the current connection.
- `thread/name/updated` is present in the 0.125.0 schema as a server notification with `threadId` and nullable `threadName`.
- `thread/name/set` is present in the 0.125.0 schema as a client request with required `threadId` and `name`, and an empty response object on request success. Empty or whitespace-only names are rejected after app-server normalization.
- For a loaded thread, `thread/name/set` queues `Op::SetThreadName` and returns after the op is accepted; the later `thread/name/updated` notification is the durable success signal after the core appends the name to the session index and updates in-memory thread metadata.
- In the installed 0.125.0 source snapshot, `thread/name/set` loads the target through the thread manager and does not directly rename an unloaded persisted rollout. Beryl-created target threads are already loaded after `thread/start`, so automatic naming can set them directly. If a future flow needs to name an unloaded external thread, it must resume/load that thread first.
- Thread names set through `thread/name/set` are surfaced through `thread/read`, `thread/list`, and `thread/resume`. They are stored as thread-name metadata in the session index rather than as target transcript items, and `ThreadNameUpdated` events are not persisted into the rollout history.
- `thread/start` accepts optional `ephemeral` in the 0.125.0 schema. Beryl may use app-server ephemeral threads only through its title-generation maintenance boundary, and those maintenance threads must not enter user-facing thread inventory or activation state.
- `thread/start` accepts optional `dynamicTools` in the 0.128.0 experimental schema. Each dynamic tool spec requires `name`, `description`, and `inputSchema`, and may include `namespace` and `deferLoading`.
- In the inspected 0.128.0 experimental schema, `dynamicTools` is present on `thread/start`; it is not present on `turn/start` or `thread/resume`.
- Beryl-owned dynamic tools that need to be available during a conversation thread, including lifecycle yield tools, can therefore be registered only when Beryl creates the thread unless a later app-server contract adds dynamic tool registration on resume or turn start.
- In the generated stable 0.128.0 schema, `turn/start` has no `developerInstructions` field or equivalent standalone per-turn developer-instructions request field. The generated experimental 0.128.0 schema exposes `turn/start.collaborationMode.settings.developer_instructions`, with `collaborationMode.mode` set to `default` or `plan` and `settings.model` required. The schema describes `collaborationMode` as taking precedence over model, reasoning effort, and developer instructions; `null` developer instructions means app-server uses the built-in instructions for the selected mode. `thread/start.developerInstructions` remains available for new threads.
- `thread/unsubscribe` is present in the 0.125.0 schema as a client request with `threadId`; its response status is `notLoaded`, `notSubscribed`, or `unsubscribed`.
- Local app-server source and tests for `thread/unsubscribe` show that when the caller is the last subscriber, app-server responds `unsubscribed`, submits thread shutdown asynchronously, removes loaded thread state, publishes `notLoaded` status, emits `thread/closed`, and `thread/loaded/list` no longer includes that thread.
- `thread/unsubscribe` during an active turn interrupts the turn and still emits `thread/closed` after shutdown completes. A second unsubscribe after unload returns `notLoaded`. If shutdown submission fails or times out, app-server logs the failure and may not emit `thread/closed`, so callers should treat `thread/closed` or subsequent not-loaded state as cleanup completion rather than assuming the `unsubscribed` response means the thread is already unloaded.
- For title generation, Beryl should create one fresh ephemeral thread per title attempt and call `thread/unsubscribe` after the attempt reaches a terminal state. This is cleanup of an ephemeral loaded thread, not deletion of persisted conversation history.
- The app-server contract exposes stored thread-name metadata, but Beryl must not assume app-server auto-generates a name after `turn/completed`.
- The observed app-server schema and local source snapshots do not expose a direct title-generation request or standalone non-history model invocation.
- The 0.125.0 `Thread` schema describes `name` as an optional user-facing thread title and `preview` as usually the first user message.
- `thread/fork` is present in the stable generated 0.128.0 schema. It accepts required `threadId` and optional runtime or configuration overrides; the experimental schema additionally exposes path-based fork and richer history-persistence fields.
- `thread/fork` creates a new backend-owned thread id from the source rollout, does not mutate the original rollout, auto-subscribes the caller to the new thread, emits `thread/started`, and returns a `thread` plus runtime metadata such as model, model provider, cwd, approval policy, sandbox, nullable reasoning effort, and optional instruction sources.
- By default, `thread/fork` returns the forked thread with `thread.turns` populated. `excludeTurns = true` returns metadata without populated turns, which is useful only when the caller intends to load turns through another path.
- The stable and experimental generated 0.128.0 `Thread` schemas include optional `forkedFromId`, described as the source thread id when a thread was created by forking another thread. The field appears in thread list/read/fork/rollback/resume response shapes and the thread-started notification shape.
- Live probing on May 8, 2026 against `codex-cli 0.128.0` showed `thread/list` returning `forkedFromId: null` for a persisted forked thread while metadata-only `thread/read` for the same thread returned the durable parent id stored in the rollout `session_meta.forked_from_id`. Callers that need thread lineage for inventory presentation should treat `thread/read` as the reliable source when list rows lack a parent id.
- The generated 0.128.0 schemas do not expose the source turn id, fork point, or a full lineage object in thread inventory metadata.
- `thread/rollback` is present in the stable generated 0.128.0 schema. It accepts required `threadId` and `numTurns`, where `numTurns` is the number of trailing user turns to drop and must be at least 1. It is not targeted by a turn id.
- `thread/rollback` keeps a chosen target turn only when the caller computes `numTurns` as the number of turns after that target turn. Including the target turn in `numTurns` removes the target turn.
- `thread/rollback` returns the updated `thread` with `turns` populated after pruning. Local source and tests show a rollback of `numTurns = 1` after two turns leaves the first turn present, persists the pruned history, and later `thread/resume` observes the same pruning.
- Source inspection for thread-edit planning shows `thread/rollback` records a `ThreadRolledBack { num_turns }` history event and effective history reconstruction applies that marker by dropping trailing user turns. If `numTurns` exceeds the existing user-turn count, effective history is cleared.
- The inspected 0.128.0 schema and source do not expose turn-id-targeted delete, truncate, tombstone, or an atomic rollback-plus-`turn/start` request. A GUI edit flow must perform rollback and replacement turn start as separate requests.
- `thread/rollback` mutates thread history only; it does not revert local filesystem changes made by dropped turns. Beryl transcript branching only applies rollback to the newly forked thread, leaving the source thread and source workspace filesystem state unchanged. Beryl source-thread editing must likewise treat filesystem and GUI-owned side effects as outside the rollback boundary.
- Observed and source-visible rollback errors include method-not-found for unsupported primitives, invalid request for `numTurns = 0`, invalid request for duplicate rollback on the same thread, invalid request when rollback is attempted while a turn is active, and internal errors for failed rollback execution.
- Fork-specific branch errors include invalid request when fork cannot locate a rollout for the requested source id and internal errors for failed fork execution.
- Rollback and replacement turn start are non-atomic in the observed protocol. If rollback succeeds and the following `turn/start` fails, the thread remains rolled back and the caller must preserve enough GUI state for retry or recovery.

## Turn Execution Stream

- `turn/start` returns immediately with an in-progress turn record and is followed by notifications.
- `turn/start` targets a concrete `threadId`; app-server can start a separate ephemeral maintenance thread and submit a maintenance turn after the target thread id and first user prompt are known without waiting for the target thread's assistant response.
- `turn/start.input` is an ordered array of `UserInput` records. Beryl can send multiple accepted fragments in one backend turn by preserving their text and image content as ordered `UserInput` entries.
- The observed and normalized `UserInput` variants include `text`, `image` with URL, `localImage` with local path, `skill`, and `mention`.
- `localImage` records accepted by `turn/start` do not include a name, label, alt, or caption field in the observed schema. A GUI that wants the model to associate a label such as `Image A` with an image should send adjacent generated text input records such as `Image A:` to establish the label.
- `localImage` submission is path-based in the observed protocol. Beryl cannot submit image bytes while asking app-server to preserve an unrelated GUI-only path for later interpretation.
- `localImage.path` is interpreted by the backend runtime that receives the turn. Host-Windows and WSL-Linux launches therefore need runtime-readable paths rather than assuming a host temp path is visible from both runtimes.
- A WSL-launched app-server needs a Linux path that resolves inside that selected distro. For a Beryl image asset stored under the Windows user profile, the practical path is the selected distro's mounted view of that file, such as `/mnt/c/Users/<user>/.beryl/...`, after Beryl validates the mapping.
- The May 5, 2026 ephemeral probe supports the practical assumption that app-server preserves ordered text/localImage/text input parts through submission and that the model can use that order when the image content is visible.
- `turn/start` accepts optional `model` and `effort` fields as model and reasoning-effort overrides for that turn and subsequent turns on the same thread. Beryl's title-generation maintenance turn sends `effort = "medium"` so title generation does not inherit high or xhigh user defaults.
- `turn/steer` accepts required `threadId`, required `expectedTurnId`, and ordered `input` array using the same `UserInput` record shape as `turn/start`. The `expectedTurnId` is an active-turn precondition; the request fails when it does not match the currently active turn.
- `turn/steer` returns a response containing `turnId`.
- `turn/interrupt` accepts required `threadId` and `turnId`, returns an empty acceptance response, and reaches user-visible completion through a later `turn/completed` notification whose turn status is `interrupted`.
- Current app-server documentation states that `turn/interrupt` requests active-turn cancellation but does not terminate background terminals. Background terminal cleanup is a separate `thread/backgroundTerminals/clean` request.
- In the inspected 0.128.0 experimental schema, `command/exec/terminate` terminates a running command-exec session by client-supplied connection-scoped `processId`. Command-exec notifications and command-execution item schemas may expose `processId`, but a buffered command without an exposed process id is not directly terminable through that method.
- The 0.125.0 schema declares an `activeTurnNotSteerable` error info shape for `turn/start` or `turn/steer` submitted while the current active turn cannot accept same-turn steering. The declared non-steerable turn kinds are `review` and `compact`.
- Local source inspection shows app-server applies `turn/start` model and effort overrides before queuing the user input for that turn.
- The stable 0.125.0 app-server surface does not expose a standalone request that updates a loaded thread's model or reasoning effort without also starting a turn. The core protocol has an internal override operation, but app-server currently exposes that behavior through `turn/start`.
- The core submission loop spawns user-turn work and remains available for later submissions, so publishing a generated title through `thread/name/set` does not require waiting for the target assistant response or terminal turn state. For loaded target threads, the resulting `thread/name/updated` notification remains the authoritative success signal.
- For streamed `turn/*` notifications, the `Turn.items` list is not the primary carrier of item data; item bodies arrive through separate `item/*` notifications.
- Observed notifications include `thread/status/changed`, `turn/started`, `item/started`, `item/completed`, `item/agentMessage/delta`, `item/reasoning/summaryPartAdded`, `item/reasoning/summaryTextDelta`, `item/commandExecution/outputDelta`, `thread/tokenUsage/updated`, `turn/diff/updated`, and `turn/completed`.
- The 0.125.0 schema additionally declares `thread/name/updated`; Beryl should normalize it as backend thread-name metadata when received.
- Live reasoning is exposed as its own `reasoning` item type. It is not folded into `agentMessage`.
- The 0.128.0 experimental schema declares reasoning lifecycle items, `item/reasoning/summaryPartAdded`, `item/reasoning/summaryTextDelta`, `item/reasoning/textDelta`, completed reasoning `summary` arrays, and token usage with `reasoningOutputTokens`. Live probes observed reasoning item start/completion without summary text for some turns and summary text deltas for other turns. Beryl should therefore treat reasoning summary text as optional activity detail.
- CAS does not expose raw chain-of-thought as a stable user-facing activity contract. For the Activity panel, Beryl should show `reasoning` when only lifecycle is available and `reasoning: <summary>` only from summary text fields. Raw `item/reasoning/textDelta` content is not a user-facing activity detail.
- Live shell/tool execution is exposed as `commandExecution` items, including the spawned command string when available, with output streamed through `item/commandExecution/outputDelta`.
- Long-running shell execution is represented as a long-lived `commandExecution` item rather than as an immediate-start/immediate-complete placeholder.
- The 0.125.0 and 0.128.0 schemas expose native and dynamic operational activity as distinct `ThreadItem` types rather than a single universal MCP envelope. Declared operational item types include `commandExecution`, `fileChange`, `mcpToolCall`, `dynamicToolCall`, `collabAgentToolCall`, `webSearch`, `imageView`, `imageGeneration`, and `contextCompaction`.
- `mcpToolCall` is the app-server thread item for external MCP server tool calls. It includes `server`, `tool`, `arguments`, `status`, optional `mcpAppResourceUri`, and optional completion metadata such as `result`, `error`, and `durationMs`.
- `dynamicToolCall` includes `namespace`, `tool`, `arguments`, `status`, optional returned content items, optional success, and optional duration.
- App-server asks the client to execute a registered dynamic tool through the server request method `item/tool/call`.
- `item/tool/call` parameters require `threadId`, `turnId`, `callId`, `tool`, and `arguments`, with optional `namespace`.
- The dynamic tool-call response requires `success` and `contentItems`; output content items can carry text or image content, so Beryl's graph tools can return compact JSON text results without introducing a separate transport.
- A Beryl lifecycle `yield` tool can be represented as an ordinary namespaced dynamic tool call and response at this protocol layer. App-server does not provide a host-level compact-and-restart primitive to the dynamic tool itself, so Beryl must treat the tool result as an app-shell lifecycle request and perform any compaction or resume only after the active turn reaches backend-reported terminal state.
- `collabAgentToolCall` includes the collab tool name, sender thread id, receiver thread ids, target-agent state map, status, and optional spawn prompt/model/effort fields. The generated 0.128.0 schema declares item-level `model` and `reasoningEffort` fields for collab-agent activity, which are exact activity metadata when present rather than values to infer from defaults. In the inspected app-server schema, `agentsStates` maps receiver thread ids to status/message records; it is not the source of subagent nicknames. A May 3, 2026 live probe against `codex-cli 0.128.0` confirmed that spawn and wait item-completion payloads carry receiver thread ids without nickname fields.
- Backend `Thread` metadata includes optional top-level `agentNickname` for AgentControl-spawned subagents. The 0.125.0 and 0.128.0 schemas can also expose the same nickname through nested source metadata at `source.subAgent.thread_spawn.agent_nickname`. A metadata-only `thread/read` response for the spawned child thread is the reliable app-server source observed for subagent nicknames.
- A separate initialized WebSocket client can call metadata-only `thread/read` for a child thread id immediately after `spawnAgent` completion and receive `Thread.agentNickname` without transferring transcript turns. Beryl treats `collabAgentToolCall.receiverThreadIds` as nickname-resolution keys, not display labels.
- The raw `codex/event/collab_agent_spawn_end` stream notification may carry the spawned subagent thread id and backend-chosen nickname on some protocol paths. This notification is an opportunistic label source only; Beryl must not depend on it as the sole live nickname source.
- `webSearch`, `imageView`, `imageGeneration`, and `contextCompaction` are separate native item types with their own fields rather than MCP tool calls.
- The observed `imageGeneration` thread item includes `id`, `type = "imageGeneration"`, `status`, `revisedPrompt`, `result`, and `savedPath`.
- The observed `imageGeneration.result` is raw PNG base64, not a `data:` URL. `savedPath` points at the same generated PNG under the Codex-generated-images directory for the thread.
- The observed `imageGeneration.status` was still `generating` after `result` and `savedPath` were present. Consumers that render generated images should prefer usable image bytes or a usable saved path over status alone when deciding whether the final image is available.
- The observed image-generation flow exposed no progressive image deltas or intermediate raster stages. A client should model this as placeholder while bytes/path are absent, then final image when bytes/path arrive.
- The observed app-server surface does not expose a structured image-generation size or aspect-ratio parameter. Any Beryl default for generated-image dimensions must be expressed through conversation or developer-instruction text, while transcript rendering should preserve the actual raster dimensions returned by app-server.
- `item/mcpToolCall/progress` exists for MCP tool-call progress updates and includes `threadId`, `turnId`, `itemId`, and a progress message.
- `mcpServerStatus/list` lists configured external MCP server inventory. Its tool records can include `name`, nullable `title`, nullable `description`, and schemas, but this inventory is not the registry for native app-server tool activity. The local May 3, 2026 live probe reported no configured external MCP tools in the current environment.
- `mcpServer/resource/read` exists as a client request for MCP resources, but the inspected schema did not show a corresponding live thread item or progress notification for a running resource read.
- The terminal assistant response for a completed turn is exposed as an `agentMessage` item whose `phase` is `final_answer`.
- Agent-message phases are backend protocol metadata; `doc/design.md` owns which parent assistant messages are transcript narrative and which operational stream items are excluded from transcript presentation.

## Status Line Operations

- `model/list` is present in the stable 0.125.0 schema and returns paginated model records with `id`, `model`, `displayName`, `hidden`, `supportedReasoningEfforts`, `defaultReasoningEffort`, input modalities, personality support, and default-model metadata.
- The live `model/list` probe succeeded after `initialize`, so Beryl can populate the model/reasoning popup from app-server rather than hardcoding model ids or reasoning options.
- `config/read` is present in the stable 0.125.0 schema, accepts an optional `cwd`, and returns a `config` object that can include `model` and nullable `model_reasoning_effort`.
- In a configured workspace, `config/read.config.model_reasoning_effort` returned `xhigh` while `model/list.defaultReasoningEffort` for the default model returned `medium`; a disposable ephemeral `thread/start` for the same workspace returned `reasoningEffort = xhigh`.
- With an empty temporary `CODEX_HOME`, `config/read` returned no model or reasoning configuration and a disposable ephemeral `thread/start` returned the default model with nullable reasoning. Beryl should therefore show unknown reasoning when `config/read` does not expose a reasoning value instead of inferring one from `model/list.defaultReasoningEffort`.
- Status-line model/reasoning changes for an already selected idle thread, or for a pending new-thread draft before its first submitted turn, must be held by Beryl until the next real user turn, then sent through `turn/start.model` and `turn/start.effort`.
- A pending new-thread draft with no explicit GUI selection should not require Beryl to synthesize model or effort overrides; its status presentation should follow the current effective app-server defaults that would be used if the first turn were submitted immediately.
- `thread/compact/start` is present in the stable 0.125.0 schema, accepts only `threadId`, and returns an empty object after app-server accepts the compaction request.
- Local source inspection shows `thread/compact/start` submits a backend compaction operation for the loaded thread. The response is request acceptance, while compaction progress and completion are represented through the normal thread and item notification stream.
- App-server notification delivery is client-session scoped. A Beryl worker that waits for compaction stream notifications on its own backend client must first subscribe that client to the target thread, such as through metadata-only `thread/resume`, before calling `thread/compact/start`.
- `thread/status/changed` exposes loaded-thread status as `idle`, `active`, `systemError`, or `notLoaded`; `active` includes flags such as waiting on approval or waiting on user input. Beryl can gate loaded-thread status-line operation cells on selected-thread `idle` state.

## Status Metadata

- `thread/tokenUsage/updated` is an observed and schema-declared server notification in `codex-cli 0.125.0`.
- The notification params include `threadId`, `turnId`, and `tokenUsage`.
- `tokenUsage` contains `last`, `total`, and nullable `modelContextWindow`.
- `last` and `total` are token-usage breakdowns with `inputTokens`, `cachedInputTokens`, `outputTokens`, `reasoningOutputTokens`, and `totalTokens`.
- The schema does not expose a direct context-space-left percentage.
- For the UI status strip, Beryl can derive a selected-thread context percentage only when `modelContextWindow` is non-null and positive; the planned derivation uses `last.inputTokens`, and cumulative `tokenUsage.total` is spend accounting that should not be used as current context occupancy.
- The observed `codex-cli 0.125.0` schema exposes token usage through `thread/tokenUsage/updated`; it does not expose latest per-thread token usage through `thread/resume`, `thread/read`, `thread/list`, `thread/turns/list`, or another read-only thread-status response.
- The April 30, 2026 read-through re-check found no backend field for seeding restored-thread context status, so Beryl has no backend normalization work to perform for restored threads in this app-server version beyond normalizing `thread/tokenUsage/updated` notifications.
- Beryl can cache exact `thread/tokenUsage/updated` notifications per thread for thread switches within the same GUI process and can persist those exact notification payloads as GUI-held last-known snapshots for app restarts.
- The `codex-cli 0.128.0` schema exposes `account/rateLimits/updated` as a server notification whose params contain `rateLimits`.
- `rateLimits.primary` and `rateLimits.secondary` are nullable windows with required `usedPercent` and optional `windowDurationMins` and `resetsAt`.
- Each `RateLimitSnapshot` may include nullable `limitId` and `limitName`; Beryl preserves these fields so the UI can select the active model's bucket instead of merging unrelated buckets.
- The schema also exposes `account/rateLimits/read` with `params = null`, returning a backward-compatible `rateLimits` shape plus optional `rateLimitsByLimitId`.
- `rateLimitsByLimitId` is a multi-bucket map keyed by metered `limit_id` values such as `codex`; Beryl should prefer the active model's exact bucket, use the `codex` bucket for non-Spark Codex models, and avoid merging Spark limits into main-model status.
- Streamed `account/rateLimits/updated` payloads expose only the legacy single-snapshot shape, so Beryl treats them as partial bucket updates and must not clear a short-window or weekly bucket merely because that bucket is absent from one notification.
- For the UI status strip, Beryl may derive short-window or weekly remaining percentages only from exact window `usedPercent` values where `windowDurationMins` identifies the bucket. Remaining is `100 - usedPercent`, clamped to `0..100`.
- A durable snapshot is exact for the notification Beryl observed, but it remains a last-known status value and may be stale if the same backend thread changes outside this GUI state.
- Restored threads without an in-memory or durable exact usage snapshot must keep context space as `Unknown` unless a future app-server version exposes read-only latest token usage.
- Beryl must not send synthetic user input or create a backend turn just to force a token-usage notification.
- The 0.128.0 generated schema declares `remoteControl/status/changed` as a server notification with required status and optional nullable `environmentId`. Declared status values are `disabled`, `connecting`, `connected`, and `errored`. Beryl does not currently consume this notification for UI state.

## Artifacts And Citations

- File edits can surface as `fileChange` items.
- Local artifact paths are available at `fileChange.changes[].path` when the backend emits file-change records.
- Historical or streamed assistant messages may include `memoryCitation` entries with path and line metadata.
- Shell command execution records do not by themselves guarantee per-file paths, so the GUI should only treat explicit `fileChange` records as authoritative filesystem artifacts.
- App-server exposes native generated images as `imageGeneration` thread items with embedded raster bytes or a generated-image saved path when available. This is a specific native item type, not a generic artifact attachment model.
- App-server does not expose a generic durable "attach this arbitrary file to the thread" artifact primitive in the observed schema. Non-image generated artifacts such as SVG, CSV, or other files remain ordinary workspace filesystem state plus any transcript text, Markdown link, file-change record, or command output that mentions them.
- Markdown image syntax in assistant text is durable as transcript text, but app-server does not attach the target file bytes to that Markdown expression. A GUI that wants to render such a reference must resolve and read the referenced path at display time.

## Historical Persistence Caveats

- Historical `thread/read` and `thread/resume` are lossy relative to the live notification stream. Streaming deltas are not reconstructed as deltas later.
- When a thread is started with `persistExtendedHistory = false`, historical reads can collapse to mostly `userMessage` and `agentMessage` items even if the live turn emitted `commandExecution` and `reasoning`.
- When a thread is started with `persistExtendedHistory = true`, historical reads can retain `commandExecution` items and other richer execution records.
- Reasoning persistence is gated more narrowly than command persistence. With `persistExtendedHistory = true` and reasoning summaries enabled, historical reads can retain standalone `reasoning` items.
- With `persistExtendedHistory = true` but turn `summary = "none"`, live reasoning can still stream as `reasoning` items and reasoning delta notifications while historical reads omit standalone `reasoning` items and still retain `commandExecution`.
- Historical `localImage` user-message content is path-only in the observed 0.128.0 protocol. `thread/read` and `thread/turns/list` do not embed image bytes, labels, names, alt text, or captions for those records.
- `fs/readFile` can retrieve bytes for a historical `localImage.path` only when that path still exists and is readable by the app-server runtime. It is a recovery/import mechanism for reachable files, not durable transcript image storage.
- Beryl needs its own durable image asset store for pasted images when transcript image-marker preview must survive app restarts or app-server temp-file cleanup.
- Historical `imageGeneration` items can retain embedded result bytes and/or a saved generated-image path, but clients should handle either field being absent or stale because the observed protocol does not present a separate generic artifact-retention contract.
- Historical Markdown file references are only as durable as the transcript text that contains them. If the referenced file has moved, changed, or disappeared, app-server has no observed attachment payload that can reconstruct the prior bytes.

## Error And Recovery Surface

- Unknown methods return structured JSON-RPC errors through the active app-server transport.
- Malformed transport input can produce stderr deserialization logs instead of structured protocol errors.
- The local probe observed non-fatal startup stderr warnings unrelated to the core request flow, so the GUI must tolerate noisy stderr while still treating protocol failures as request-level or process-level failures.

## Backend Integration Implications

- Beryl's target managed app-server transport is authenticated loopback WebSocket so foreground and background backend clients can connect independently to one managed app-server process.
- Backend integration should normalize streamed items around `agentMessage`, `reasoning`, `commandExecution`, `fileChange`, request errors, and thread lifecycle notifications.
- Blocking incompatibility handling should be based on handshake plus required-method probing rather than a declared backend capability manifest.
