# Scope

Phase 13 live CAS item capture for one ordinary turn.

# Invalidated approach

The live worker retained every active and completed CAS item in process-local hash maps until the
terminal turn event. Each entry carried exact item identity, kind, digest, and text frontier proof.

# Evidence

The delta coalescer was bounded to 65,536 bytes, but the two item maps had no count or byte bound.
A provider turn containing arbitrarily many distinct items therefore grew worker memory without a
deterministic ceiling. Existing tests exercised only a small fixed item set.

# Why it failed

Per-item proof was duplicated in memory even though Syndic already durably owns exact CAS-item
indexes, canonical item revisions, content manifests, chunks, and completion lifecycle. Bounding the
maps with an arbitrary item limit would reject otherwise valid long turns and would not satisfy the
product's count-independent history contract.

# Course correction

Retain only the single bounded coalesced delta in the worker. Resolve each observed item event
through bounded, record-stabilized Syndic reads; compare completion prefixes and final text through
bounded content pages; and scan already admitted durable turn items in bounded pages to reject a
status-only terminal handoff that leaves an observed item live or undisposed. The terminal event
contains no item snapshot and cannot enumerate or reconstruct an event Beryl did not receive. Add
many-item and large-prefix tests that prove memory is independent of the number of prior items.

# Affected authority

`doc/plan.md` Phase 13, `doc/systems/cas-live-syndic-transcript/design.md`, and
`crates/beryl-app/doc/design.md` must state durable proof ownership and the exact retained worker
bound. Phase 13 remains in progress until the bounded capture tests pass.
