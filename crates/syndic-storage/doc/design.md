# Goals

Provide the reusable production storage boundary for Syndic-owned durable conversation state, reference metadata, and rebuildable projection state.

Support low-latency Beryl reads over large conversation histories while keeping memory growth bounded as stored history grows.

Support short durable write commits for streaming agent output, generated artifacts, and concurrent user or subagent activity without buffering large resources in memory.

## Non-goals

- Define Syndic product behavior, user-visible thread semantics, or Codex-like runtime compatibility. Those contracts belong in `doc/features/syndic/design.md` and its supplemental feature docs.
- Store Beryl's existing workspace persistence state or replace `beryl-app`'s current `redb` workspace database.
- Preserve benchmark harness code or captured benchmark result logs inside the Beryl project. Those artifacts live in the sibling `../syndic-benchmarks` workspace.

# Decisions

## Storage Engine

- `syndic-storage` uses `fjall` as its primary embedded storage engine.
- `fjall` is chosen over `redb` for Syndic storage because local Syndic-shaped scale benchmarks showed significantly lower observed RSS as database size grew and better durable write speeds for the expected write-heavy workloads.
- `redb` remains the existing storage engine for Beryl workspace persistence where it is already deployed. This decision does not change `beryl-app`'s current workspace database.
- `syndic-storage` should not keep a production `redb` adapter in parallel unless a later feature or package design decision requires an explicit fallback.

## Storage Shape

- The durable hot path is optimized for metadata-heavy cursor reads over turns, turn children, turn items, Markdown projections, code-line ranges, table-row ranges, reference metadata, and generated-artifact metadata.
- Large generated images, attachments, logs, tables, code blocks, and other heavy resources are addressed through durable metadata and range-capable resource records rather than loaded as part of ordinary transcript or history page reads.
- Large byte payloads should favor sidecar storage when that keeps the database hot path metadata-heavy and avoids large-value cursor tail latency.

## Ownership And Concurrency

- One Syndic process owns a storage directory for writes at a time.
- Parallel human turns, subagent turns, background projection work, and generated-artifact streams submit write intents through a storage-owned write coordination boundary.
- Readers must be able to perform bounded cursor and point reads while writes are committed through short durable transactions.

## Rebuildable Projections

- Canonical replayable history and durable reference metadata are authoritative.
- UI, search, activity, media, and rendering projections are derived state that must be rebuildable or invalidatable from canonical history plus reference metadata.
- Projection records must preserve stable identity, ordering, and provenance back to the canonical Syndic objects they summarize.
