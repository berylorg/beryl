# Draft Marker Replay Cleanup Accounting

## Logical Associations And Physical Replay Records

Terminal cleanup cannot count every persisted target leaf as one live marker association.
Assignment retains an old target leaf for exact receipt replay while replacing the live leaf;
the retained charge includes both leaves' bytes but only one logical association.

Fresh-asset cancellation originally exposed ordinary terminalization dropping the selected receipt
without reclaiming its shadow target leaves. The correction deletes obsolete target leaves with
terminalization and subtracts their encoded bytes only. The historical focused fresh-readiness,
close and tree run passed 27 cases covering partial-assignment and ready cancellation, rejected
cancelled-proof consumption without state changes, and complete cleanup.

## Writer Handoff Reproduction

After the V6 build-frontier correction in `0042f2e`, the public app witness reaches the same defect
through a different entry path. `admission/writer/begin.rs::prepare_draft_marker_writer_begin_v1`
removes the selected assignment receipt and its metadata charge but retains its obsolete target
leaf. Its contribution emits no node deletion. Later writer terminalization carries this residue
into exact admission cleanup.

Diagnostic run `24be1849-994e-4f6e-8aa7-38b736a61d39` establishes the first failing guard in
`admission/terminal/mutation.rs::prepare_cleanup`: logical associations `1` minus physical target
leaves `2`. The owned generation-1 head is `TerminalCleanup`, with zero builders, empty roots,
initial cursor `SourceOrder(None)`, and charge of one head, one association and 2,088 bytes. The
first cursor page contains one source leaf and two target leaves for the same marker, page,
evidence and AssetId: one assigned and one unassigned. Their encoded charge is 985 bytes; the
old head occupies 429 bytes. No cleanup progress has occurred.

```text
cargo +stable --config .cargo/local.toml nextest run --locked -p beryl-app --features test-faults --test composer_marker_evidence --test-threads 1 -E 'test(cancellation_resumes_the_submitted)'
```

The original failure is `ContributorValidation { domain: "syndic", source: Charge }`, classified
publicly as `MutationAdmission(TerminalCleanup { outcome: Cancelled, refusal: Some(Rejected) })`.
The initial app run compiled ten cases, with seven passing across that run and fixture corrections;
build cancellation, service disposal and mismatched staging-replay cancellation remained blocked.
The app uses the required public cleanup API with exact custody; it cannot discard that owner or
claim successful cancellation. Temporary diagnostics were removed byte-exactly before correction.

## Accepted Writer Handoff Correction

Writer begin now reclaims the obsolete target while its selected readiness receipt still
authenticates it. It exchanges the exact physical byte charge without decrementing the live
association and reserves the deletion effect for ambiguous-outcome reconciliation. Metadata,
acquisitions, writes and deletions share the existing whole-command allowance.

The old cleanup helper's descriptor/current-target checks alone did not establish supersededness.
Writer begin and ordinary terminalization now compose complete receipt-transition verification
and retained-node authentication using one ledger/cache before deletion selection. This follows the
[canonical admission deletion contract](../../crates/syndic-storage/doc/design-draft-storage.md#canonical-admission-deletion).
The verifier also authenticates non-EOF ingestion and partial-page cancellation through the exact
selected head's cursor and ordinal. Control point limits match their pre-acquisition reservations.
The derived retained-predecessor maximum remains 52; no schema, public API or profile changed.

Independent review checked the complete writer-begin inventory: 13 control reads, five puts and
one receipt deletion, plus bounded retained-node and target-path work. The conservative analytic
peak is 2,416,681 bytes, below 4,194,304; at most 83 points and 70 stored-node acquisitions fit the
existing 512/256 caps. These are static bounds, not measured runtime-counter claims.

Four new real fresh-readiness cases verify exact physical-byte and aggregate exchange, preserved
live target, complete pre-build cleanup, both persistence fault boundaries, rejected reordered
predecessors without publication, and partial-page cancellation. They passed in
`ba807d1b-0ddd-4116-8fe2-872f1ab7d8a0`. After `AfterPersist`, the original committed outcome consumes
local finalization before recovery; replay cannot revive prior-generation writer authority and
must leave the recovered handoff unchanged. `AfterCommitBeforePersist` separately exercises the
captured reconciliation path.

All 49 unique focused storage cases passed across fresh-readiness/close/tree run
`26d8ff01-d667-47eb-9e4f-590c0e7ceb53`, writer run `34f3857c-2443-423c-9193-be08cf1ee830`, and final
two-case run `5245f605-898c-4ad8-abdf-ab43207fc263`. The ready-target fixture now derives a canonical
final assignment instead of fabricated transition metadata, preserving its explicit labels and
256-target bound. Legacy assertions now account for bounded cleanup and exact V6 mapping stages.
The stale-clone loop retains every assertion in a separate helper and passes with the unchanged
default stack; the earlier overflow occurred during setup before that loop began.

All ten public app evidence cases passed in `c553e5ec-6942-471e-95b5-b569370c423a`, including the
three original cleanup witnesses. The disposal fixture distinguishes monotonic session generation
from the unchanged candidate identity, root, history and extent.

Locked production Syndic and app checks passed in a detached `0042f2e` worktree overlaid with only
the ten production files; unaccepted app work, test fixtures and local configuration were excluded.
Source hashes matched before and after. Independent semantic/adversarial review passed for the
same production and storage-test artifacts. Task-owned temporary homes, logs, verification worktree
and target were removed; shared build artifacts were preserved. App refusal presentation, mounted
regressions and final app acceptance remain separate work.
