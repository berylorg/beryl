# Goals

Define Beryl's semantic graph as the durable workspace scope map that helps users organize project capabilities, decisions, bugs, ideas, checklists, source documents, and important Codex threads without making the graph a transcript index or a replacement for authoritative project documents.

## Non-goals

- Replacing `doc/design.md`, feature design documents, plans, research notes, or other authoritative source documents with graph summaries.
- Mirroring every conversation turn, backend event, tool log, or transient activity record into semantic graph state.
- Owning AI-assisted graph upkeep, source- or conversation-driven graph maintenance, semantic search behavior, or a dedicated user-facing search UI; graph upkeep authority lives in `doc/features/graph-upkeep/design.md`, and semantic search authority lives in `doc/features/semantic-search/design.md`.

# Decisions

## Product Features

Beryl presents the semantic graph as a per-workspace scope map for durable work concepts. Users can use it to organize project capabilities, bugs, ideas, open questions, design areas, implementation tasks, checklists, source documents, and important Codex conversations.

The graph is semantic rather than conversational. Conversation threads may be attached to graph nodes, but ordinary transcript turns, backend events, tool logs, and activity records are not graph nodes.

Semantic nodes have stable identity, short titles, concise summaries, and one or more constrained semantic facets. The supported facets are `Topic`, `Checklist`, and `ChecklistItem`.

Topic-capable nodes represent reusable work concepts. Starting Codex work from a topic-capable node creates a new user-facing Codex conversation thread through the current primary workspace member, attaches that thread to the existing node, and activates the resulting transcript.

Checklist-capable nodes own ordered checklist-item nodes. Checklist items have visible status values such as `todo`, `in_progress`, and `done`, and are also topic-capable so a user can start Codex work directly from an existing checklist item.

The hard graph structure is an ordered forest. Users can browse root-level semantic nodes, follow hard parent-child structure, and preserve root and child ordering as durable workspace organization.

Soft links connect semantic nodes outside the hard forest. Soft links are typed directed relationships, such as `depends_on` or `informs`, and may connect nodes inside one hard-tree component or across different root-level components.

Thread refs attach backend-owned Codex conversation threads to semantic nodes. A node may reference multiple threads, and one thread may be relevant to multiple nodes. Deleting a semantic node removes its graph-owned refs but never deletes backend-owned Codex thread history.

Markdown refs attach source documents to semantic nodes. A markdown ref may target a document, section, or block using path, heading, optional explicit anchor, and content fingerprints. Source documents remain authoritative; graph summaries are navigational aids.

Markdown refs may become stale when referenced source documents change. Markdown-ref status values are `resolved`, `stale`, `unresolved`, and `ambiguous`. Stale, unresolved, or ambiguous refs remain graph-owned records until a direct graph action or graph-upkeep workflow updates or removes them.

The graph changes through direct GUI actions and bounded app-server dynamic tools registered by Beryl. Successful dynamic graph writes are durable before the tool result is returned to the model.

Graph upkeep may use graph records, graph refs, graph tools, and graph provenance to update the graph from conversation context and current source-document context read during upkeep. Upkeep policy, AI-driven repair, and graph upkeep instructions are defined in `doc/features/graph-upkeep/design.md`.

Semantic search may consume semantic graph records, thread refs, markdown refs, graph provenance, and graph proximity metadata. Retrieval behavior, search result contracts, search-owned dynamic tools, and search-owned caches are defined in `doc/features/semantic-search/design.md`.

Graph mutation failures are localized. A failed write reports a recoverable state without deleting unrelated visible graph state, closing the graph overlay, discarding checklist sidebar scroll, or switching the active transcript.

Backend-unavailable states disable backend-required graph actions for the affected runtime target, such as opening a thread ref or starting a new thread from a node. GUI-owned graph browsing, checklist viewing, and source refs remain usable when their data is already available locally.

## Architecture

GUI-local workspace state owns the semantic graph, checklist state, semantic node summaries, soft links, thread refs, markdown refs, graph provenance, and graph revision metadata.

Backend conversation history remains owned by `codex app-server`. The graph stores only refs and metadata needed to reopen or describe relevant conversations; it does not copy transcript history into graph state.

Workspace members and member-thread inventories are not semantic nodes. They are external workspace/runtime context used to validate thread refs, start graph-created threads, and present link-thread choices.

The pure semantic graph model stores semantic nodes, constrained facets, hard parent links, root order, soft links, thread refs, markdown refs, checklist-item status, provenance-bearing records, and atomic patch application types.

A non-empty hard graph is an ordered single-parent forest. Every non-root node has exactly one hard parent, every node is reachable from exactly one root-level node, and hard-parent updates reject self-parenting and cycles.

Checklist-item nodes may only be hard children of checklist-capable nodes. Checklist-capable nodes may only own checklist-item hard children.

Soft links are outside the hard forest. They may be cyclic and may cross root-level hard-tree components. Deleting a semantic node removes any soft links whose source or target is deleted.

Thread refs store backend thread identity and enough runtime/path metadata to determine whether the thread is currently openable inside the workspace. Invalid or unopenable status is derived presentation state rather than automatic ref deletion.

Markdown refs store source identity separately from rendered labels. A section ref records the path, heading path, generated heading slug when available, heading occurrence, optional explicit anchor id, source hash, section hash, and last-known line range as a display hint.

A markdown block ref records its parent section ref, block index, block hash, and leading-text fingerprint. Line numbers are never durable identity for markdown refs.

Markdown ref resolution first uses explicit anchor ids, then heading paths, then slug plus occurrence, then content fingerprints, then fuzzy matching. Failed or ambiguous resolution marks the ref as stale or needing repair without deleting the ref.

Graph mutations record provenance identifying the actor, timestamp, source conversation turn or tool action, and dynamic tool-call identity when a tool caused the mutation.

Model-supplied graph write arguments are never trusted as provenance. Beryl injects provenance from the app-server thread, turn, and tool-call context before persisting graph or checklist mutations.

Graph write validation is mechanical. It enforces tool schemas, graph invariants, workspace/path bounds, source-ref shape, provenance injection, and operation-specific constraints, but it does not judge the factual correctness of model-authored summaries, links, checklist updates, or structural graph changes.

Patch operations that restate already-current graph facts are no-ops for semantic records. Repository revision metadata may advance, but no-op writes do not change provenance, root order, child order, soft-link identity, thread refs, markdown refs, or checklist state solely to record a redundant write.

Per-workspace persistence stores the semantic graph aggregate and graph revision metadata as durable workspace content. Durable graph mutations atomically update the graph aggregate, graph revision metadata, and workspace last-updated metadata.

Repository graph write outputs are commit-or-failure results. Only accepted durable writes produce graph mutation commits.

The shell graph projection applies graph mutation commits in revision order. Stale, duplicate, or gapped commits are handled through no-op acknowledgement or explicit recovery rather than silently replacing visible graph state.

Direct GUI graph actions may apply an optimistic graph projection over the latest committed graph plus pending local patches. Optimistic projection is presentation state and never becomes authoritative durable graph state.

Pending optimistic mutations are keyed by mutation identity and affected semantic ids so rows, menus, checklist projections, thread refs, and markdown refs can show pending or disabled state without replacing the graph overlay body.

Successful graph commits reconcile visible graph overlay state, checklist sidebar state, context menus, selection, expansion, and scroll by semantic identity. Full graph reload is reserved for startup, workspace open, and explicit recovery.

Dynamic graph tools expose bounded targeted reads, such as root summaries, node-centered neighborhoods, checklist slices, source refs for one node, and stale markdown refs.

Dynamic graph tools expose operation-specific writes rather than a polymorphic internal patch DSL. Node upsert includes parent or root-level assignment in the same operation so hard-forest invariants remain atomic. Markdown-ref writes are likewise operation-specific, such as source-ref upsert, source-ref status update, and source-ref repair, and must flow through graph-owned validation and provenance injection.

Graph tools may expose bounded workspace member, primary-member, and runtime-environment metadata so the model can understand where graph-started threads and thread refs are valid without receiving full filesystem snapshots.

Dynamic graph writes flow through the same repository mutation path as direct GUI graph actions. A successful tool result means the durable write committed before the result was returned.

Graph tool execution and repository work must not block the `gpui` thread.

Workspace-level metadata such as titles, the default runtime environment, runtime-bound workspace-member registrations, and primary-member designation is persisted separately from semantic graph provenance.

## UI

The semantic graph UI consists of the graph overlay, graph node context menu, checklist sidebar, sidebar splitter, and graph-ref status treatments.

The graph overlay is a toggleable overlay surface shown above the conversation column. It is hidden by default, closes the thread selector when opened, and is the only column-browser surface interactive while visible.

The graph overlay anchors its left and right edges to the conversation column and its top edge to the bottom edge of the thread strip. Its default height is bounded near the upper half of the visible conversation-column space.

The graph overlay remains bounded within the visible conversation column in small-window layouts. It must not push the toolbar strip, thread strip, checklist sidebar, user input panel, status line strip, or transcript region off-screen.

When the OS window cannot provide the graph overlay's preferred height, the overlay clamps its height within the conversation column and leaves scrolling to the graph browser viewport and browser columns.

The graph overlay does not reflow the main workspace layout. It floats above the transcript region, leaves the toolbar strip, thread strip, user input panel, status line strip, and checklist sidebar in place, and prevents underlying transcript content from acting as the active interaction surface while open.

The graph overlay has a fixed header strip and a graph browser viewport below that header. The header strip may show compact graph scope or status information, but it does not show node summaries, graph-wide node counts, or long explanatory text inline.

The graph browser viewport is a columnar graph browser. It owns horizontal scrolling when the column trail exceeds the available overlay width.

Each browser column owns vertical scrolling for its own rows beneath a fixed column header. Browser columns do not share one vertical scroll position.

The root column lists the workspace's ordered root-level semantic nodes. It does not introduce a synthetic root object.

A successor column opens from the selected branching row in the preceding column. Selecting a semantic node opens a successor column rooted at that node. Selecting a soft-link row opens a successor column rooted at that link target.

The column trail is a navigation projection over the graph. It does not imply that the underlying graph is a tree, and it must support cross-links, multiple roots, soft-link traversal, and repeated encounters with the same semantic node without duplicating durable graph records.

The selection path records the selected domain items across the visible column trail. Graph mutation commits reconcile the selection path by semantic identity and prune only invalidated rows and dependent successor columns.

Each browser column header is a compact single-line strip for the column scope. It does not include summaries, node counters, or long breadcrumbs.

Semantic node rows are compact column rows. Each row shows the node title, or a compact checklist status marker followed by the title for checklist-item nodes.

Node type is conveyed through row treatment and checklist status markers rather than inline facet badges.

Node summaries are exposed through hover tooltips. Summary tooltips are suppressed while any graph node context menu is open.

Rows with visible hard children expose an expand-or-collapse control. Expanding a row can show hard children, attached soft-link rows, thread-ref rows, and markdown-ref rows according to the column's visible depth and expansion state.

Soft links render as compact terminal rows attached beneath expanded semantic nodes. A soft-link row label identifies the link kind and target title. Activating a soft-link row opens the target semantic node in a successor column instead of mutating graph state.

Thread refs render as compact terminal rows attached beneath expanded semantic nodes. A valid thread-ref row label shows the thread display title. Activating a valid thread-ref row closes selector surfaces as needed and uses the exact existing-thread activation path, including pending transcript activation state.

Invalid thread-ref rows remain visible. They render a compact invalid-link indicator, expose the invalid reason through a hover tooltip, and report the invalid reason through the standard localized notice path when activated.

Markdown refs render as compact terminal rows attached beneath expanded semantic nodes. A markdown-ref row label identifies the source file and target heading or block when known.

A resolved markdown-ref row activates the ordinary source-opening path for that document target. A stale or unresolved markdown-ref row remains visible, renders a compact stale-link indicator, exposes the resolution problem through a hover tooltip, and preserves stable target identity for graph-upkeep reconciliation workflows.

Rows affected by pending local graph mutations may show pending, disabled, or dimmed state. Unaffected rows remain visible and interactive according to current graph-action policy.

Ordinary graph mutations keep the graph overlay body and browser columns mounted when graph content is already available. Full-body loading or recovery surfaces are reserved for startup, empty graph, and explicit authoritative refresh recovery.

Right-clicking a semantic-node row opens a graph node context menu without changing the active transcript thread.

The graph node context menu is a bounded context menu surface layered above the graph overlay and clamped within the OS window bounds. The menu and its submenus own vertical scrolling when their rows exceed the bounded height.

The graph node context menu contains compact menu items. Disabled menu items remain visible and expose the disabled reason through a hover tooltip.

The `Delete` menu item deletes only the target semantic node and only when the target has no hard children. It remains visible but disabled when the selected node has hard children.

The `Delete Recursively` menu item is a hold-for-action trigger. Holding the row for three seconds deletes the target semantic node and its hard descendants only; soft links are not followed as additional deletion targets.

While `Delete Recursively` is held, the menu item shows continuous progress feedback with a left-to-right background fill. Releasing early, moving outside the row, closing the menu, pressing `Escape`, focus loss, disabled-state transition, or loss of the stable target node cancels the hold without deleting graph state.

After the hold completes, `Delete Recursively` triggers exactly once, shows an in-flight state, and suppresses duplicate graph mutation submissions until the request finishes or fails.

The `Link thread` menu item creates a thread ref from a selected existing conversation thread to the target semantic node without activating the transcript.

When the active workspace has exactly one available member, including the implicit home member case, `Link thread` opens directly into that member's thread-list submenu.

When the active workspace has more than one available member, `Link thread` opens a member-list submenu, and each member row opens that member's thread-list submenu. Unavailable explicit members do not appear in the submenu.

Thread-list submenu rows show only the thread display title and are sorted by last-updated time descending. A member with no linkable threads shows a disabled `No threads` menu item.

Graph mutation failures from context-menu commands report localized error or recovery state near the menu or graph surface, clear the in-flight state, and preserve unaffected graph columns and checklist sidebar state.

The checklist sidebar is an optional right-edge panel that shows the currently selected checklist-capable semantic node. It is hidden by default, can be hidden explicitly, and auto-shows when a checklist-capable node is selected.

The checklist sidebar anchors its top edge to the bottom edge of the thread strip, its right edge to the OS window, its bottom edge to the top edge of the status line strip, and its left edge to the sidebar splitter when visible.

The sidebar splitter is a draggable vertical separator between the conversation column and the checklist sidebar. It is visible only while the checklist sidebar is visible.

Dragging the sidebar splitter changes the transcript/sidebar width split while respecting the minimum sizes of the conversation column, transcript region, checklist sidebar, user input panel, and status line strip.

The checklist sidebar owns vertical scrolling for checklist rows and does not own horizontal scrolling. Checklist-item text wraps within the visible sidebar width.

The checklist sidebar presents a flat numbered list of checklist-item rows. Visible rows preserve item identity, order, numbering, status labels, and thread-start affordances.

Checklist sidebar rows are materialized from the current semantic graph for the visible row window. The sidebar does not create a second durable checklist model.

Checklist-affecting graph mutations update the sidebar in place when the selected checklist remains valid. If the selected checklist is deleted or loses checklist capability, Beryl clears or hides only invalidated checklist sidebar state.

Right-clicking a checklist-item row opens a context menu with `Start New Codex Thread`. That command creates and activates a new Codex thread attached to the existing checklist-item node rather than creating a new semantic child node.

Surface notices report graph recovery, invalid thread refs, invalid markdown refs, backend-unavailable graph actions, and graph mutation failures without replacing the graph overlay or checklist sidebar.
