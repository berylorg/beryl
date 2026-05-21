# Goals

Build a desktop GUI client for Codex that organizes user work as Beryl-owned semantic workspaces with low memory footprint, responsive UI, and clear ownership between GUI-local state and backend-owned Codex state.

## Non-goals

- Reimplementing Codex authentication, session storage, configuration, skills, MCP, subagent orchestration, or other non-UI agent behavior in this project.
- Restricting the client to Windows only.
- Guaranteeing that AI-maintained semantic graph updates are always correct without user review or later repair.
- Making backend conversation history, filesystem roots, or app-server thread inventories the authoritative source of Beryl workspace identity.
- Providing built-in file diff views, change review workflows, or other agent edit inspection UI in V1.

# Decisions

## Documentation Authority

- Root `doc/design.md` owns cross-feature, cross-package, and shared architecture decisions for this workspace project.
- Feature-owned product behavior, UI contracts, state ownership, persistence, async behavior, failure behavior, and cross-package implementation architecture are defined in `doc/features/<feature>/design.md`.
- Package `doc/design.md` files own package public boundary contracts and must not duplicate or contradict feature contracts.
- `doc/product-features.md` is a navigational index rather than the authority for detailed product behavior.
- `doc/ui.md` owns shared reusable UI mechanics, window rules, widgets, and scroll contracts that are not specific to one product feature.
- `doc/input-hotkeys.md` owns shared baseline text-input behavior. Feature-specific field behavior belongs in the owning feature doc.

## Feature Design Entry Points

- Workspace startup, workspace identity, workspace picker, runtime environments, workspace members, and workspace persistence are defined in `doc/features/workspaces/design.md`.
- Backend runtime availability, managed app-server lifecycle, capability probing, backend-unavailable states, and connection-loss recovery are defined in `doc/features/backend-runtime-recovery/design.md`.
- Conversation thread selection, activation, inventory, binding, branch/edit workflows, automatic thread-title generation, and user-initiated thread-title updates are defined in `doc/features/conversation-threads/design.md`.
- Threaded decision workflows that bind checklist items to decision child branches, parent handoff turns, resolution outcomes, and child cleanup are defined in `doc/features/threaded-decisions/design.md`.
- Composer behavior, draft submission, image input, input queues, composer history, quote insertion, and developer-instructions injection on user turns are defined in `doc/features/composer/design.md`.
- Transcript rendering, Markdown, media, selection, quote harvesting, turn context menus, history pagination, and transcript scroll anchoring are defined in `doc/features/transcript/design.md`.
- Status line behavior, model/reasoning controls, context/rate-limit display, context compaction controls, and turn stop controls are defined in `doc/features/status-line/design.md`.
- Activity panel behavior and activity projection are defined in `doc/features/activity-panel/design.md`.
- Semantic graph, graph overlay, primitive graph tools, graph provenance, and markdown/thread refs are defined in `doc/features/semantic-graph/design.md`.
- Graph upkeep, graph upkeep instructions, AI-assisted graph maintenance, on-demand source-ref repair, and graph-upkeep write policy are defined in `doc/features/graph-upkeep/design.md`.
- Semantic search, local knowledge corpus, search dynamic tools, lexical/vector indexing, embedding generation, and search-owned caches are defined in `doc/features/semantic-search/design.md`.
- Settings window shell, settings rows, settings persistence, and settings dynamic tools are defined in `doc/features/settings/design.md`.
- Appearance themes, theme repository, Themes settings page, theme candidate code panels, theme dynamic tools, and theme editor authority are defined in `doc/features/theming/design.md`.
- Surface notices, turn-error notices, end-turn sounds, and attention-trigger behavior are defined in `doc/features/notifications/design.md`.
- AI lifecycle yield tool behavior is defined in `doc/features/lifecycle-yield/design.md`.
- Supervisor diagnostics and diagnostic child control are defined in `doc/features/diagnostics/design.md`.

## Implementation Technology

- Beryl-owned code must be Rust only.
- The desktop client is implemented with `gpui`.
- The application stack must not depend on browser technologies, JavaScript toolchains, Node.js, WebView wrappers, or non-Rust native libraries.
- Beryl may depend on the official `gpui` package or on a fork anchored to upstream Zed `gpui` source when targeted patches are needed to satisfy Beryl's product constraints.
- GPUI-owned native build dependency surface, including transitive C or assembly compilation, remains allowed.
- A Beryl-maintained GPUI fork must preserve GPUI's public boundary for Beryl and must not be used to remove GPUI-owned build dependencies without an explicit design decision.
- Beryl's normal dependency on a Beryl-maintained GPUI fork does not require GPUI's HTTP client capability.
- The fork may gate GPUI-owned HTTP client integration, including `http_client` and `zed-reqwest`, behind an opt-in `gpui` Cargo feature so Beryl builds that do not use GPUI HTTP APIs do not compile that dependency stack.
- The GPUI HTTP-client feature exception is limited to that dependency surface and must preserve GPUI's public HTTP-client boundary for consumers that enable the feature.
- Beryl consumes the standalone `gpui-text-input`, `gpui-settings-window`, and `gpui-scrollbar` crates for reusable text editing, settings-window mechanics, and scrollbar affordances where practical. Beryl-owned feature code adapts those app-neutral boundaries for Beryl-specific behavior.

## Backend Boundary

- Agent execution, transcript history, and Codex-owned state flow through `codex app-server`.
- Beryl integrates with `codex app-server` as an out-of-process GUI client rather than by directly linking Codex internal crates.
- Beryl does not bundle or install Codex.
- Beryl may own narrow GUI-side orchestration using app-server protocol primitives when the app-server protocol does not expose a direct GUI-needed helper.
- Cross-boundary communication uses the app-server contract rather than direct access to backend storage, process memory, or implementation internals.
- Authentication, session storage, agent execution, subagents, configuration, skills, MCP state, and other non-UI agent behavior remain backend-owned.
- Backend conversation thread contents and execution event streams remain backend-owned.
- Beryl may launch and manage app-server processes in V1, but backend process ownership, runtime-target availability, loopback WebSocket auth, capability probing, and recovery behavior are defined by the backend runtime recovery feature.

## Responsibility Split

- The GUI owns presentation, input handling, windowing, desktop integration, semantic workspace state, default-runtime selection, runtime-bound workspace-member registrations, semantic graph state, GUI-local thread refs, thread-title display precedence, automatic and user-initiated thread-title orchestration, derived member-thread inventory snapshots, GUI-local settings, installed themes, and GUI-local persistence.
- Backend conversation history remains backend-owned even when Beryl renders, branches, edits, titles, or links threads.
- Conversation thread editing mutates backend conversation history only. It must not present or assume rollback of filesystem changes, semantic graph/checklist-item mutations, workspace state, thread-title metadata, durable image assets, in-memory activity records, or other non-history side effects.
- Deleting or retitling a Beryl workspace changes only GUI-owned workspace state and must not delete or mutate backend-owned Codex thread history.
- Deleting semantic graph nodes changes only GUI-owned semantic graph state and must not delete or mutate backend-owned Codex thread history.
- User-facing backend status metadata is presentation state derived from exact app-server responses, notifications, and GUI-held projections. Missing backend fields render as unknown or are omitted rather than guessed.
- User-facing activity is transient presentation state derived from backend execution stream notifications and bounded GUI-derived records. It is not backend conversation history or durable workspace content.
- User-visible turn-completion and lifecycle notifications are GUI-local desktop notification side effects and must not affect backend turn completion semantics.

## Cross-Feature Safety Rules

- Beryl must not silently switch a requested operation to another runtime target, workspace member, backend process, thread, turn, or stop target when the requested target cannot be used exactly.
- Beryl must not start synthetic backend turns, send synthetic user input, or mutate backend history solely to refresh status chrome, apply model/reasoning choices, or decorate activity rows.
- Beryl must not emulate missing backend fork, rollback, resume, or edit primitives by copying backend-owned transcript history into GUI-local state.
- Beryl must not terminate guessed OS pids, process names, working directories, or local process trees. Hard-stop behavior requires exact backend-exposed handles.
- If `codex app-server` requests user approval during a Beryl-managed turn, V1 denies the request, prefers a denial response that interrupts when the protocol supports it, logs the full backend approval request payload for diagnostics, and avoids leaving the turn waiting indefinitely.
- Turn execution stream inactivity is not itself backend failure. Request and probe timeouts apply to bounded JSON-RPC requests; active streams may remain quiet until terminal events, protocol error, transport disconnect, or backend process exit.

## Persistence

- The configured Beryl home directory owns GUI-controlled durable app state, including shared cross-workspace metadata, workspace-scoped state, workspace image assets, `preferences.toml`, and the installed theme repository.
- The Beryl home directory does not own backend-managed Codex authentication, session storage, configuration, skills, MCP state, or conversation execution history.
- Changing the configured Beryl home directory does not change any runtime environment's home directory used for implicit workspace members.
- GUI-owned user settings persist separately from backend-owned Codex configuration.
- Workspace-scoped GUI-local state is stored under the Beryl home directory's `workspaces/` child.
- Workspace-scoped GUI-local state must use an embedded pure-Rust storage engine.
- The design does not require multiple GUI instances to perform concurrent writes to the same workspace-scoped state store, because one GUI instance owns one workspace at a time.
- Mutable GUI-local cursors such as active workspace selection, active thread selection, window state, and splitter positions may use last-write-wins semantics across GUI instances.
- Backend-owned conversation contents and execution history do not move into GUI-local settings or GUI-local state storage.
- Paged transcript data is a transient projection of backend-owned conversation history.
- Derived projections and caches are not authoritative when they can be rebuilt from backend data plus GUI-local metadata.

## Responsiveness And Performance

- UI responsiveness, including input latency and render latency, is a first-order design constraint rather than deferred polish.
- RAM efficiency and CPU efficiency are first-order design constraints.
- The application must not perform blocking filesystem, process, network, parsing, image decoding, persistence, or backend protocol work on the thread that drives `gpui`.
- Interactive code paths must avoid avoidable algorithmic complexity cliffs as transcript size, semantic graph size, workspace count, or backend event volume grows.
- Any Beryl-owned runtime data structure retaining data derived from user input, backend events, filesystem contents, workspace contents, generated output, dependency callbacks, or other externally variable sources must have deterministic growth bounds unless the retained data is exact durable domain state.
- Caches, queues, projections, maps, histories, retry sets, diagnostic buffers, media stores, and dependency-facing handles must either enforce deterministic limits or be documented as exact durable domain state.
- Expensive recomputation on hot paths should be replaced with reasonable caching or incremental maintenance when necessary for responsiveness.
- Background backend clients must be bounded, cancellable, and lower priority than foreground turn streaming and transcript activation.
- Implementation work must prefer predictable latency and bounded resource use over shortcuts that compromise responsiveness.

## Platform Targeting

- Windows is the primary target platform for product quality and developer attention.
- The design must preserve the ability to run the GUI on other platforms supported by `gpui` when the backend boundary permits it.
