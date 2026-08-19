# Scope

Range-backed composer edit settlement and durable autosave publication.

# Invalidated Approach

Treat widget `Committed` as the same operation that advances the durable current-draft selector,
then authenticate resumable build transitions with a mutable head self-hash and tagged progress
values inside the canonical proposal-fragment family.

# Decisive Contradiction

The range-backed widget has one transaction slot and requires each edit to settle before the next
edit can use its successor, while the product requires timed autosave, continued editing during an
in-flight save, and lifecycle flush barriers. One-stage publication would either serialize typing
behind autosave or falsely report an editor transaction committed before its required durable
selector publication had settled.

A mutable self-hash cannot prove that no bounded build transition was skipped, forked, deleted, or
replaced across an ambiguous commit. Storing progress values beside canonical fragments also breaks
the existing one-based fragment key/value contract and leaves fragment-ahead versus head-ahead
states without an immutable closure point. Treating a byte-identical occupied next receipt as replay
while the mutable head still selects its predecessor would silently accept or repair the same split.

# Accepted Correction

Use two stages: `Committed` means exact durable adoption of an immutable predecessor-linked root
into one bounded editor-candidate session; autosave or flush separately selects an eligible
candidate as the durable current draft. Fresh activation trusts only that selector, so a crash may
discard post-autosave candidates without exposing partial state. Edit root, build, settlement, and
custody identities are session-qualified. Each session head has one bounded active-operation slot:
the first admitted transition claims the exact operation/proposal identity, every continuation and
reconciliation binds it, and the sole terminal settlement clears it atomically. Only unadmitted,
never-claimed staging may become a disposed-session orphan, so old disposed orphans cannot block a
fresh session while admitted work cannot be silently abandoned.

Use the dedicated append-only `draft-piece-build-progress` primary family for one immutable fixed-
size receipt per bounded transition. Each receipt is keyed by exact draft, session, operation, and
one-based transition ordinal and binds its immediate predecessor and referenced progress closure;
`None` is the source exactly at ordinal one, including a terminal-before-ordinary-begin election.
Each command names separate source and target receipts/effects. Source-head continuation requires an
absent target; exact replay requires the head already to select the target and every same-command
effect to match byte for byte. A predecessor-head/occupied-next split always fails closed, while
`draft-piece-build-fragments` remains canonical-fragment-only.

# Affected Work And Residual Risk

The composer feature, Syndic conversation-history system, `syndic-storage`, `beryl-app`, and Phase
142 plan authority require this split. Implementation must preserve exact receipt replay and
collision, immediate-predecessor/root closure, ambiguous-writer custody, asset-owner atomicity, and
bounded candidate-lineage validation. Rejoining adoption and publication, dropping or clearing an
admitted session custody slot before terminal settlement, dropping the session from
edit natural keys, overloading canonical fragments, trusting a mutable head without its receipt
endpoint, or treating occupied-next equality as replay before the head advances would recreate the
failure.
