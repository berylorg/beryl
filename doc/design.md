# Goals

Build a responsive, low-memory desktop GUI for Codex in which one Beryl home owns durable application state, Syndic owns captured conversation history, and multiple independent conversation windows share process-wide runtime services safely.

Let users create, browse, branch, edit, and resume durable threads without making Codex App Server catalogs or historical reads authoritative and without requiring CAS before the shell can open.

## Non-goals

- Reimplementing Codex authentication, execution, sandboxing, approvals, configuration, skills, MCP, subagents, or enterprise policy.
- Restricting the client to Windows only.
- Retaining workspaces, semantic graph, graph upkeep, checklists, or checklist-bound threaded decisions behind renamed models or compatibility adapters.
- Implementing graph-independent semantic search or turn/resource garbage collection.
- Providing built-in file diff or agent-edit review workflows in V1.

# Decisions

## Documentation Authority

- Root `doc/design.md` owns project-level goals, non-goals, and global constraints.
- `doc/features/<feature>/design.md` owns user-visible behavior, workflows, visible state, disabled and error behavior, and acceptance rules.
- `doc/systems/<system>/design.md` owns cross-feature and cross-package architecture, durable models, consistency, lifecycle, recovery, and provider integration.
- Package `doc/design.md` files own only their package's public boundary.
- `doc/gui/integration.md` owns OS windows and GUI slots; linked feature `gui.md` files own mounted composition.
- `doc/input-hotkeys.md` owns shared baseline text-input behavior; feature-specific commands remain in feature docs.
- Active architectural replacements are tracked by `doc/rework/<name>/REWORK.md`, while target-state authority stays in normal feature, system, package, and GUI paths.

## Feature Design Entry Points

- Beryl-home opening, busy-home behavior, unreadable startup, and running store failure are defined in `doc/features/beryl-home/design.md`.
- Independent main windows, ordinary close, dedicated Exit, session restore, and virtual-desktop placement are defined in `doc/features/main-windows/design.md`.
- Runtime/CAS unavailability and visible recovery are defined in `doc/features/backend-runtime-recovery/design.md`.
- Thread creation, executable-path runtime/root configuration, catalog scope, activation, window occupancy, navigation, lineage, generated titles, and the current immutable binding/management boundary are defined in `doc/features/conversation-threads/design.md`.
- Graph-independent branch discussion and parent resolution handoff are defined in `doc/features/branch-discussions/design.md`.
- Durable composer behavior, submission, steering/queueing, composer history, quote insertion, and developer-instructions injection are defined in `doc/features/composer/design.md`.
- Image-asset ownership and visible lifecycle are defined in `doc/features/image-assets/design.md`.
- Transcript narrative, scrolling, media, selection, quote and branch harvesting, menus, and anchoring are defined in `doc/features/transcript/design.md`.
- Model/reasoning, context/rate-limit status, compaction, view counts, and stop controls are defined in `doc/features/status-line/design.md`.
- Bounded live activity is defined in `doc/features/activity-panel/design.md`.
- Settings-window behavior and Beryl-owned preference persistence are defined in `doc/features/settings/design.md`.
- User-visible theme selection, editing, validation, fallback, preview, and repository-command
  outcomes are defined by the [Theming feature](features/theming/design.md).
- Main-window notices and attention sounds are defined in `doc/features/notifications/design.md`.
- AI lifecycle yield behavior is defined in `doc/features/lifecycle-yield/design.md`.
- Supervisor and isolated-child diagnostics are defined in `doc/features/diagnostics/design.md`.

## System Design Entry Points

- The physical Beryl-home database, typed domains, lock, durability, sessions, claims, catalog, and health gate are defined in `doc/systems/beryl-home-storage/design.md`.
- Syndic threads, drafts, turn DAG, transcript views, projections, references, resources, and replay are defined in `doc/systems/syndic-conversation-history/design.md`.
- Exclusive CAS projections, live capture, CAS-native lineage reuse, one-time recovered-history injection, and active-turn coordination are defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Branch resolution admission, durable handoff jobs, idempotency, and archive ordering are defined in `doc/systems/branch-discussion-handoff/design.md`.
- Image content addressing, references, sidecars, and Host/WSL projection are defined in `doc/systems/image-assets/design.md`.
- Backend runtime launch, listener security, managed lifecycle, exact release admission, coalesced warm-up, and connection recovery are defined in `doc/systems/backend-runtime/design.md`.
- Transcript residency, presentation, renderer demand, resources, diagnostics, and scroll architecture are defined in `doc/systems/transcript-presentation/design.md`.
- Theme schema resolution, installed-repository coordination, appearance generation publication,
  cross-window application, and preview arbitration are defined by the
  [theme runtime system](systems/theme-runtime/design.md).
- Risk-based limits for large-data streaming, paging, queues, caches, decode and layout expansion,
  media, and GPU working sets are defined in `doc/systems/bounded-resource-dataflow/design.md`.

## Implementation Technology

- Beryl-owned code is Rust only.
- The desktop client uses `gpui` and does not depend on browser technologies, JavaScript toolchains, Node.js, WebView wrappers, or non-Rust native application libraries.
- Beryl may use the official `gpui` package or a Beryl-maintained fork anchored to upstream Zed `gpui` when targeted patches are required.
- GPUI-owned transitive native build dependencies remain allowed.
- A Beryl GPUI fork preserves GPUI's public boundary and may gate GPUI HTTP integration behind an opt-in feature when Beryl does not consume it.
- Beryl consumes standalone app-neutral `gpui-text-input`, `gpui-settings-window`, and `gpui-scrollbar` packages where practical and keeps product semantics in Beryl-owned adapters.

## Backend Boundary

- Agent execution, live event streams, authentication, sandboxing, approvals, tools, skills, MCP, subagents, managed configuration, and enterprise policy flow through out-of-process `codex app-server`.
- Unmodified out-of-process `codex app-server` is Beryl's sole agent-execution provider; Beryl does not implement a replacement, fork, embedded Codex runtime, or independently operated CAS-compatible provider.
- Beryl does not bundle, install, modify, or directly link Codex internal crates.
- Beryl may launch and supervise app-server processes and may implement narrow GUI-side orchestration from public protocol primitives.
- Cross-boundary communication uses the app-server contract rather than Codex storage, process memory, or internal implementation details.
- Syndic captures accepted live events and remains the canonical durable history authority for captured threads. CAS remains the source of live execution events, not ordinary captured transcript reads.
- When live capture proves or conservatively suspects any gap in one exact correlated turn,
  CAS-live may read at most one exact terminal snapshot for that repair-required turn and commit
  the bounded whole-turn repair through Syndic. Outside that exception, Beryl never repopulates
  captured Syndic history from CAS historical transcript methods.
- GPUI transcript code consumes Syndic only through the transcript provider and residency boundaries and never reaches durable storage or backend process state directly.

## Codex App Server Version Invariant

- This Beryl version targets exactly `codex-cli 0.146.0` / Codex App Server 0.146.0.
- Beryl carries no runtime branches for older schemas or speculative future schemas.
- Runtime admission fails closed unless the exact version and required effective configuration are
  proven under the [backend-runtime contract](systems/backend-runtime/design.md). Beryl does not
  infer compatibility by probing user or synthetic work.
- CAS-native collaboration, continuation, lineage, recovery injection, and branch-context mechanics
  remain provider-owned capabilities governed by the [backend-runtime](systems/backend-runtime/design.md)
  and [CAS-live Syndic transcript](systems/cas-live-syndic-transcript/design.md) systems; Beryl does
  not replace them with application-private equivalents.
- An incompatible configured app-server disables affected backend work instead of falling back to another schema.
- Upgrading the target replaces this single invariant and refreshes its memory evidence, feature/system contracts, normalized package boundary, and tests together.
- CAS thread lists, thread names, full-history `thread/read`, `thread/turns/list`, and item-history
  reads are not Beryl catalog, title, restore, or ordinary captured-history inputs. The one exact
  correlated terminal snapshot allowed for a repair-required turn is the sole bounded exception.

## Responsibility Split

- Syndic owns stable threads, each thread's immutable execution binding and intrinsic properties,
  exactly one current durable draft per thread, submitted turns, immutable parentage, canonical
  captured events, transcript projections, resources, and exclusive CAS projection bindings.
- Beryl-home state owns executable-path runtimes, configured roots, runtime/root availability,
  window/session records and thread claims, settings, paged asset-reference sets
  and compact owner heads, durable host jobs, and rebuildable compact catalog projections.
- The theme runtime coordinates the Beryl-home installed-theme repository and owns canonical theme
  resolution plus the process-wide durable and preview appearance generations.
- CAS owns live execution and all Codex policy-sensitive behavior.
- Main windows own only their selection, navigation, presentation, focus, transient interaction, and in-memory editing projection over the selected durable draft.
- Resident transcript data, activity records, caches, diagnostics, and render state are bounded runtime projections and never replace durable authority.
- Thread editing changes one selected Syndic path. It never promises rollback of filesystem changes, settings, assets, activity records, other threads, or external side effects.
- Beryl exposes no manual thread deletion or automatic empty-thread cleanup; durable turns and resources remain until a future explicit management and garbage-collection design.

## Cross-Feature Safety Rules

- Beryl never silently substitutes another Beryl home, runtime, root, backend process, Syndic thread, CAS thread, turn, window, parent, or stop target when the requested identity cannot be used exactly.
- Beryl never starts synthetic backend turns solely to refresh status chrome, apply pending model settings, enumerate history, or decorate activity rows.
- Beryl never copies CAS transcript history into a local compatibility model to emulate missing branch, edit, rollback, resume, or materialization behavior.
- Beryl does not satisfy an architectural contract through lossy encoding, repeated context replay, duplicated authority, undocumented fallback behavior, compatibility glue, or another expedient workaround. If an exact clean design cannot be supported by the targeted dependency boundary, work stops at an explicit architectural blocker until the controlling authority changes.
- A resilience fallback may run only when its exact primary path is unavailable or unprovable. It must not quietly replace the primary path merely because it is easier to implement.
- Correctness-sensitive generated input uses current authoritative Syndic and Beryl-home revisions or rejects; stale projections and caches are insufficient.
- Exact CAS soft interruption is Beryl's sole turn-stop mechanism. Beryl exposes no product,
  control, or diagnostic hard stop; process termination, coarse cleanup, guessed OS pids, process
  names, working directories, and process trees never become turn-stop authority.
- V1 denies app-server approval requests during Beryl-managed turns, prefers a denial that interrupts when supported, logs bounded redacted diagnostic evidence, and avoids leaving turns waiting indefinitely.
- Quiet live streams are not failures. Bounded request timeouts do not impose an inactivity timeout on active streams.

## Persistence

- The configured Beryl home contains all Beryl-owned durable state and exactly one physical Fjall database for Syndic and Beryl metadata domains.
- Logical ownership remains separated by typed keyspaces and APIs even though the physical database is shared.
- Beryl-home sidecars contain durable image and heavy Syndic resources. Installed theme documents
  remain in the home repository governed by the [theme runtime system](systems/theme-runtime/design.md)
  over the [Beryl-home storage boundary](systems/beryl-home-storage/design.md).
- Beryl does not store Codex authentication, Codex configuration, skills, MCP state, capability tokens, or backend-owned policy state.
- One OS process owns a home at a time; multiple main windows share that process and store.
- Correctness-sensitive accepted mutations complete the durability barrier defined by the Beryl-home storage system before they are reported saved.
- Large Syndic text is logical durable state assembled from bounded records. A Fjall value ceiling or internal chunk threshold is never a whole-draft, whole-user-input, or whole-canonical-item product limit.
- Old workspace-era persisted state is not read, imported, dual-written, or adapted by the target architecture.
- Derived catalog, transcript-presentation, search, media, activity, and diagnostic projections are not authoritative when their source records can rebuild them.

## Responsiveness And Performance

- Input and render latency, RAM use, and CPU use are first-order constraints.
- The GPUI thread performs no blocking filesystem, process, network, parsing, image decode, persistence, or backend protocol work.
- Established coherent content remains visible during asynchronous replacement whenever possible; Beryl does not flicker through temporary blank or opening surfaces.
- Selected-thread content and its initial viewport publish in one transaction and are not corrected by later render callbacks.
- Catalog metadata and large durable collections remain paged from durable indexes. GUI row
  construction is virtualized over revision-bound query pages, and each owning cache has a
  practical item or byte budget.
- The major CAS-to-Syndic, Syndic-to-Beryl, Beryl-to-CAS, and Beryl-to-renderer paths use enforced
  payload, page, queue, cache, concurrency, decode, layout, pixel, or GPU limits at their actual
  amplification and accumulation points.
- Operations over large exact durable content use paging, streaming, or bounded staged commits
  where practical. A dependency that necessarily materializes a whole value is governed by a
  documented generous operation limit rather than reconstructed private allocation accounting.
- Background work is bounded, cancellable, and lower priority than foreground turn streaming and selected transcript activation.
- Implementation favors predictable latency, backpressure, eviction, and explicit unavailability
  over unbounded queues, caches, decode expansion, or renderer retention. Canonical content is
  never silently truncated, but explicit product limits are allowed where an external API requires
  a contiguous whole value.

## Platform Targeting

- Windows is the primary product-quality and developer-attention target.
- The architecture preserves operation on other GPUI-supported platforms when the backend and windowing boundaries permit it.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `persistent-state-integrity/v1`
- `shared-resource-protection/v1`
