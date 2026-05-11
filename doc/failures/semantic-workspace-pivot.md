# Semantic workspace pivot

## Failed approach

- Beryl's earlier design treated the primary graph as a GUI-owned DAG of related Codex conversation threads.
- That approach made backend threads do double duty as both execution resources and the user's long-lived organizational structure.
- The result forced workspace discovery, navigation, and long-term organization to inherit the limitations of backend thread history instead of modeling the user's actual topics, plans, and checklists.
- It also pushed product complexity into branch, handoff, and detached-summary lineage that is useful for thread management but is not the same thing as a semantic map of user work.

## Course adjustment

- Beryl now treats the canonical graph as a GUI-owned semantic structure of first-class semantic nodes with constrained facets, including topic-capable checklist items.
- The hard semantic structure is a single-parent tree with optional typed soft links layered over it.
- Codex conversation threads are no longer the graph backbone. They are standalone backend resources attached to semantic nodes through many-to-many thread refs.
- Beryl workspaces are now GUI-owned durable semantic containers under the configured Beryl home directory, whose default is `~/.beryl/`, rather than filesystem roots discovered from backend thread history.
- Startup and navigation should therefore be driven by Beryl-owned workspace state rather than backend thread enumeration.
