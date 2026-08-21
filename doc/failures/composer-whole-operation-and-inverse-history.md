# Composer Whole-Operation And History-Lineage Assumptions

## Invalidated Approach

The large-composer design initially retained a complete logical mutation behind a finite fragment
ceiling and reconstructed undo from copied inverse text and marker witnesses. Later corrections
still assumed that SourceSelected custody could live only on the worker stack, that one global
operation high-water could classify page reuse across binding changes, that the app could rebuild a
post-finish proposal prefix, and that every move or same-id marker replacement could be forced into
fragment one.

The first retention design then treated a draft-global cumulative seek plus immediate predecessor
links as sufficient lineage authority. After sibling forks disproved that, the correction selected
the floor directly from a 64-slot ancestor witness but overextended ordinary reads into recursively
re-proving every committed skip derivation and root adjacency so they could detect coordinated
digest-valid post-commit rewrites.

The first app integration also translated one bounded widget page into separate physical storage-
page commands and accepted older widget ordinals as replay without exact comparison. It passed the
large by-value settlement closure through branch-heavy app and storage call chains, assuming the
ordinary worker stack was sufficient.

## Decisive Evidence

A whole-operation collection makes edit and undo capacity depend on arbitrary fragment count and
resident payload, while reconstruction loses immutable historical-root identity and cannot give one
large undo a single exact settlement. The cursor protocol learns terminal totals only at
authenticated finish, so pre-finish custody must be durable and independent of caller replay. After
finish, a builder that lacks authenticated consumed-lane frontiers can find a late page only by
restarting at ordinal one or accepting a caller-reconstructed prefix; neither is bounded restart
authority.

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

SourceSelected means the exact physical-page target remains absent, not that the request may be
retranslated or discarded from stack-only custody. A binding-independent high-water misclassifies
stale ABA completions when a new activation/session/base reuses an operation or ordinal. Marker
moves and same-id replacements may naturally occur on any proposal page; requiring fragment one
either prescans/reorders the widget stream or buffers earlier pages. Splitting removal from later
placement also loses the one bounded closure needed to validate the exact source occurrence and
successor anchor/order together.

## Accepted Correction

Use the app-neutral cursor/session protocol with bounded source and proposal pages, durable two-lane
staging custody, explicit finish, qualified app frontiers, and one terminal settlement. The app
retains one current validated widget page and its one prepared atomic physical-page batch while
SourceSelected remains possible. Only TargetSelected advances the widget frontier and releases that
payload; exact pre-admission cancellation may discard it only after source selection with every
target absent. Operation high-water is reset and qualified by the exact binding, candidate session,
and base revision, so stale ABA completion conflicts rather than reinterpreting coordinates.

Finish atomically transfers custody to the copy-on-write builder. The existing build and progress
records carry the finished-staging reference, consumed source/proposal lane frontiers, canonical
fragment endpoint/chain, structure frontiers, and at most one bounded pending marker effect. Storage
therefore point-reads only the next durable staging window after restart and never scans from ordinal
one, accepts caller bytes, or uses app-built prefix reconstruction. A window has at most 256 physical
pages/items, two page/receipt reads per page, eight fixed endpoint-read slots, 520 total acquisition
reads, and 34,078,720 complete encoded-value bytes. The independent limits remain 256 fragments and
65,536 inserted UTF-8 bytes. Source-only windows durably advance even when they create no fragment.
Per-command byte-equal source/target closure and cumulative authenticated checkpoints close replay
without rescanning earlier pages.

Every marker insert, removal, move, or same-id replacement is one self-contained proposal item on
its natural page. A move/replacement carries its exact predecessor occurrence facts and complete
accepted successor anchor, identity, label, order, and checked charges. It carries no caller gap or
neighbor witness. Storage derives the exact immediate insertion gap and neighbors from its removal-
applied current working roots; future-anchor dependency, repeated identity, order collision, charge
mismatch, source mismatch, or partial effect rejects without a widget pre-scan, semantic reorder,
prior-page buffer, or operation-wide marker map.

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
node, leaf, or content. No additional history proof field, history record, family, index, pin family,
single-lineage restriction, or compatibility path is added.

One widget page is prevalidated before translation and admitted through one bounded atomic storage
batch over the existing staging page, progress, head, and session families. Immediate exact replay
uses the widget protocol's fixed last-page receipt; older pages are obsolete and differing reuse is
a collision. SourceSelected retains the exact request and prepared command; caller payload is
released only after complete batch acceptance or TargetSelected reconciliation.

Large settlement closure state is heap-indirected without changing canonical durable bytes, and
mutually exclusive execution or status branches are split only when measured frames still require
it. Default-stack verification is required; increasing `RUST_MIN_STACK` or moving only fixture data
is not an accepted correction.

## Affected Authority

The correction is authoritative in `doc/systems/syndic-conversation-history/design.md`,
`crates/syndic-storage/doc/design.md`, and `crates/beryl-app/doc/design.md`. The composer feature
behavior remains unchanged. Existing staging/build/progress families are sufficient; their build-
head and progress-receipt shapes must carry the authenticated continuation and pending-effect fields,
without adding a family or compatibility path.

The remaining normal-logic risks are malformed witness construction at admission, partial atomic-
closure classification, cumulative threshold/floor or staging-frontier off-by-one errors, allowing a
global-order hint or sibling to influence selected-head lifting, accepting a stale binding-qualified
high-water, partial physical-page batch publication, storage-derived gap/neighbor misvalidation
across pages, and future by-value expansion of large settlement closures.
