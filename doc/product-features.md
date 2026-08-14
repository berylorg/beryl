# Product Features

This document is a navigational index for Beryl V1 feature contracts. It is not the authoritative location for detailed feature behavior.

Authoritative feature design entry points:

- Workspace startup, selection, runtime environments, workspace members, workspace picker, and workspace persistence: `doc/features/workspaces/design.md`
- Backend runtime availability, managed backend lifecycle, capability probing, backend-unavailable states, and connection recovery: `doc/features/backend-runtime-recovery/design.md`
- Conversation thread creation, activation, thread selector, inventory, branch/edit, thread binding, rebind behavior, automatic thread-title generation, and user-initiated thread-title updates: `doc/features/conversation-threads/design.md`
- Composer, draft submission, image input, queued input, active-turn steering, composer history, quote insertion, and developer-instructions injection: `doc/features/composer/design.md`
- Transcript rendering, Markdown, history pagination, transcript media, selection, quote harvesting, turn context menu, edit preview, and transcript scroll anchoring: `doc/features/transcript/design.md`
- Status line, model/reasoning controls, context and rate-limit display, compaction controls, and turn stop controls: `doc/features/status-line/design.md`
- Activity panel and activity projection: `doc/features/activity-panel/design.md`
- Semantic graph, graph overlay, checklist sidebar, primitive graph tools, graph refs, and provenance: `doc/features/semantic-graph/design.md`
- Graph upkeep, graph upkeep instructions, AI-assisted graph maintenance, on-demand source-ref repair, and graph-upkeep write policy: `doc/features/graph-upkeep/design.md`
- Semantic search, local knowledge corpus, search dynamic tools, lexical/vector indexing, embedding generation, and search-owned caches: `doc/features/semantic-search/design.md`
- Settings window shell, settings rows, settings persistence, and settings dynamic tools: `doc/features/settings/design.md`
- Appearance themes, theme repository, Themes settings page, theme candidate code panels, theme dynamic tools, and theme editor: `doc/features/theming/design.md`
- Surface notices, turn-error notices, end-turn sounds, and attention-trigger behavior: `doc/features/notifications/design.md`
- AI lifecycle yield tool and continuation behavior: `doc/features/lifecycle-yield/design.md`
- Supervisor diagnostics and diagnostic child control: `doc/features/diagnostics/design.md`

Shared non-feature contracts:

- Root cross-feature architecture and global constraints: `doc/design.md`
- Shared UI infrastructure, widgets, geometry, and scroll mechanics: `doc/ui.md`
- Shared baseline text-input interaction: `doc/input-hotkeys.md`
