# Research Notes

## 2026-04-20

### Zep: A Temporal Knowledge Graph Architecture for Agent Memory (`arXiv:2501.13956`)

- Why I researched it: The proposed redesign replaces the current thread-lineage graph with a semantic graph that must survive across conversations while staying queryable by the model.
- Outcome: Useful. The paper reinforced that graph memory becomes more valuable when it is temporal and provenance-aware rather than just a flat bag of facts. For Beryl this supports keeping node and edge provenance tied to source turns and treating thread links as references into a larger graph, not as the graph itself.

### Large Language Models and Knowledge Graphs: Opportunities and Challenges (`arXiv:2308.06374`)

- Why I researched it: The redesign depends on LLM-driven extraction and maintenance of explicit graph state from conversational text.
- Outcome: Useful. The survey supports the hybrid approach of keeping explicit graph state outside the model while letting the model read and update it. It also reinforced a key risk for Beryl: graph construction and refinement driven by LLM output need validation and bounded tool contracts because extraction and relation typing are error-prone.

### Semantic XPath: Structured Agentic Memory Access for Conversational AI (`arXiv:2603.01160`)

- Why I researched it: The operator raised a practical concern that graph-aware turns may become slow if every turn has to query a growing graph.
- Outcome: Useful. The paper supports hierarchical, structured retrieval over dumping the whole memory into context. For Beryl this argues for MCP tools that return small relevant subgraphs or neighborhoods, not whole-graph reads, and for keeping the hard hierarchy meaningful enough to support efficient targeted retrieval.
