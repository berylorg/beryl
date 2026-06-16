# Syndic Concepts

This supplemental feature doc captures the current model for Syndic's conversation, branching, item, and reference concepts.

It is authoritative for current Syndic vocabulary and accepted model statements. Sections that explicitly say TBD, unresolved, or open question record non-final issues rather than locked behavior.

# Turn DAG

Syndic models conversation history as a directed acyclic graph of turns.

A turn is one user input and the agent response produced from that input.

Every turn has either one parent turn or no parent. A turn with no parent is a root turn.

The parent chain of a turn defines the conversation context that should be replayed or summarized before running a child turn.

When a turn has more than one child, that is a branch point. Each child represents an alternate continuation from the same prior context.

The graph shape is closer to source-control history than to a flat chat log. The stable object is the turn graph; user-visible threads are views over that graph.

# Threads

Any turn can be marked as the start of a thread.

Thread-starting turns are what the user sees as threads in the UI.

A thread can have user-facing metadata such as title, archive state, pinned state, timestamps, and workspace binding.

The relationship between a thread-starting turn and the visible turn sequence inside that thread is intentionally unresolved.

Thread-scoped DAG view-flattening is TBD. A thread that starts at `B` might eventually show one selected path below `B`, several branch paths below `B`, or a graph-aware view. That policy should be decided separately from the base turn model.

# Turn Items

Each turn contains an ordered sequence of lightweight items.

The initial item is the user's input for that turn.

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

Thresholds are TBD, but the intent is that a code block longer than a few short lines or a table larger than a few rows and columns should not be forced into every turn-item page load.

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

Examples:

- `syndic://turn/<guid>`
- `syndic://thread/<guid>`
- `syndic://message/<guid>`
- `syndic://codeblock/<guid>`
- `syndic://table/<guid>`
- `syndic://image/<guid>`
- `syndic://image/<human_alias>`
- `syndic://attachment/<guid>`

The canonical reference for Syndic-owned objects should be ID-based.

The examples use the readable `syndic://kind/<id>` shape.

The exact URI grammar is still TBD. Before implementation, Syndic should decide whether to keep that custom parsing convention or use a stricter URI path form such as `syndic:///kind/<id>`.

Human-readable aliases can be layered on top of stable IDs where they are useful.

Aliases are convenience handles that resolve through Syndic.

When a user submits input containing an alias reference, Syndic should resolve it to a concrete ID for that turn. The resolved reference should be captured on the turn so old turns do not silently change if an alias is later retargeted.

# Heavy Item Storage

Heavy item bytes should live outside turn items.

Turn items should contain stable references, media type, size, digest, storage location, preview metadata, and authorization or retention metadata where needed.

Transcript and history reads should be able to load item metadata without loading generated image bytes, attachment bytes, large log contents, or other heavy payloads.

The heavy-item backing store can choose its own technology independently of the turn graph, as long as references remain stable and recoverable.

# File Generation

Raw model text is not arbitrary binary file generation.

Images are a first-class OpenAI API output mode, and image result bytes or references should become Syndic-addressable generated image items.

Arbitrary files are better modeled as ordinary filesystem outputs created by tools. For example, an agent can write files in a workspace, create archives through a shell or code tool, or produce files in a sandbox/container.

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

# Open Questions

Thread-scoped DAG view-flattening needs refinement.

The `syndic://` namespace needs rules for uniqueness, alias conflicts, permissions, and retention.

The exact `syndic://kind/<id>` versus `syndic:///kind/<id>` URI grammar needs to be decided before implementation.

Markdown projection thresholds need to be defined for paragraph chunking, code block externalization, and table externalization.

The model for edits, rollbacks, compaction, and summarization needs to specify how derived turns relate to earlier turns.

The context builder needs a precise policy for when reference metadata, file text, previews, snapshots, or full bytes are included in a model request.
