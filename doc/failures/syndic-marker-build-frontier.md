# Syndic Marker Build Frontier

## Invalidated Assumption

The app integration assumed the accepted marker builder always committed a canonically readable
build endpoint. The opaque staged outcome API transports the actual serialized endpoint; the app
does not author its cursor fields and cannot repair them after commit.

## Decisive Evidence

The public `composer_marker_evidence` success case completes its first operation inserting three
markers. Its next operation moves the first marker and inserts a fourth. In bounded reproduction
`3ee8608b-eb08-4119-8eac-2d876c62ab02`, opaque `Advance` commits transition 38 in `CrossValidating`
with base rank/count 3/3, successor rank 5, and working sequence piece count 4. Writer admission is
present and its remaining target set is empty. The next preparation fails decoding
`syndic/draft-piece-builds` with `InvalidLength("draft-piece build record")`.

The failure is canonical-shape validation, not demonstrated truncation: `codec.rs::decode_build`
calls `build_record_is_exact`, whose successor-frontier guard rejects rank greater than working
piece count. The encoder does not apply that same final guard. The temporary app-only probe was
removed after reproduction; no Syndic production change or app endpoint workaround was applied.

Independent fixture review established that `After(first)` is invalid with three objects at that
anchor: the widget's `After` witness denotes the last object. The first-marker target was corrected
to `Before(first)..Between(first, second)`, preserving all orders, payloads, labels, assets and
assertions. The widget's insertion contract retains successor-relative object anchors and order
keys. Fresh focused nextest run `ba3c5a11-59e5-49aa-a43b-67fb64c5f217` compiled successfully but
failed the mixed edit with `MutationAdmission(TerminalCleanup { outcome: Rejected, refusal:
Some(Rejected) })`; six other cases were skipped. No internal frontier counts were measured in
that run. The earlier malformed-witness reproduction still establishes publication of a record
that its own reader rejects, independently of the valid-input correction.

## Contract Conflict

Independent source review traced the original sequence through both
`persistent/marker_program.rs::advance` and `tree/marker_progress/planning.rs::finish`. Move at
source boundary `(0,0)` removes piece rank zero, retains effective source end `(0,0)`, and maps
insertion to successor `(1,0)`. Its published tree again contains three pieces. The next Insert at
original source boundary `(3,0)` maps to `(4,0)`; inserting at actual rank three produces successor
rank five in a four-piece tree. The final decoder rejects that shape. The authenticated build
transition does not apply the same final exact-shape guard before publication.

These calculations followed the then-current authority rather than a stray producer error:
[draft storage](../../crates/syndic-storage/doc/design-draft-storage.md#bounded-marker-effect-continuation)
formerly said Move retains the shared source boundary as its effective end, and the
[closed schema program](../../crates/syndic-storage/doc/design-schema-v7.md) explicitly permits
Move's removed occurrence to differ from that boundary and derived effective end `B`. The scalar
frontiers can therefore continue counting an original occurrence already removed by Move. Changing
the effective end would change the closed program and is not by itself a demonstrated solution
for arbitrary removals beyond `B` and later source positions. Producer and receipt checker must
remain consistent with the resolved contract.

A minimal general counterexample starts with same-anchor markers `[A1, B2, C3]`: Move C to
order zero at source `BeforeAll`, producing `[C0, A1, B2]`. Original boundary rank one must map to
two, while original end rank three must map to three. One affine base/successor pair cannot give
both results. Advancing the effective source end to three would instead reject the intervening
boundary at rank one. This is a support restriction, not a complete mapping correction. A shape
guard also cannot detect every wrong mapping that remains within the working piece count.

## Correction Boundary

Readiness for the authorized frontier correction exposed this material contract conflict.
The Operator subsequently authorized the recommended full-support mapping design correction.
Bounded design/source feasibility and independent semantic/adversarial readiness review are now
complete. The owning draft-storage, schema and system documents specify the selected mapping
mechanism; implementation proceeds through its independent primitive and continuation boundaries.
The [conversation-history system contract](../systems/syndic-conversation-history/design.md)
also owns simultaneous predecessor interpretation, repeated empty marker ranges, one bounded
active effect, and bounded retained construction state. Full Move support uses an authenticated
immutable alignment tree of Copy, Deleted and Inserted runs, measured in UTF-8 bytes plus markers.
Source lookup has right affinity. Original marker removal additionally requires a surviving Copy
unit and exact current occurrence. Actual source ends govern ordering for every fragment kind.
One-leaf map updates precede sequence surgery; coherent tree/map installation and separate
frontier refresh replace the invalid scalar derivation. The new immutable mapping family follows
existing build-evidence custody and uses the shared command ledger. No narrower composition
envelope, accumulated resident delta list or new garbage-collection project is selected.

The design review found the primitive and complete V6 program ready for implementation planning.
The largest endpoint addition is 337 bytes under the conservative 480-byte allowance; the tightest
complete branch has 1,728 bytes of conservative headroom below 4 MiB. These are design bounds,
not measured production acceptance. Implementation must particularly verify deleted plateaus,
multi-leaf map deletion within one sequence leaf, split-text refresh, partial-surgery terminal
custody and complete command accounting.
Preserve canonical roots, cursor transitions, exact receipts, writer custody and the existing work
bounds. Verify legal mixed edits across command/reopen boundaries and refusal of malformed input
without committing an unreadable build. Do not substitute status reads, replay buffered fragments,
weaken decoding, or change caller endpoints to evade the defect.

A final `build_record_is_exact` check after authenticating the proposed progress reference can
refuse a noncanonical transition without new reads or records. That bounded guard is derivable
from current authority but cannot alone restore supported mixed Move behavior. It has not been
implemented as a substitute for the full correction.

The app integration and its remaining runtime acceptance remain unaccepted until this prerequisite
is resolved. The minimal fixture correction and existing app work are preserved uncommitted. No
Syndic production changes, probe, running Cargo process or fixture temporary directory remained
at the close of that readiness investigation.

## Accepted Mapping Primitive

The authenticated mapping family, canonical codecs and bounded query/splice primitive passed
independent semantic/adversarial implementation review. The focused nextest run completed all
11 mapping cases and five existing family-inventory regressions, with 16 passed and 62 deliberately
filtered cases. An isolated production Syndic library check without test features also passed.
The independent oracle checks both source-cut transformation and surviving original unit identity.
Coverage includes deleted plateaus, split/merge/redistribution, root collapse, an 18-run leaf
result, multileaf deletion and malformed codec/owner/key/digest refusal.

The complete shared immutable height-22 deletion fixture measured 43 acquisitions, 21 emissions,
64 point attempts and stored-structure charges, 27,685 emitted canonical bytes, 62,328 charged
bytes and a 62,344-byte peak reservation. These are primitive measurements; complete builder
command and outcome accounting remain part of continuation integration.

Maximum-node verification exposed a separate shared-reader limit error: a valid 1,385-byte
mapping payload occupies 1,389 stored bytes with HomeStore's four-byte record-version prefix.
Build acquisition and mutable recheck had passed the payload ceiling directly to the physical
reader. Both now use the existing family point-limit helper, which includes the prefix.
Canonical codecs and the Syndic ledger still charge payload bytes under the existing contract;
the maximum-width node regression passes without enlarging the mapping certificate.

Mapping is appended as implemented family 87, preserving all prior family order. The four
terminal-repair families in the target schema remain unimplemented and outside this correction.
The builder still requires the separately planned V6 continuation integration before the app's
mixed-edit acceptance can resume.

## Accepted V6 Continuation

The complete V6 continuation now uses original-coordinate mapping for all implemented text and
marker edits. Map updates precede sequence surgery; compact Ready states permit one atomic
coherent tree/map installation. Separate refresh commands publish the actual resulting frontier.
Build, progress and settlement codecs and digests advance together, and the final canonical-shape
guard runs after the proposed progress reference is authenticated. Selected and predecessor map
roots participate in construction, reopening and outcome custody; terminal and admitted-cleanup
paths preserve that evidence.

Focused serial nextest verification covered 90 cases across the main run and targeted correction
runs. All passed. The three new mapping binaries contribute six behavioral, five codec/receipt and
eight custody cases; existing durable-builder, marker-continuation, bounds, staged-outcome and
sequence-continuation tests provide the remaining coverage. Separate canonical V6 vectors verify
digest domains and rejection of older envelopes. Mixed-edit cases cover all marker effects,
original Copy-only removal, intermediate and EOF source cuts, repeated insertion gaps and UTF-8
continuation. A seeded text-leaf case requires four map-leaf deletions before one atomic sequence
mutation. Partial reopen proves three actual sequence removals with 126 of 129 bytes remaining.

The 257-fragment outcome case converges across two staging windows in exactly 5,912 advances
(`257*23+1`). Executed construction commands check their combined preparation/submission counters
against 256 stored-structure records, 512 point attempts and 4 MiB. Outcome commands separately
check 128 reads and 8 MiB, including mapping-root custody and cleanup. These are measured limit
assertions, not retained numeric runtime maxima. The complete sequence-insertion peak of
4,192,576 bytes and its 1,728-byte margin remain conservative analytical bounds verified by the
branch inventory and independent review.

Existing fault fixtures needed the V6 field layout and full continuation closure; old eager
mutation assertions needed to follow proof and refresh commands to the actual mutation or
publication. Those repairs preserve malformed-record refusal, exact admission consumption and
stale-command checks. Three large debug fault fixtures exceeded the default stack through their
combined suspended test frames. Extracting sequential setup, execution and recovery helpers
restored all assertions on the unchanged default stack, including all three writer fault cuts.
Production validation and stack settings were not relaxed.

Isolated production library checks for Syndic and the accepted app source passed without test
features. All 42 copied source files matched the working implementation before and after those
checks; unaccepted app and admission fixtures were excluded. Independent semantic/adversarial
review accepted the final 60-file source/test change. Temporary verification and attributable
aborted-test resources were removed. This resolves the storage prerequisite; application marker
admission still requires its own resumed runtime and integration acceptance.
