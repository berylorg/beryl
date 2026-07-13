# Syndic Concepts

This supplemental system doc captures the current model for Syndic's conversation, branching, item, and reference concepts.

It is authoritative for current Syndic vocabulary and accepted model statements.

# Turn DAG

Syndic models conversation history as a directed acyclic graph of turns.

Most turns are one user input and the agent response produced from that input.

Syndic may also record provider-operation turns when the execution provider exposes a turn identity or turn-scoped operational item for work that is not ordinary user input, such as context compaction.

Provider-operation turns participate in the turn DAG as ownership roots, but transcript projections may hide or collapse them when they are not parent transcript narrative.

Every turn has either one parent turn or no parent. A turn with no parent is a root turn.

The parent chain of a turn defines the conversation context that should be replayed or summarized before running a child turn.

When a turn has more than one child, that is a branch point. Each child represents an alternate continuation from the same prior context.

The graph shape is closer to source-control history than to a flat chat log. The stable object is the turn graph; user-visible threads are views over that graph.

Provider events update turn-owned items, status, metadata, resources, and projections. They do not create, remove, or restore graph parent edges unless an explicit Syndic graph operation authorizes that topology change.

# Threads

A Syndic thread is a stable named reference over one selected path through the turn DAG.

The thread record owns a stable thread id, one committed conversation-tail turn id when submitted history exists, exactly one current draft id, and a revision covering that mutable binding state.

Walking immutable parent links backward from the committed tail to a root produces the thread's visible submitted conversation path. The current draft is not part of transcript narrative.

Different threads may point to the same committed tail and later diverge by submitting distinct child turns. A turn existing in the DAG never creates a thread by itself.

Generated title, automatic branch-discussion archive state, execution binding, and other application presentation facts are Beryl metadata keyed by Syndic thread id unless a specific Syndic record is named below.

A branch-discussion thread additionally owns the stable parent Syndic thread id used for eventual handoff and the id of the first draft or submitted turn that owns its context envelope. The draft or submitted turn's parent identifies the historical branch point; the parent thread id identifies the mutable handoff destination.

# Current Drafts

Every Syndic thread owns exactly one current draft.

A current draft is durable mutable pre-submission state with a stable id and revision. It owns composer-authored payload plus immutable parentage and an optional immutable typed context envelope.

Ordinary drafts have no branch context. A branch-discussion first draft has the selected source turn as its immutable parent and owns exact selected-text provenance without becoming canonical transcript narrative or starting an execution provider. Beryl may derive one presentation-only synthetic context group at that branch boundary.

Draft autosave may change only composer-authored payload and mutable draft timestamps. Parentage, thread ownership, and context/provenance fields never change after draft creation.

An idle-thread submission transitions the same draft identity into a submitted turn, advances the thread's committed tail, and creates its replacement current draft atomically.

Input accepted while another turn is active or context compaction is running is frozen from the current draft into a durable ordered input fragment or pending-input record owned by that execution lifecycle, then replaced by a new current draft. It does not create a competing active turn.

# Turn Items

Each turn contains an ordered sequence of lightweight items.

The initial item for an ordinary user turn is the user's input for that turn.

Provider-operation turns may instead start with an operation item, such as a context-compaction item.

Response items can include narrative commentary, final answers, tool activity, command activity, errors, token accounting, and references to generated or attached items.

Items should be metadata-rich enough to render the transcript and reconstruct agent context, but should not embed heavy binary data.

Large outputs are represented as references.

# Canonical Messages

Syndic should preserve the original provider message exactly as received.

This canonical message is the source of truth for rare recovery, replay, export, debugging, and projection rebuilds.

The canonical message does not have to live in the hot read path. It can be stored in a rarely used backing store as long as it is durable and recoverable.

Normal Beryl rendering should use Syndic's parsed and indexed projection instead of loading the whole canonical message.

# Markdown Projection

Assistant Markdown should be parsed into a structured block projection.

The projection can split large rendered content into smaller blocks that are easier for Beryl to page, virtualize, and render.

Short paragraphs, short lists, short code blocks, and small tables can remain inline in the turn item page.

Large paragraphs can be indexed into sentence or span chunks for viewport rendering. The projection should still preserve the fact that the text came from one original Markdown paragraph, because splitting it into separate Markdown paragraphs would change output semantics.

Large code blocks should become independently loadable code block resources.

Large Markdown tables should become independently loadable table resources.

Inline Markdown projection uses deterministic storage thresholds:

- A paragraph above 16,384 UTF-8 bytes is indexed into source-preserving span chunks whose payload is at most 8,192 UTF-8 bytes, split only at valid source boundaries.
- A code block is externalized when its body exceeds 4,096 UTF-8 bytes or 64 logical lines. Its inline preview is at most 8 lines and 2,048 UTF-8 bytes.
- A table is externalized when it exceeds 32 body rows, 12 columns, or 8,192 UTF-8 source bytes. Its inline preview contains the header and at most 4 body rows within 4,096 UTF-8 bytes.
- One turn-item page carries at most 65,536 UTF-8 bytes of inline Markdown source. Additional large structures are represented through ordered resources rather than enlarging the page.

These are storage and paging thresholds, not visible truncation limits. Exact source remains available through the owning projection or resource.

# Lazy Markdown Widgets

Large code blocks and large tables should have dedicated Beryl widgets with their own scroll viewport.

Those widgets should request only the visible range they need, and should be able to release data that is no longer visible.

A code block resource should expose metadata such as language, line count, byte count, digest, and optional preview lines.

It should support line-range reads.

A table resource should expose metadata such as row count, column count, header information, byte count, digest, and optional preview rows.

It should support row-range reads, and may eventually need column-range reads for very wide tables.

The parent answer projection should contain lightweight placeholders or refs for these resources rather than embedding the full code block or table body.

# Assistant Message Phases

Narrative commentary is visible assistant output emitted during the agent response.

From the OpenAI API point of view, this can arrive as an assistant message output item with phase metadata such as `commentary`.

Final answers can arrive as assistant message output with phase metadata such as `final_answer`.

Syndic should preserve phase metadata when the provider supplies it.

If phase metadata is missing, Syndic should store the message with an unknown phase rather than inventing a classification.

Codex App Server's behavior is a useful precedent here: it does not need to invent narrative commentary text. It projects assistant message items and preserves their phase when present.

# Syndic References

`syndic://` is the reference language for Syndic-addressable things.

Syndic references can point to turns, threads, messages, code blocks, tables, generated images, user-attached files, and other concrete objects that Syndic tracks.

Syndic references should include the resource kind explicitly.

A generic `item` kind should not be used when the referenced object has concrete rendering or loading semantics.

Canonical examples:

- `syndic:///turn/<id>`
- `syndic:///thread/<id>`
- `syndic:///message/<id>`
- `syndic:///codeblock/<id>`
- `syndic:///table/<id>`
- `syndic:///image/<id>`
- `syndic:///attachment/<id>`
- `syndic:///alias/image/<name>`

The canonical reference for Syndic-owned objects should be ID-based.

Canonical references use the strict `syndic:///kind/<id>` URI shape: scheme `syndic`, empty authority, absolute path, one registered lowercase-ASCII kind segment, and one canonical percent-encoded identifier segment. Parsers reject non-empty authorities, omitted third slashes, dot segments, query strings, fragments, unknown kinds, empty ids, non-canonical percent encoding, and extra path segments.

Object ids are globally unique within one Beryl home and never reassigned. A reference is resolved only against that exact home; it cannot address another home or arbitrary filesystem content.

Human-readable aliases use `syndic:///alias/<kind>/<name>`, are unique within `(Beryl home, kind)` under Unicode normalization and case folding, and never share the canonical object-id grammar.

Aliases are convenience handles that resolve through Syndic.

When a user submits input containing an alias reference, Syndic should resolve it to a concrete ID for that turn. The resolved reference should be captured on the turn so old turns do not silently change if an alias is later retargeted.

Reference resolution enforces the same owning-history and feature authorization as direct typed access. Merely knowing a reference string grants no additional read permission.

Canonical references and captured alias resolutions participate in future reachability analysis, but physical deletion is unavailable. An alias record itself is not proof that otherwise unreachable bytes are safe to retain or delete.

Syndic does not automatically expand a reference into model context. An owning feature or the CAS projection/recovery system must explicitly select bounded metadata, preview text, source ranges, or bytes under its own authority and budgets; otherwise the reference remains ordinary captured text plus typed metadata.

# Heavy Item Storage

Heavy item bytes should live outside turn items.

Turn items should contain stable references, media type, size, digest, storage location, preview metadata, and authorization or retention metadata where needed.

Transcript and history reads should be able to load item metadata without loading generated image bytes, attachment bytes, large log contents, or other heavy payloads.

The heavy-item backing store can choose its own technology independently of the turn graph, as long as references remain stable and recoverable.

# File Generation

Raw model text is not arbitrary binary file generation.

Images are a first-class OpenAI API output mode, and image result bytes or references should become Syndic-addressable generated image items.

Arbitrary files are better modeled as ordinary filesystem outputs created by tools. For example, an agent can write files under an execution root, create archives through a shell or code tool, or produce files in a sandbox/container.

Absolute filesystem paths remain the local stable references for those files for now.

A file path can change contents over time, can be deleted, and can be affected by branch checkout.

If a turn must preserve the exact file content observed at submit time, Syndic needs a snapshot or digest-backed attachment reference rather than only a filesystem path.

# Lazy History Access

The turn DAG should support cursor-based walking.

Useful walking directions include:

- Backward from a selected turn toward its root.
- Forward from a turn to its children.
- Forward and backward through whatever visible turn sequence a thread-scoped view later defines.
- Forward and backward through the items of a turn.
- Forward and backward through independently loadable Markdown block ranges.

Cursor reads should return lightweight metadata by default.

Generated image bytes, attachment bytes, large logs, and other heavy payloads should only load through explicit fetch operations.

# Replay And Context

The canonical graph must be sufficient to reconstruct the context for a new agent turn.

That does not mean every UI projection must load the full graph or every heavy item byte.

Syndic can maintain separate projections for fast UI reads, search, activity, and media browsing, as long as they can be rebuilt or invalidated from the canonical turn graph and reference metadata.

Markdown block projections are one such derived projection.

They should be rebuildable from canonical provider messages.

# Derived Lineage

Editing a prior user input never changes an existing submitted turn or its parent edge.

For replacement editing, Syndic creates a replacement turn from the edited turn's parent and atomically moves only the selected thread's committed tail and current-draft binding to the replacement path.

The original target and descendants remain immutable durable Syndic turns. They may remain reachable through another thread or become unreachable from every named thread.

Syndic does not physically delete unreachable turns, items, resources, or projections. The database may grow with unreachable records until a later garbage-collection design is approved.

Retry and regenerate are not Beryl product features. Users ask for another attempt by sending another normal input.
