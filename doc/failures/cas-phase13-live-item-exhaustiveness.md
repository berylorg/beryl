# Scope

Checkpoint 3 Phase 13 ordinary-turn item capture against Codex App Server 0.144.1.

# Invalidated approach

The first Phase 13 design and implementation treated `turn/completed` as a full terminal item
snapshot. It expected that payload to backfill or validate every item, allowed a sparse generic
normalization fallback, ignored several item variants during capture, and permitted deltas without
first authenticating the durable item kind.

# Evidence

Pinned source proves that live `turn/completed` carries `items = []` and
`itemsView = "notLoaded"`. For a normally finishing ordinary turn on one healthy continuously
subscribed connection, earlier same-thread item notifications are queued and written before that
status-only fence, but CAS provides no
notification replay after reconnect, resume, late subscription, or process restart.

Pinned item lifecycle is also not uniformly paired: `SubAgentActivity` is completion-only. Hosted
Responses image-generation parser support has no supported producer path because CAS 0.144.1 cannot
send the required native tool declaration. A guarded installed-runtime probe with an image-capable
provider/model passed 21 of 21 assertions and captured zero native hosted declarations. Standalone
`image_gen.imagegen` is a separate extension-owned producer.

# Why It Failed

A status-only terminal event cannot enumerate or repair missing history. Sparse generic and ignored
item paths can silently discard history-relevant fields while still publishing a complete turn.
An untyped delta can mutate content under the wrong durable item kind. Reconnecting or reading CAS
history would be a different, lossy recovery architecture rather than replay of the admitted source
sequence.

# Course Correction

Treat `turn/completed` only as the pinned healthy-stream status and ordering fence. Serially admit
the preceding stream, retain one bounded pending delta, flush it at the fence, and audit only already
admitted durable items before terminal publication. Any stream/subscription loss fails closed as
incomplete and is never repaired through implicit reconnect, resume, or historical reads.

Normalize every pinned public item through a closed typed provider representation plus its separate
canonical narrative/resource presentation policy. Exact submitted-input correlation avoids content
duplication; a presentation-only activity policy never discards public provider fields. Admit
completion-only variants explicitly. Carry the expected item kind and closed field identity through
every delta and reject kind/index mismatch before mutation. Keep hosted Responses image generation
outside the CAS 0.144.1 supported producer contract, make no complete-history claim for unsolicited
nonconforming provider behavior, and normalize standalone generated media separately. Preserve the
completed provider item and terminal lifecycle; until the later asset checkpoint admits its bytes
into Beryl-owned authority, a separate pending-resource disposition keeps canonical finalization and
history completeness behind rather than masquerading a runtime path as durable media. The later
completion review's distinct activity-only field-loss correction is recorded in
`doc/failures/cas-phase13-activity-only-public-item-loss.md`.

# Affected Authority

`doc/plan.md` Phase 13, `doc/rework/beryl-home/REWORK.md`,
`doc/systems/cas-live-syndic-transcript/design.md`,
`doc/systems/syndic-conversation-history/design.md`, `doc/systems/backend-runtime/design.md`,
`doc/systems/image-assets/design.md`,
`crates/beryl-backend/doc/design.md`, `crates/beryl-app/doc/design.md`, and
`crates/syndic-storage/doc/design.md` carry the corrected target contract.

Pinned source and probe evidence is retained under
`doc/memory/github.com/openai/codex/commit/44918ea10c0f99151c6710411b4322c2f5c96bea/`.
