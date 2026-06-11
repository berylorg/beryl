# Reason For Investigation

Beryl's semantic graph redesign needed background on durable agent memory that can survive across conversations while remaining queryable by the model.

# Outcome

Useful. The legacy finding supports treating Beryl graph memory as temporal and provenance-aware rather than as a flat bag of facts. For Beryl, this supports keeping node and edge provenance tied to source turns and treating thread links as references into a larger graph, not as the graph itself.

# Sources

- Preston Rasmussen, Pavlo Paliychuk, Travis Beauvais, Jack Ryan, and Daniel Chalef. "Zep: A Temporal Knowledge Graph Architecture for Agent Memory." arXiv:2501.13956, submitted January 20, 2025. URL: https://arxiv.org/abs/2501.13956. Metadata checked 2026-06-11.
- Legacy source: doc/research.md entry dated 2026-04-20.

