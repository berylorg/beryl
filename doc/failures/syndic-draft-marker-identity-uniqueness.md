# Scope

Global stable marker-id uniqueness in persistent composite composer drafts.

# Invalidated Approach

Use only the composite sequence piece tree and its ordering/search summaries to reject a marker id
that already exists elsewhere in the draft.

# Evidence

Composite search envelopes order markers by anchor, same-anchor order key, and marker id. They can
authenticate an exact marker or gap on one sequence path, but absence on that path or at that anchor
does not prove that the same stable id is absent at another anchor. Proving global absence from
those summaries requires scanning the complete sequence tree.

# Why It Failed

Whole-tree uniqueness work violates the path-bounded autosave contract and grows with unchanged
draft size for a small insertion or move.

# Course Correction

Bind every draft revision to both the immutable composite sequence tree and a persistent immutable
copy-on-write index keyed by stable marker id. The index stores only stable occurrence facts, not an
absolute anchor. Edits authenticate absence or presence through bounded index descent; exact
location validation additionally consumes the caller's composite position or anchor witness and
checks it through one bounded sequence descent. Text inserted before unchanged markers therefore
does not rewrite their identity records. Both structures still publish atomically as one combined
draft root.

# Affected Work

The Syndic conversation-history and `syndic-storage` designs own the paired-root invariant, marker
lookup, staging, settlement, encoding, corruption, and bounded-work contracts.
