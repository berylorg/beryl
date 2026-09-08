# Scope

Global stable marker-id uniqueness in persistent composite composer drafts.

# Invalidated Approach

Use only sequence search to establish global identity absence, or only the current working identity
index to establish that a logical edit has not already used that stable identity.

# Evidence

Composite search envelopes order markers by anchor, same-anchor order key, and marker id. They can
authenticate an exact marker or gap on one sequence path, but absence on that path or at that anchor
does not prove that the same stable id is absent at another anchor. Proving global absence from
those summaries requires scanning the complete sequence tree.

Marker-continuation readiness review also found that Remove followed by plain Insert of the same
id passes working-index absence: removal has already erased its evidence. An intervening ordinary
text fragment can keep source ranges ordered, so range validation does not prevent this sequence.
The system's explicit one-semantic-effect-per-id rule still forbids it. The existing Insert effect
validation and working-index lookup did not check original-predecessor absence.

# Why It Failed

Whole-tree uniqueness work violates the path-bounded autosave contract and grows with unchanged
draft size for a small insertion or move.
Working-state uniqueness also differs from operation identity reuse: a deletion can hide the prior
use without creating a duplicate in the final tree.

# Course Correction

Bind every draft revision to both the immutable composite sequence tree and a persistent immutable
copy-on-write index keyed by stable marker id. The index stores only stable occurrence facts, not an
absolute anchor. Edits authenticate absence or presence through bounded index descent; exact
location validation additionally consumes the caller's composite position or anchor witness and
checks it through one bounded sequence descent. Text inserted before unchanged markers therefore
does not rewrite their identity records. Both structures still publish atomically as one combined
draft root.

Plain Insert must additionally prove absence in the operation's immutable predecessor identity
index. It performs this proof in its own bounded source-proof command before the existing working
absence check. Remove, Move and SameIdReplacement authenticate their original occurrence instead;
later removal effects cannot match an occurrence already removed or rewritten. This uses the
existing two root bases and adds no operation-wide identity registry. The V5 marker program owns
the closed proof transition; implementation and cross-fragment regression evidence remain pending.

# Affected Work

The Syndic conversation-history and `syndic-storage` designs own the paired-root invariant, marker
lookup, staging, settlement, encoding, corruption, and bounded-work contracts.
