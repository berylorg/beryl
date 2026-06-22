# Product Features

This document is a navigational index for Beryl V1 feature contracts. It is not the authoritative location for detailed feature behavior.

Authoritative feature design entry points:

- Workspace startup, selection, runtime environments, workspace members, workspace picker, and workspace persistence: `doc/features/workspaces/design.md`
- Backend-unavailable states and user-visible connection recovery: `doc/features/backend-runtime-recovery/design.md`
- Conversation thread creation, activation, thread selector, thread history navigation, inventory, branch/edit, thread binding, rebind behavior, automatic thread-title generation, and user-initiated thread-title updates: `doc/features/conversation-threads/design.md`
- Threaded decision workflows that bind checklist items to decision child branches, parent handoff turns, resolution outcomes, and child cleanup: `doc/features/threaded-decisions/design.md`
- Composer, draft submission, image input, queued input, active-turn steering, composer history, quote insertion, and developer-instructions injection: `doc/features/composer/design.md`
- Transcript rendering, Markdown behavior, transcript media, selection, quote harvesting, turn context menu, edit preview, and transcript scroll anchoring: `doc/features/transcript/design.md`
- Status line, model/reasoning controls, context and rate-limit display, compaction controls, and turn stop controls: `doc/features/status-line/design.md`
- Auto-visible activity panel and activity projection: `doc/features/activity-panel/design.md`
- Semantic graph, toolbar graph toggle, graph overlay, primitive graph tools, graph refs, and provenance: `doc/features/semantic-graph/design.md`
- Graph upkeep, graph upkeep instructions, AI-assisted graph maintenance, on-demand source-ref repair, and graph-upkeep write policy: `doc/features/graph-upkeep/design.md`
- Semantic search, local knowledge corpus, search dynamic tools, lexical/vector indexing, embedding generation, and search-owned caches: `doc/features/semantic-search/design.md`
- Settings window shell, settings rows, settings persistence, and settings dynamic tools: `doc/features/settings/design.md`
- Appearance themes, theme repository, Themes settings page, theme candidate code panels, theme dynamic tools, and theme editor: `doc/features/theming/design.md`
- Workspace notices, turn-error notices, end-turn sounds, and attention-trigger behavior: `doc/features/notifications/design.md`
- AI lifecycle yield tool and continuation behavior: `doc/features/lifecycle-yield/design.md`
- Supervisor diagnostics and diagnostic child control: `doc/features/diagnostics/design.md`

Shared non-feature contracts:

- Project-level goals, non-goals, and global constraints: `doc/design.md`
- Syndic durable conversation history, projections, references, resources, and replay: `doc/systems/syndic-conversation-history/design.md`
- CAS-live Syndic capture, CAS projection bindings, graph-action reflection, and selected-history read authority: `doc/systems/cas-live-syndic-transcript/design.md`
- Backend runtime launch, listener security, managed backend lifecycle, capability probing, connection recovery, and protocol ownership: `doc/systems/backend-runtime/design.md`
- Transcript presentation internals, residency, shell host boundary, renderer demand, resource admission, and diagnostics: `doc/systems/transcript-presentation/design.md`
- Codex-compatible replacement-agent constraints: `doc/systems/codex-compatible-agent-layer/design.md`
- GUI window and slot integration: `doc/gui/integration.md`
- External GUI widget spec registry: `doc/gui/external-specs.md`
- Beryl-local reusable GUI widget specs and contracts: `doc/gui/widgets/...`
- Shared baseline text-input interaction: `doc/input-hotkeys.md`
