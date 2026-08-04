# Scope

Checkpoint 3 Phase 13 provider-narrative projection generations, reusable Markdown projection
prefixes, derived identities, resources, and completed-item capture in `syndic-storage`.

# Invalidated Approach

Phase 13 briefly proposed scoping reusable projection membership and textual-resource identity to a
new source epoch because `item/completed` was assumed able to revise arbitrary text already emitted
through `item/started` and live deltas.

That proposal treated completion as a replacement narrative snapshot. It would have rebuilt bounded
Markdown projection from logical zero and created a second provider-narrative generation whenever a
transcript-visible item completed.

# Evidence

Pinned CAS 0.144.1 feeds initial assistant text and later deltas through its streaming assistant-text
parser, flushes that parser before completing the item, and applies the corresponding citation and
proposed-plan normalization to the completed public item. When turn-item contributors prevent early
streaming, CAS emits the already finalized start and completion forms together.

The supported healthy path therefore intends the normalized `AgentMessage` or `Plan` narrative in
`item/completed` to equal the complete narrative previously exposed by start and deltas. Completion
still owns completion-only and non-narrative final fields, but it does not legitimately edit already
streamed public prose.

# Why It Failed

The proposal conflated final public-item authority with permission to revise the item's public
narrative. Its motivating example, where live text said one thing and completion silently changed
it to another, is not a supported state transition.

Building source-epoch machinery around that premise would add storage identities, rebuild work, and
resource churn for a representation that should never occur. It would also conceal a protocol or
capture failure by choosing one conflicting narrative.

# Course Correction

- Start and delta text extend one item-owned append-only provider-narrative generation.
- Completion performs a bounded byte-for-byte equality check against that complete generation and
  seals the same source when it agrees.
- The exact completion frame remains durable provider evidence. Equal narrative fields may reuse
  prior ProviderItemV1 ranges without copying cumulative text.
- Disagreement is a typed history-incomplete invariant violation. The live append generation remains
  sole transcript authority; completion text never replaces it.
- Existing generation-independent stable projection membership and textual-resource identity remain
  valid because the selected narrative source never changes from one byte sequence to another.
- After the affected turn terminates, Beryl invalidates the exact loaded capture session and
  reacquires the same native CAS thread. It does not declare that thread corrupt or recover through
  an incomplete Syndic prefix.

# Affected Authority

The correction is authoritative in `doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, and
`crates/syndic-storage/doc/design.md`. The retained pinned-source investigation is under
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`.

# Status

Resolved by Operator decision. The projection-source-epoch blocker is withdrawn; Phase 13 must
remove the disposable completion-snapshot WIP and continue with one append generation plus the
completion equality fence.
