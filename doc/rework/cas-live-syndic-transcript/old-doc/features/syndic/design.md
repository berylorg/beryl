# Goals

Define Syndic as Beryl's future durable conversation, branching, item, projection, and reference model for agent work.

Preserve replayable agent history while enabling Beryl to render and navigate large histories through bounded, lazy projections.

Give future Syndic work a feature-level documentation home without putting package-owned storage-engine details in the feature contract.

## Non-goals

- Define the production storage engine, database schema, file layout, compaction policy, or low-level persistence API. Those decisions belong in `crates/syndic-storage/doc/design.md`.
- Replace Beryl's current Codex App Server integration before the root backend-boundary design is explicitly updated.
- Treat exploratory benchmark results as feature behavior authority.

# Decisions

## Documentation Set

- `concepts.md` is the supplemental Syndic domain model. It is authoritative for current vocabulary and accepted model statements about turns, threads, turn items, canonical messages, Markdown projections, Syndic references, heavy item references, lazy history access, and replay. Sections that explicitly say TBD, unresolved, or open question are non-final issue records.
- `codex-like-agent-layer.md` is the supplemental constraint checklist for any future Codex-derived or Codex-compatible local agent layer. It is authoritative for compatibility, auth, policy, safety, event, and history-preservation constraints that such a layer must satisfy before Beryl can rely on it, but it is not an implementation plan and does not authorize replacing Codex App Server by itself.
- Storage-engine choice and persistence implementation rationale belong to the `syndic-storage` package design doc, not this feature doc.

## Product Boundary

- Syndic owns the target conversation-history model for a durable turn DAG, thread-starting turns, ordered turn items, canonical provider messages, generated or attached item references, and lazy projections derived from canonical history.
- User-visible threads are views over the durable turn graph rather than the authoritative history object.
- Heavy generated or attached bytes must be addressable through stable Syndic references and must not be forced into ordinary transcript or history page reads.
- Syndic references use an explicit resource kind so loading, rendering, retention, and permissions can be reasoned about from the reference target.

## Beryl Integration Boundary

- Until a later root design decision changes the backend boundary, Beryl continues to treat Codex App Server as the owner of agent execution and backend conversation history.
- Syndic feature work must not silently copy backend-owned history into a new durable authority or bypass Codex auth, policy, sandbox, approval, rate-limit, or retention semantics.
- Any Syndic-backed runtime path must preserve canonical replayable history and expose efficient Beryl-facing projections as rebuildable or invalidatable derived state.

## Projection And Access

- UI reads should favor bounded cursor pages of lightweight metadata and inline payloads.
- Large Markdown blocks, code blocks, tables, generated images, attachments, logs, and other heavy resources should be independently loadable by explicit range or byte fetches.
- Derived projections for rendering, search, activity, and media browsing must remain recoverable from canonical history plus durable reference metadata.
