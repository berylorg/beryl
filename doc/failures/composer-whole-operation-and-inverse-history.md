# Composer Whole-Operation And History-Lineage Assumptions

## Invalidated Approach

The large-composer design initially retained a complete logical mutation behind a finite fragment
ceiling and reconstructed undo from copied inverse text and marker witnesses. Its first durable
builder correction still required terminal totals and caller-resupplied fragments before the app-
neutral cursor protocol could know them.

The first retention design then treated a draft-global cumulative seek plus immediate predecessor
links as sufficient lineage authority. After sibling forks disproved that, the correction selected
the floor directly from a 64-slot ancestor witness but overextended ordinary reads into recursively
re-proving every committed skip derivation and root adjacency so they could detect coordinated
digest-valid post-commit rewrites.

The first app integration translated one bounded widget page into separate physical storage-page
commands and accepted older widget ordinals as replay without exact comparison. It also passed the
large by-value settlement closure through branch-heavy app and storage call chains, assuming the
ordinary worker stack was sufficient.

## Decisive Evidence

A whole-operation collection makes edit and undo capacity depend on arbitrary fragment count and
resident payload, while reconstruction loses immutable historical-root identity and cannot give one
large undo a single exact settlement. The cursor protocol learns terminal totals only at
authenticated finish, so pre-finish custody must be durable and independent of caller replay.

Independent editor sessions can fork one published frontier and append siblings at overlapping
cumulative positions, so global order cannot select the chosen lineage. Head-origin binary lifting
does select it when append admission constructed the canonical witness correctly.

Recursive post-commit proof was not a sound bounded contract. A coordinated rewrite can replace the
derivation source and its own lower derivation while recomputing every digest, so adding three reads
per level merely moves the same question downward. When transition, frontier, session, and receipt
anchors all come from the same database, they cannot promise detection of hostile replacement,
cosmic bit flips, arbitrary I/O/media corruption, or another fully self-consistent digest-valid
rewrite. That machinery complicated normal logic without strengthening the stated trust boundary.

A widget page can split into as many as 257 physical storage pages. Separate commands can publish a
durable prefix before local widget-frontier rejection, while released payload leaves the app unable
to reconcile that prefix as one widget-page effect. Native first-chance debugging of the first
one-fragment edit separately proved no recursion: oversized production frames exhausted the 2 MiB
worker stack, led by app execution, storage status, and settlement decode frames of roughly 604 KiB,
434 KiB, and 203 KiB. The test payload was not causal.

## Accepted Correction

Use the app-neutral cursor/session protocol with bounded source and proposal pages, durable two-lane
staging custody, explicit finish, immediate caller-payload release, and one terminal settlement.
Finish atomically transfers custody to the copy-on-write builder, whose acyclic canonical digests,
immutable progress receipts, and exact source/target closure support bounded replay and
reconciliation.

Append admission fully validates the source frontier and head, exact immediate predecessor, same-
draft roots and positions, cumulative and retained-byte accounting, and every derived slot in the
canonical fixed 64-level witness before one atomic transition/frontier/session/settlement commit.
Witness construction uses at most 64 transition point reads and fixed state. Replay and
reconciliation prove the complete source-versus-target atomic publication and canonical bytes;
missing, partial, stale, or colliding effects fail closed.

After correct commit, ordinary retained-history reads trust immutable witness references that pass
local key/value, codec, shape, and digest agreement. Direct selected-head lifting follows at most one
target per level and chooses the cumulative-threshold floor in at most 64 transition point reads and
fixed state. It does not recursively re-prove witness derivation or root adjacency. Siblings remain
independent and cannot be selected; a global seek is at most a non-authoritative hint.

Digests retain identity, canonical replay, accidental-local-mismatch, and cheap fail-closed decode
roles. Floor advancement remains logical-only and copies or physically deletes no transition, root,
node, leaf, or content. No proof field, record, family, index, pin family, single-lineage restriction,
or compatibility path is added.

One widget page is prevalidated before translation and admitted through one bounded atomic storage
batch over the existing staging page, progress, head, and session families. Immediate exact replay
uses the widget protocol's fixed last-page receipt; older pages are obsolete and differing reuse is
a collision. Caller payload is released only after complete batch acceptance or reconciliation.

Large settlement closure state is heap-indirected without changing canonical durable bytes, and
mutually exclusive execution or status branches are split only when measured frames still require
it. Default-stack verification is required; increasing `RUST_MIN_STACK` or moving only fixture data
is not an accepted correction.

## Affected Authority And Plan

The correction is authoritative in `doc/systems/syndic-conversation-history/design.md` and
`crates/syndic-storage/doc/design.md`. The composer feature behavior remains unchanged. Root
`doc/plan.md` Phase 148 must implement the write-admission trust boundary, at-most-64-read direct
selection, local ordinary-read validation, exact atomic replay/reconciliation, and unchanged live
family inventory.

Root `doc/plan.md` Phases 150-153 separately own stack-residency correction, atomic widget-page
storage authority, storage implementation, and resumed app integration.

The remaining normal-logic risks are malformed witness construction at admission, partial atomic-
closure classification, cumulative threshold/floor off-by-one errors, allowing a global-order hint
or sibling to influence selected-head lifting, partial physical-page batch publication, and future
by-value expansion of large settlement closures.
