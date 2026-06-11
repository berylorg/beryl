# Reason For Investigation

The operator raised the practical concern that graph-aware turns may become slow if every turn has to query a growing graph.

# Outcome

Useful. The legacy finding supports hierarchical, structured retrieval instead of dumping the whole memory into context. For Beryl, this argues for tools that return small relevant subgraphs or neighborhoods, not whole-graph reads, and for keeping the hard hierarchy meaningful enough to support efficient targeted retrieval.

# Sources

- Yifan Simon Liu, Ruifan Wu, Liam Gallagher, Jiazhou Liang, Armin Toroghi, and Scott Sanner. "Semantic XPath: Structured Agentic Memory Access for Conversational AI." arXiv:2603.01160, submitted March 1, 2026. URL: https://arxiv.org/abs/2603.01160. Metadata checked 2026-06-11.
- Legacy source: doc/research.md entry dated 2026-04-20.

