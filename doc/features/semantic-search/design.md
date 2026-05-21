# Goals

Define Beryl's local semantic search as a derived workspace retrieval layer that helps app-server conversations find relevant project knowledge across semantic graph records, source documents, and selected conversation memories without making indexes authoritative or requiring remote search.

## Non-goals

- Replacing `doc/design.md`, feature design documents, plans, research notes, source documents, semantic graph state, or backend-owned conversation history as the authoritative source of project facts.
- Owning the semantic graph model, graph overlay, graph mutation tools, graph refs, graph provenance, graph upkeep workflows, graph upkeep instructions, or source- and conversation-driven graph maintenance.
- Mirroring every conversation turn, reasoning event, backend event, tool log, transcript dump, or transient activity record into the search corpus.
- Owning a dedicated user-facing semantic search panel, search field, result list, or search popup.
- Requiring a remote search service or remote search request to answer local workspace knowledge queries.

# Decisions

## Product Features

Beryl may expose local workspace knowledge search through app-server dynamic tools. Search covers semantic graph records, graph refs, markdown source records, AI-generated source summary cards, and selected final-answer records from conversation threads.

Workspace knowledge search is a discovery aid for the AI conversation interface. Search results carry source kind and authority metadata so design documents can outrank graph summaries and thread memories when the model needs authoritative project facts.

Semantic search consumes semantic graph records, thread refs, markdown refs, graph provenance, and graph proximity metadata as source and ranking signals. The graph remains owned by `doc/features/semantic-graph/design.md`, and search must not mutate semantic graph state as part of indexing or query execution.

No dedicated user-facing semantic search surface is defined by this feature. CAS-facing knowledge search results appear through ordinary conversation output and dynamic-tool activity presentation until a separate UI-search design owns that surface.

## Architecture

Semantic search owns the local knowledge corpus, indexing pipeline, dynamic search tools, ranking behavior, result contracts, and rebuildable search caches.

The local knowledge corpus is derived from durable sources. Corpus records may be built from graph nodes, graph refs, markdown sections, markdown summary cards, and selected final-answer chunks from Codex threads.

Source documents remain authoritative over markdown summary cards. Semantic graph state remains authoritative over graph-derived records. Backend conversation history remains authoritative over selected final-answer records.

Final-answer indexing is selective. Reasoning, tool logs, backend activity streams, full transcript dumps, and transient turn events are not indexed by default.

The search index combines lexical records for exact paths, headings, ids, symbols, and error text with vector records for semantic recall. Ranking uses source authority metadata, graph proximity, recency, and exact-match evidence.

Embedding vectors and vector indexes are rebuildable derived cache. Losing, corrupting, deleting, or rebuilding the vector index must not lose semantic graph state, source documents, thread refs, markdown refs, or conversation history.

AI-generated summary cards are derived index records. Each summary card stores backing source identity and source hash, and search results that match a summary card must be able to retrieve the backing source section before treating the result as authoritative.

Search query execution is local. If a workspace has no usable local embedding backend or vector index, Beryl may fall back to lexical search and graph-neighborhood reads rather than making a remote search request.

Search tools expose bounded result sets rather than whole-workspace index dumps. Results identify the source kind, source authority, durable source identity, backing hash when available, and enough provenance to retrieve or cite the backing source before treating a result as authoritative.

Search indexing, embedding generation, vector index updates, source parsing, summary-card generation, and query execution must not block the `gpui` thread.

Search index freshness checks compare recorded hashes and fingerprints against current source records. Stale, missing, corrupt, or partially rebuilt search indexes degrade query quality or fall back to lexical and graph-neighborhood reads rather than corrupting durable workspace state.
