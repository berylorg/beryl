# Mutation Protocols

This supplement is normative only for package-generic revision, publication, replay, custody,
cancellation, admission, reconciliation, acknowledgement-loss, and bounded-work rules. Record bytes
are controlled by [`design-schema-v7.md`](design-schema-v7.md). External CAS lifecycle is controlled
by [CAS live Syndic transcript](../../../doc/systems/cas-live-syndic-transcript/design.md), and
cross-domain commit and proof composition are controlled by
[Beryl home storage](../../../doc/systems/beryl-home-storage/design.md).

## Revisions And Preconditions

Every mutable authority has a nonzero monotonic typed revision independent from unrelated records.
A command names the exact revisions, immutable roots, generations, and natural identities that
justify its package effects. A revision is meaningful only with its owner and complete selected
state. No digest, cursor, external id, or process observation substitutes for an expected revision.

Preparation reads one bounded natural closure and returns an opaque command owning the exact source
and proposed target. It performs checked accounting before publication and returns a typed stale,
conflict, unavailable, limit, collision, cancellation, or corruption outcome without writing when
preconditions fail.

## Atomic Publication

One logical package transition contributes all of its Syndic writes, deletes, index changes,
receipts, roots, and revision advances to one HomeStore commit. Multi-domain operations contribute
private typed effects to the same HomeStore commit. The package never publishes a head without its
selected immutable receipt, a root without all newly referenced records, a current draft without its
matching history authority, or an index entry without its primary record.

Mutation preparation is storage-bounded and side-effect free. It does not observe clocks, generate
identities, call providers, wait for resource capacity while holding the serialized writer, retain a
whole logical value, or execute application work. No error, cancellation, or panic path exposes a
partially assembled contribution as committed authority.

## Canonical Replay

Every operation has a stable natural identity and canonical request bytes. Replay is exact only when:

- The durable head selects the proposed target rather than the source.
- The occupied natural records have canonical key and value bytes equal to the proposal.
- Every same-command primary, index, receipt, proof, root, and custody effect equals the proposed
  bounded target closure.
- The source and target revisions and predecessor references form the documented transition.

Digest, chain, summary, or correlation equality may reject a mismatch early but never proves
canonical equality. Equal target bytes while a head still selects the source is split publication
and therefore corruption. A differing request at an occupied identity is a typed collision or
occupied-identity noncommit and authorizes no alternate identity or mutation.

Earlier immutable windows may be committed by an authenticated source receipt and chain rather than
rescanned, but a detached chain digest is never replay authority. The current bounded window and all
same-command effects are still compared byte for byte.

## Typed Outcomes And Linear Custody

Prepared commands, page attempts, proof consumers, receipts, final proofs, reconciliation handles,
and ambiguous-outcome authorities are opaque move-only values. Public code may transport them but
cannot inspect, clone, forge, serialize, reconstruct, or combine their private facts.

A successful local commit yields only the typed result appropriate to the committed closure.
`ExactOld` preserves the original owner for retry or cancellation. `ExactNew` advances custody once.
`Indeterminate` transfers exclusive ownership to targeted reconciliation. A same-key canonical
disagreement closes ordinary progress as a collision until exact durable classification.

A process-local capability is valid only for its exact HomeStore generation, registered Syndic
attachment, protocol, operation, owner, page, command, and receipt. Durable records can classify an
outcome after restart but cannot recreate a prior process's attempt, proof consumer,
installation token, or completion capability.

## Cancellation And Admission

Cancellation is an explicit election before the command's irreversible publication boundary. It
publishes either the operation's canonical terminal noncommit closure or no mutation. Once the
durable target is selected, later cancellation cannot rewrite it as noncommit.

Admission checks the exact current owner/revision closure, operation identity, package byte and
record ceilings, configured shared capacity, and any required opaque proof. Failure releases only
unpublished process custody. Once durable custody exists, cleanup first publishes an inert terminal
or transfers authority into the terminal settlement; process retirement never deletes or
reclassifies durable custody.

No operation retains an unbounded resident registry, input prefix, page vector, edit, route
membership set, or history path. Large work advances through one bounded page, tree path, fragment
window, or natural closure per command.

## Targeted Reconciliation

Reconciliation requires the exact opaque handle produced by the indeterminate submission and starts
at the operation's natural anchor. It reads only the bounded source/target closure, checks every
canonical record and revision, and returns a closed result:

- Source selected: no proposed package effect became authoritative; original linear custody may
  resume only as the returned typed authority permits.
- Target selected: the complete target closure became authoritative exactly once and custody
  advances to its typed result.
- Concurrent change: a mutable anchor drifted during observation; the same reconciliation owner
  remains required.
- Corruption or collision: source and target do not form either canonical closure; no new authority
  is minted.

Mutable anchors are observed first and last. A missing or disagreeing required record under a stable
anchor is corruption, not absence or retryable uncertainty. Reconciliation never scans a family,
chooses an alternate operation, infers success from a digest, or reconstructs application input.

## Acknowledgement Loss And Restart

Commit acknowledgement loss does not authorize blind resubmission, duplicate publication, or
freshly generated identities. The single reconciliation owner must classify the exact natural
closure. A completed reconciliation failure retains that owner; a permitted retrigger operates on
the same handle and generation.

On service or attachment retirement, process-local owners are abandoned and invalid. Fresh recovery
may inspect durable records through explicit recovery APIs and owning-system rules, but it cannot
recreate old linear capabilities. Durable pending-work indexes may expose revision-bound pages for
system recovery; broad primary-family sweeps are maintenance, not ordinary restart behavior.

When an operation has an explicit package-local completion capability tied to the just-returned
committed receipt, only that exact move-only capability may finalize the active same-generation
flight. A historical or reconstructed receipt cannot mint or substitute it.

## Bounded Commit Shape

Each command has declared checked maxima for records, canonical family-key plus value bytes, reads,
decoded bytes, resident values, and tree height. Exceeding a maximum rejects before mutation rather
than clamping, saturating, widening a page, or splitting one atomic authority transition.

Canonical accounting includes every encoded field and repeated key in a value. It excludes family
names, HomeStore registration bytes, Fjall framing, allocator metadata, cache residency, compression
estimates, and filesystem allocation. Public operation-footprint descriptors describe Syndic
effects only; HomeStore metadata and other domains remain separately owned.

Payloads remain chunked, trees remain path-copied, and reads use caller-supplied pages or fixed
bounded response values. No mutation or reconciliation buffers a complete draft, content value,
history, provider payload, projection, resource, accepted-input generation, or route membership.
No boundedness claim relies on an undocumented implementation read-count derivation.
