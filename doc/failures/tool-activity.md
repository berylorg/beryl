# Tool Activity

## Subagent nickname source

Live testing the Activity panel showed subagent rows still rendering `thread:<id>` after an app-side label-priority fix.

The invalid assumption was that `collabAgentToolCall.agentsStates` carried nickname-like metadata. Local app-server schema and source inspection showed that `agentsStates` maps receiver thread ids to status/message records. Subagent nicknames are exposed on backend `Thread` metadata as `agentNickname`, including through `thread/started` notifications and thread summary/read/list responses.

The course adjustment is to normalize `agentNickname` from backend thread metadata and let the Activity panel label projection learn subagent labels from thread metadata events. Collab-agent tool items remain useful for activity lifecycle and receiver ids, but they are not the nickname source.

## Missing experimental initialize capability

Live testing after thread-metadata normalization still showed subagent rows as `thread:<id>`.

The invalid assumption was that parsing `thread/started` and `agentNickname` was enough for the live foreground stream to deliver those fields. The installed app-server schema gates experimental fields and notifications behind `initialize.params.capabilities.experimentalApi = true`, and Beryl's initialize request only sent `clientInfo`.

The course adjustment at that point was to have every Beryl backend client request the experimental API capability during initialize and to avoid opting out of `thread/started` on foreground turn-stream sessions. The temporary backend-thread-id fallback part of that adjustment was later superseded by the maintenance `thread/read` approach logged below.

## Nested subagent source nickname metadata

Live testing still showed Activity panel subagent rows as `thread:<id>` after Beryl requested the experimental API and parsed top-level `agentNickname`.

The invalid assumption was that app-server would always lift the spawned subagent nickname to the top-level `Thread.agentNickname` field on the response path Beryl observes. The installed 0.125.0 schema and local rollout metadata also expose the nickname through `Thread.source.subAgent.thread_spawn.agent_nickname`, and Beryl was ignoring that nested source metadata.

The course adjustment is for backend thread-summary normalization to derive `agent_nickname` from top-level `agentNickname` first, then fall back to nested subagent source metadata. The Activity panel should keep consuming only the normalized backend field.

## Ignored raw collab spawn completion notification

Live testing still showed Activity panel subagent rows as `thread:<id>` after Beryl normalized top-level and nested thread metadata.

The invalid assumption was that thread metadata would be the first live nickname source observed by the foreground stream. The app-server stream can also emit a raw `codex/event/collab_agent_spawn_end` notification that carries the spawned subagent thread id and backend-chosen nickname, and Beryl was ignoring unsupported raw `codex/event/*` notifications before they reached the Activity projection.

The course adjustment is to normalize that raw collab-agent spawn completion notification into a backend `AgentLabelUpdated` stream event. The app Activity projection owns label priority and row repair, while the conversation surface only passes `Main` for activity on the currently selected parent thread.

## Foreground nickname sources were not reliable

After upgrading to `codex-cli 0.128.0`, an isolated app-server probe still observed `spawnAgent` and `wait` completion payloads carrying child `receiverThreadIds` but no subagent nickname. The observed `thread/started` notification path did not provide a reliable child nickname, and `thread/loaded/list` returned ids only.

The invalid assumption was that foreground stream metadata or raw spawn-completion notifications were sufficient to keep live Activity panel rows from falling back to `thread:<id>`.

The course adjustment is to resolve subagent nicknames through metadata-only `thread/read` on an independent initialized backend client session. `collabAgentToolCall.receiverThreadIds` are resolution keys only, not display labels. Activity rows must keep the agent name empty while unresolved and update when `thread/read` returns the backend-provided nickname.

## Read-only subagent model metadata gap

While planning model/reasoning suffixes for subagent activity labels, Beryl assumed the metadata-only `thread/read` path used for subagent nicknames could also expose exact child-thread model and reasoning effort.

The invalid assumption was that the read-only child-thread metadata source had parity with metadata-only `thread/resume`. Existing app-server contract notes show `thread/start` and `thread/resume` return exact top-level model and nullable reasoning effort, but observed `thread/read` does not expose those runtime fields. `thread/resume` is an activation/subscription primitive, so using it for background activity labels would be a semantic change rather than read-only metadata plumbing.

The course adjustment is to normalize optional exact runtime model/reasoning fields from `thread/read` when a protocol version exposes them, keep missing values unknown, and avoid inferring them from defaults, model-list metadata, thread ids, or nicknames.

## Collab activity model metadata was ignored

After adding optional `thread/read` runtime metadata normalization, live Activity panel testing still showed only subagent nicknames without model/reasoning suffixes.

The invalid assumption was that `thread/read` was the only read-safe source worth wiring for subagent activity model metadata. The installed 0.128.0 app-server schema exposes optional `model` and `reasoningEffort` directly on `collabAgentToolCall` activity items, while live `thread/read` responses for observed child threads still omit top-level runtime fields. `thread/resume` can return exact model/reasoning, but it is an activation/subscription primitive and is not appropriate merely to decorate activity rows.

The course adjustment is to preserve exact `collabAgentToolCall` model/reasoning metadata through backend activity normalization and let the app projection apply it to observed receiver child-thread ids. `thread/read` remains the read-only nickname resolver and a future-compatible runtime metadata source only if the protocol later exposes exact runtime fields there.
