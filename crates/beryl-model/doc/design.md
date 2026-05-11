# Goals
Provide shared pure-data types used across the Beryl workspace.

## Non-goals
- Owning process launch, transport I/O, or protocol parsing.
- Owning `gpui` rendering types or window lifecycle logic.
- Owning persistence engine implementation details.
- Owning durable graph revision sequencing, optimistic UI projection, or repository commit publication policy.

# Decisions

## Purity
- This crate defines cross-crate semantic-workspace, execution-target, conversation, provenance, and semantic-graph data types that can be reused without pulling in UI or backend runtime code.
- This crate must not depend on `gpui`, Tokio, or process-management APIs.

## Runtime Environments, Workspace Members, and Execution Targets

- Workspace id values for named workspaces are filesystem-friendly slugs derived from display titles by pure transliteration and normalization rules owned by this crate.
- Slug derivation is deterministic and rejects titles that produce an empty slug. Slug uniqueness across persisted workspaces is enforced by the persistence boundary rather than by this pure model crate.
- Updating a workspace title to an accepted generated or manual title updates the workspace manifest id and title together.
- Runtime-environment identity is represented by `RuntimeMode`, preserving the distinction between host-Windows and WSL-Linux runtimes even when textual paths overlap.
- Explicit workspace members are represented separately from concrete execution targets so workspace-level member selection can persist independently from thread-level backend targets.
- Concrete execution-target identity is represented by runtime mode plus canonical path, with WSL distro name included only for WSL-Linux mode.
- Workspace-conversation state owns the selected runtime environment, explicit workspace members, primary-member designation, active-thread selection, backend thread-name snapshots, manual GUI-local thread title metadata, whether a registered backend thread was created by Beryl, automatic thread-title generation attempt state for Beryl-created threads, thread/member binding metadata, last-known exact per-thread token-usage snapshots for status presentation, and registered thread summaries for one semantic workspace.

## Semantic Graph

- This crate owns the pure semantic-graph model for Beryl workspaces, including semantic nodes, constrained V1 semantic facets, hard parent/child structure, soft typed links, thread refs, provenance-bearing graph records, and batched patch application types.
- This crate also owns the pure read-side query helpers used to build bounded, node-centered graph neighborhoods, checklist reads, and other targeted graph projections without exposing persistence details.
- V1 semantic facet combinations are constrained rather than free-form. `ChecklistItem` requires `Topic` and does not coexist with `Checklist`.
- A non-empty hard semantic graph is an ordered single-parent forest. Every non-root node has exactly one hard parent, every node is reachable from exactly one ordered root-level semantic node, and hard-parent updates must reject self-parenting and cycles.
- Root-level semantic-node order is part of the pure semantic graph model. Moving a node to root level uses the same optional index semantics as moving a node under a hard parent.
- Soft links may connect semantic nodes inside the same hard-tree component or across different root-level components.
- Checklist-item nodes are first-class semantic nodes and may only be hard children of checklist-capable nodes.
- Checklist-capable nodes may only own checklist-item hard children.
- The semantic graph node set contains only semantic nodes. Workspace members, member-thread inventories, and backend conversation threads are represented outside the semantic node set.
- Thread refs remain associations to backend-owned conversation threads. This crate stores only the metadata needed to identify the thread and its execution target from GUI-owned graph state.
- A node may not attach the same conversation thread more than once.
- Leaf semantic-node deletion is a graph patch operation that deletes only the target node and is valid only when that node has no hard children at patch-application time.
- Recursive semantic-node deletion is a graph patch operation over the hard semantic forest. It deletes the target node and its hard descendants only and does not traverse soft links to expand the deletion set.
- Semantic-node deletion removes soft links whose source or target is deleted and removes thread refs attached to deleted nodes without deleting backend-owned conversation threads.
- Graph patch application is atomic at the in-memory model boundary: either the whole patch is accepted and the graph remains invariant-valid, or no change is applied.
- Patch operations that restate identical node, parent, root order, soft-link, thread-ref, or checklist-item status facts are no-ops at this model boundary. They must not touch provenance or reorder root-level nodes or hard children solely because a new mutation provenance value was supplied.

## Provenance

- Mutation provenance records distinguish workspace actions, conversation turns, generic tool actions, and app-server dynamic tool calls.
- Dynamic tool-call provenance stores the app-server thread id, turn id, tool name, and tool-call id so GUI-owned graph mutations can be traced to the exact reverse tool request that caused them.
