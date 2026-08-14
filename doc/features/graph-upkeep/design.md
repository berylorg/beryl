# Goals

Define Beryl's graph upkeep as policy-controlled maintenance that keeps the semantic graph aligned with ongoing AI conversations and changing project source documents without making the graph a transcript mirror, a source-document replacement, or a search index.

## Non-goals

- Owning the semantic graph model, graph invariants, graph persistence format, graph overlay, checklist sidebar, primitive graph read/write tools, graph refs, or graph provenance fields.
- Owning semantic search, local knowledge indexing, embedding generation, ranking, search result contracts, or search-owned caches.
- Replacing `doc/design.md`, feature design documents, plans, research notes, source documents, or backend-owned conversation history as the authoritative source of project facts.
- Mirroring every conversation turn, backend event, tool log, source-document change, or transient activity record into semantic graph state.
- Using `codex app-server` `fs/watch`, filesystem event streams, scanner loops, or hook-driven pipelines as the graph-upkeep synchronization mechanism.
- Maintaining a live synchronized source-change index for graph upkeep.
- Providing a proposal or human-review queue for AI graph changes in V1.
- Guaranteeing that AI-maintained graph updates are always factually correct without provenance, bounded current-source reads, conservative policy, later AI upkeep, or direct user edits.

# Decisions

## Product Features

Beryl uses graph upkeep to keep the semantic graph useful as project work evolves. Upkeep may consume bounded signals from ongoing and completed AI conversations, explicit graph tool activity, current source-document context read during an AI upkeep pass, stale markdown refs, and workspace-specific graph policy.

The V1 graph-upkeep implementation slice provides workspace graph-upkeep instructions and hidden model context only. Markdown-ref graph tools, on-demand source-ref repair, and automatic post-turn upkeep require later operator design approval before implementation.

Each workspace may define graph upkeep instructions. Those instructions describe the workspace-specific curation policy, such as which root nodes matter, which concepts deserve nodes, which source documents are authoritative, and which graph changes should be conservative.

AI-assisted graph upkeep uses graph tools to read bounded graph neighborhoods, create or update nodes, update checklist statuses, attach thread refs, attach markdown refs, create soft links, and reconcile stale source refs. It must not rely on whole-workspace graph dumps as the normal context source.

Graph upkeep may update graph-owned summaries, refs, statuses, structure, and links through explicit graph tool operations. When the AI decides to update the graph and the graph-owned write path accepts the operation as mechanically valid, the graph is updated.

Graph upkeep may use ongoing conversation context to attach important threads, update summaries, create missing work-topic nodes, add checklist items, or link source documents when those changes match the workspace policy and graph invariants.

Graph upkeep may read current project source documents during an AI upkeep pass to repair refs, update summaries, add new refs, or make structural graph updates. Source documents remain authoritative, and graph summaries remain navigational. Out-of-turn source edits are handled as current source reality when AI upkeep next reads the affected documents or refs, not by a filesystem-event synchronization pipeline.

## Architecture

Graph upkeep is a consumer of the semantic graph feature. It may request graph reads and writes through graph-owned primitive operations, but semantic graph owns validation, invariants, provenance fields, mutation commits, persistence, and visible graph projection reconciliation.

Model-supplied upkeep write arguments are never trusted as provenance or authority. Beryl injects graph mutation provenance through the graph-owned write path and enforces schema validation, bounded reads, allowed operations, source/path bounds, and hard graph invariants before any upkeep request can commit. Beryl does not adjudicate the factual or semantic quality of an accepted model-authored summary, link, checklist update, or structural graph change.

Per-workspace graph upkeep instructions are stored as workspace-scoped GUI state in workspace persistence, not in app-wide `preferences.toml`, backend-owned Codex configuration, transcript content, semantic graph records, thread refs, or source documents. Blank or whitespace-only instructions are disabled; nonblank instructions are stored with normalized line endings and otherwise preserved as user-authored policy text.

Workspace graph upkeep instructions are model guidance. They do not override semantic graph invariants, dynamic tool schemas, provenance injection, source-document authority, or the user's direct graph edits.

Beryl injects the latest applied graph-upkeep instructions as hidden model context for top-level user-message turns in user-facing backend conversation threads in that workspace. This includes first turns of new persistent threads created through the workspace composer, graph topic start, or checklist-item start, later turns in existing user-facing threads, automatic continuation turns after lifecycle yield requests, and retry or replacement turn starts. Activated existing threads receive graph-upkeep context only when Beryl later starts a top-level user-message turn in that thread.

Beryl does not inject graph-upkeep context into active-turn steering, subagent requests, title-generation maintenance, inventory refresh, lazy metadata reads, context-compaction requests themselves, diagnostics, or other background/status-only backend work.

Beryl builds graph-upkeep hidden context in its own request-assembly path. Graph-upkeep hidden context is late-bound at request assembly, composed before the app-wide global developer-instructions preference, and uses the same app-server developer-instructions/collaboration-mode transport when that transport is available. Codex lifecycle hooks are not the transport for Beryl-owned workspace graph-upkeep instructions because hooks are user/project/managed script configuration with separate trust and review behavior.

If both graph-upkeep instructions and global developer instructions are disabled, Beryl sends the app-server hidden reset value required by the shared developer-instructions transport. If the transport requires an exact effective model and Beryl cannot determine it from backend metadata or GUI-held pending defaults, Beryl omits the hidden developer-instructions payload rather than guessing a model.

Graph upkeep does not use `fs/watch`, filesystem events, or scanner loops as a source synchronization mechanism. Current source state is pulled on demand through bounded source reads or source-ref resolution when the AI upkeep pass needs it. Failed, stale, ambiguous, or invalid source refs are surfaced through graph-owned tool results or localized UI state so the AI can retry, repair, update the graph, or leave the graph unchanged.

Automatic post-turn graph upkeep must stay off the user-visible response path. User- or model-initiated graph tool calls during an active conversation follow the ordinary dynamic-tool execution path and must still use bounded graph reads and graph-owned write validation.

Graph upkeep work, bounded source-document reads, markdown parsing for requested source targets, source-ref reconciliation, and repository work must not block the `gpui` thread.

Upkeep failures are localized. A failed mechanical graph write, stale-ref repair, or source read reports bounded failure state without deleting unrelated graph state, closing the graph overlay, discarding checklist sidebar scroll, or switching the active transcript.

## UI

The settings window includes a `Graph` tab for workspace-scoped graph settings. The graph upkeep instructions setting is a settings row with a text area as its setting value control.

The graph upkeep instructions setting label identifies the setting as workspace-scoped. Its setting help text explains that the instructions guide AI graph upkeep for this workspace without overriding Beryl's graph invariants or tool schemas.

Applying graph upkeep instructions updates the workspace-scoped active graph policy for later Beryl-created threads in that workspace. Closing the settings window without applying discards staged edits.

When no workspace is selected, the `Graph` tab remains visible but the graph upkeep instructions row is disabled with a workspace-required state and cannot apply a draft. During a workspace switch, unapplied graph-upkeep drafts for the previous workspace are discarded and the row is rebound to the newly selected workspace's applied policy. If workspace persistence is unavailable or an apply attempt cannot prove the target workspace still matches the selected workspace, applying graph-upkeep instructions fails without mutating the active policy.

Stale-ref repair failures, source-read failures, and mechanically rejected upkeep writes are reported through localized notices or bounded surface state without replacing the graph overlay or checklist sidebar.
