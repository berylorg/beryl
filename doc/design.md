# Goals

Build a responsive, low-memory desktop GUI for Codex in which one Beryl home owns durable application state, Syndic owns captured conversation history, and multiple independent conversation windows share process-wide runtime services safely.

Let users create, browse, branch, edit, and resume durable threads without making Codex App Server catalogs or historical reads authoritative and without requiring CAS before the shell can open.

## Non-goals

- Reimplementing Codex authentication, execution, sandboxing, approvals, configuration, skills, MCP, subagents, or enterprise policy.
- Restricting the client to Windows only.
- Retaining workspaces, semantic graph, graph upkeep, checklists, or checklist-bound threaded decisions behind renamed models or compatibility adapters.
- Implementing graph-independent semantic search, turn/resource garbage collection, or the theme hierarchy/editor redesign in the Beryl-home rework.
- Providing built-in file diff or agent-edit review workflows in V1.

# Decisions

## Documentation Authority

- Root `doc/design.md` owns project-level goals, non-goals, global constraints, and the non-authoritative future-work section after `# Decisions`.
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
- Appearance themes, the installed-theme repository, and the retained theme editor are defined in `doc/features/theming/design.md`.
- Main-window notices and attention sounds are defined in `doc/features/notifications/design.md`.
- AI lifecycle yield behavior is defined in `doc/features/lifecycle-yield/design.md`.
- Supervisor and isolated-child diagnostics are defined in `doc/features/diagnostics/design.md`.

## System Design Entry Points

- The physical Beryl-home database, typed domains, lock, durability, sessions, claims, catalog, and health gate are defined in `doc/systems/beryl-home-storage/design.md`.
- Syndic threads, drafts, turn DAG, transcript views, projections, references, resources, and replay are defined in `doc/systems/syndic-conversation-history/design.md`.
- Exclusive CAS projections, live capture, CAS-native lineage reuse, one-time recovered-history injection, and active-turn coordination are defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Branch resolution admission, durable handoff jobs, idempotency, and archive ordering are defined in `doc/systems/branch-discussion-handoff/design.md`.
- Image content addressing, references, sidecars, and Host/WSL projection are defined in `doc/systems/image-assets/design.md`.
- Backend runtime launch, listener security, managed lifecycle, capability probing, coalesced warm-up, and connection recovery are defined in `doc/systems/backend-runtime/design.md`.
- Transcript residency, presentation, renderer demand, resources, diagnostics, and scroll architecture are defined in `doc/systems/transcript-presentation/design.md`.
- Codex-compatible replacement-agent constraints are defined in `doc/systems/codex-compatible-agent-layer/design.md`.

## Implementation Technology

- Beryl-owned code is Rust only.
- The desktop client uses `gpui` and does not depend on browser technologies, JavaScript toolchains, Node.js, WebView wrappers, or non-Rust native application libraries.
- Beryl may use the official `gpui` package or a Beryl-maintained fork anchored to upstream Zed `gpui` when targeted patches are required.
- GPUI-owned transitive native build dependencies remain allowed.
- A Beryl GPUI fork preserves GPUI's public boundary and may gate GPUI HTTP integration behind an opt-in feature when Beryl does not consume it.
- Beryl consumes standalone app-neutral `gpui-text-input`, `gpui-settings-window`, and `gpui-scrollbar` packages where practical and keeps product semantics in Beryl-owned adapters.

## Backend Boundary

- Agent execution, live event streams, authentication, sandboxing, approvals, tools, skills, MCP, subagents, managed configuration, and enterprise policy flow through out-of-process `codex app-server`.
- Beryl does not bundle, install, modify, or directly link Codex internal crates.
- Beryl may launch and supervise app-server processes and may implement narrow GUI-side orchestration from public protocol primitives.
- Cross-boundary communication uses the app-server contract rather than Codex storage, process memory, or internal implementation details.
- Syndic captures accepted live events and becomes durable history authority for captured threads. CAS remains the source of live execution events, not captured transcript reads.
- Beryl never repopulates captured Syndic history from CAS historical transcript methods.
- GPUI transcript code consumes Syndic only through the transcript provider and residency boundaries and never reaches durable storage or backend process state directly.

## Codex App Server Version Invariant

- This Beryl version targets exactly `codex-cli 0.144.1` / Codex App Server 0.144.1.
- Beryl carries no runtime branches for older schemas or speculative future schemas.
- Compatibility validation parses the app-server version from the initialize response user-agent product token, requires exact 0.144.1, and probes every required method and field through non-destructive typed requests.
- The target requires exact CAS-native continuation and fork primitives for the ordinary path, plus stable `thread/inject_items` for one-time recovery when exact CAS lineage cannot be reused.
- Recovered Syndic history is never a normal per-turn payload. Beryl injects it only into a fresh CAS thread whose native lineage is missing, stale, unavailable, or unprovable, and never repeats that injected prefix on later `turn/start` or `turn/steer` requests.
- The proven branch-selection channel injects one bounded provenance-framed assistant/output-text item carrying the exact accepted selected passage once before the first branch-local user turn. It does not use `additionalContext`, developer instructions, ordinary user input, or a CAS-private wrapper convention.
- Generated-schema, source, and focused live evidence for these boundaries is recorded under `doc/memory/topic/codex-app-server/`. Runtime admission combines that pinned-release semantic proof with the exact initialize version and typed non-destructive capability probes; it never creates a synthetic model turn solely to repeat compatibility proof.
- An incompatible configured app-server disables affected backend work instead of falling back to another schema.
- Upgrading the target replaces this single invariant and refreshes its memory evidence, feature/system contracts, normalized package boundary, and tests together.
- CAS thread lists, thread names, full-history `thread/read`, `thread/turns/list`, and item-history reads are not Beryl catalog, title, restore, or captured-history inputs.

## Responsibility Split

- Syndic owns stable threads, exactly one current durable draft per thread, submitted turns, immutable parentage, canonical captured events, transcript projections, resources, and exclusive CAS projection bindings.
- Beryl-home state owns executable-path runtimes, configured roots, per-thread execution bindings and presentation metadata, generated titles, automatic branch-discussion archive state, window/session records, settings, installed themes, asset references, durable host jobs, and compact catalog projections.
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
- Hard stop requires exact backend-exposed handles and never guesses OS pids, process names, working directories, or process trees.
- V1 denies app-server approval requests during Beryl-managed turns, prefers a denial that interrupts when supported, logs bounded redacted diagnostic evidence, and avoids leaving turns waiting indefinitely.
- Quiet live streams are not failures. Bounded request timeouts do not impose an inactivity timeout on active streams.

## Persistence

- The configured Beryl home contains all Beryl-owned durable state and exactly one physical Fjall database for Syndic and Beryl metadata domains.
- Logical ownership remains separated by typed keyspaces and APIs even though the physical database is shared.
- Beryl-home sidecars contain durable image and heavy Syndic resources; installed theme documents remain in the home theme repository.
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
- Complete catalog metadata may reside in memory as exact compact domain state, while GUI row construction remains fixed-height and virtualized.
- Every externally variable runtime cache, queue, projection, history, retry set, diagnostic buffer, media store, and dependency-facing handle has deterministic count and byte bounds unless it is exact durable domain state.
- Operations over large exact durable content use bounded pages and bounded staged commits; they do not require one unbounded record, command, or background-worker message.
- Background work is bounded, cancellable, and lower priority than foreground turn streaming and selected transcript activation.
- Implementation favors predictable latency and explicit rejection over unbounded retention or hidden fallback work.

## Platform Targeting

- Windows is the primary product-quality and developer-attention target.
- The architecture preserves operation on other GPUI-supported platforms when the backend and windowing boundaries permit it.

# TODO

This section is a non-authoritative issue-tracker substitute. Items here express future intent and provisional constraints only. They must be reviewed and promoted into authoritative feature and system docs before implementation.

## Semantic Search

- Reintroduce semantic search after the Beryl-home rework is complete.
- The future feature should search documents and conversation history without depending on semantic graph records or graph-derived ranking.
- No current semantic-search feature design or implementation remains live merely to preserve this intent.

## Theme Hierarchy And Editor

- After the Beryl-home rework closes, redesign the theme role hierarchy and the theme editor's presentation of inheritance.
- Preserve the ability to configure every intended GUI piece and to inherit style properties between roles.
- Do not treat the current hierarchy or editor navigation as automatically preserved target behavior.

## Collect Garbage

- Design an explicit user-invoked operation for proving and reclaiming unreachable Syndic turns, projections, sidecars, image assets, abandoned CAS provenance, and related Beryl metadata.
- Until that design exists, any future thread/reference removal must retain underlying durable records and bytes.

## Runtime, Root, And Thread Management

- Design explicit removal of configured Codex executable runtimes and roots after the Beryl-home rework.
- Design explicit rebinding of an existing thread to another runtime/root without weakening exact history, draft, and CAS-projection identity.
- Reconsider manual thread rename, pin, archive, and delete commands only as later product work. The current rework retains automatic generated titles and automatic branch-discussion archive after successful handoff.

## AIPM GUI Skill Local-Adaptation Cleanup

- Review and clean up Beryl's local adaptations to the canonical AIPM `gui` skill. Canonical AIPM
  and Beryl were synchronized in commit `48414e61ac89d67b479740cf9633ccabe51a14bd`;
  safe generic GUI improvements and intrinsic-size CSS fixes are already integrated.
- Treat the remaining local drift as an architectural ownership question before editing. Classify
  it into user-observable widget guarantees; GPUI realization, overscan, scrolling, anchoring,
  performance, and content-free diagnostic mechanics; Beryl-wide system or package policy; and
  material owned by a dedicated local skill such as `gpui-scroll-surfaces`.
- Keep stable reusable-widget behavior in the generic GUI contract when users can observe it,
  including focus, selection, navigation, scrolling, refresh, and intentional offscreen-anchor
  behavior. Preserve the canonical `gui` skill's purpose: widget contracts, shared behavior,
  composition, and integration into window slots.
- Move Beryl- or GPUI-specific implementation mechanics out of generic GUI guidance when existing
  feature, system, package, or local-skill authority owns them. Do not force an extraction when the
  repository's authority model supports a cleaner placement, and do not create dependencies
  between otherwise independent skills unless the dependency is necessary and justified.
- Preserve intentional Windows and Beryl specialization. Update `.agents/skills/SOURCE.md` if the
  resulting provenance or adaptation classification changes.
- When this work is promoted for implementation, follow repository instructions and the active
  `doc/plan.md`, use the `skill-creator` workflow for the existing-skill change, and update the
  appropriate design and planning authority before implementation.
- Before committing the eventual cleanup, summarize the proposed ownership boundaries and why
  retained or moved material belongs in each location. Stop at an architectural blocker rather
  than applying a local workaround if the documented authority cannot support a sound cleanup.
- Review the final result for contradictions, dangling references, duplicated authority, and loss
  of useful Beryl behavior. Run available validation without installing software; Python-based
  skill validation may be handwaved. Preserve unrelated dirty work, stage and commit only the
  cleanup, and do not push.

## Later Navigation Exploration

- Consider branch-sibling visualization and click-to-focus for threads already open in another main window without changing the immediate flat recent-first selector contract.
