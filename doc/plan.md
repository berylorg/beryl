# Scope

Update the authoritative status-line product and GUI contracts for compact per-thread token counters, then implement and verify the Rust projection, formatting, responsive segment sizing, rendering, and selected-thread behavior derived from those contracts.

# Phase 1: Authorize the status-line token readout and layout (finished)

Authorized the deterministic selected-thread `I`/`IC`/`O` readout, compact formatting, independent unavailable state, and responsive three-segment layout. Added and linked the valid feature GUI mount and main-window integration slot; independent phase review passed.

# Phase 2: Implement and verify the status-line token readout (pending)

Project selected-thread cumulative totals into the Context segment, derive non-cached input defensively, format compact values, resize the three segments, and cover formatting, missing data, persistence, thread switching, drafts, rendering order, and longest Turn state with focused Cargo nextest and formatting checks.
