# Draft Build Qualification Reaches Aggregate Memtable Capacity

Scope: original same-service submitted-input scale qualification and storage capacity diagnosis.

## Evidence

After marker-invariant correction `ad8448b1` on 2026-09-15, the unchanged app scale test completed
all four marker-free inputs and both the 16-image and 64-image inputs. During construction of the
first 128-image input, a text append after marker 46 was safely refused before submission.

The failure names draft seed 157, session seed 46, operation 93 and predecessor candidate generation
92. The predecessor contains 138 pieces, 46 markers and 241,408 UTF-8 bytes. The pending operation
has two bounded text fragments. Its original storage error is
`PolicyDenied(MemtablePayloadBytes)`: requested 268,438,798 bytes, limit 268,435,456 bytes, commit
state `NotCommitted`. The build remains pending; no successful adoption is claimed.

The exact test was `submitted_input_logical_work_scales_and_local_capacity_releases` in the
`beryl-app` `submitted_input_residency` target with `test-faults`, locked dependencies and one test
thread. Run `e9564316-848f-49e5-9b2f-4d1fbac9a4d3` failed after 149.615 seconds. This is distinct
from the prior shape-dependent marker-digest rejection, which the 64-image input now passes.

## Classification And Boundary

This safe policy refusal establishes a qualification blocker, not a demonstrated storage
correctness defect. It does not by itself identify retained data, pending flush work, a maintenance
failure or an incorrectly sized production limit.

The fixture holds one physical home and one projection connection service across both four-input
series. Its release and repeated-large-input evidence is collected in that same service lifetime.
Physical reopening would replace the service and handles, changing this evidence. HomeStore has
no supported memtable flush/rotation operation in its public fixture boundary. Fjall journal
persistence does not release memtables; individual-keyspace rotation thresholds alone do not explain
why aggregate capacity was reached here. Independent review found no existing supported
fixture-only remedy that preserves the workload and same-service evidence.

## Accepted Diagnosis

The Operator authorized diagnosis on 2026-09-15. Two bounded instrumented runs of the unchanged
workload reproduced the refusal. Physical payload at the cut was 268,434,712 bytes; current logical
payload was 201,321,606 bytes and the partially prepared batch held 2,398 bytes. The exact remaining
67,110,708 bytes / 13,566 records equal the completed `_beryl_domains` flush.

At refusal, there were zero live snapshots, the GC watermark had advanced from 13,741 at flush to
17,870, and the flushed keyspace retained three versions. Its flush visibility was 13,745. No
latest sealed tables, pending rotation/flush work or maintenance terminal remained. Every active
keyspace was below its 64 MiB rotation threshold.

Eligible version history still owns the flushed memtable: flush uses its earlier watermark to
retain the predecessor, while later snapshot closure advances only the tracker. Version retirement
waits for another rotation or version-publication event; no-op compaction does not perform it.
Independent review accepted this source-and-runtime explanation. It establishes an eligible
retained owner without claiming exclusive backing ownership.

The [dependency investigation](../memory/github.com/berylorg/fjall-fork/commit/0814f1875e3a35727ab3ae32bcf582908f431676/aggregate-memtable-retention.md)
preserves exact counters, source identities, dirty LSM-tree source hashes and the proof boundary.
Temporary probes were removed and the Fjall source again matches its baseline.

## Accepted Retirement Correction

The Operator authorized the correction on 2026-09-15. Fjall commit `a035895` adds one supervisor-owned
snapshot-safe history-retirement pass when prospective batch bytes or records exceed physical
headroom, before acquiring the journal writer. It refreshes the safe watermark, visits metadata
and each admitted application keyspace once, and uses existing dependency version locks. Actual
record preparation still enforces both physical limits. Pins remain charged until backing leases
are destroyed; retirement errors preserve their cause and return `NotCommitted` before journal
mutation. The correction covers batch admission and does not change direct keyspace mutations.

Focused tests passed 6/6 (`72ca7b4d-5760-4c38-abfd-48682cd609cc`), including both byte and record
pressure, readable pinned snapshots, release followed by successful same-database admission, and
recovery after success or injected retirement failure. Full Fjall test-fault verification passed
253/253 (`c3566319-4f11-48e4-9b67-6e43b18778fa`, 14.204 seconds). Normal Beryl-resolved
`cargo check -p fjall --locked`, formatting and diff checks passed. Independent semantic review
accepted snapshot safety, lock ordering, physical custody and pre-journal classification.

Dependency tests ran from the Fjall checkout with an offline-generated temporary lockfile, one
Cargo job, LLVM linking, no normal debug information and no incremental compilation. The temporary
lockfile was removed after verification. Nested LSM-tree source was unchanged. Tests inject failure
at the metadata entry; partial-pass filesystem errors and concurrent interleavings were reviewed
semantically. Traversal is bounded; filesystem and lock latency are not given a wall-clock bound.

## Remaining Qualification Boundary

The unchanged app scale run `75acd1b0-1a57-4fff-8c50-3dfce71b68f3` completed the first 128-image
input, then failed after 229.274 seconds while constructing the repeated 128-image input. Draft
seed 158, session seed 47, operation 68 attempts marker 34 after predecessor generation 67
(33 markers, 101 pieces and 178,432 UTF-8 bytes). The build is complete but its adoption remains
pending. The refusal is `PolicyDenied(MemtablePayloadBytes)`, requested 268,444,350 bytes against
268,435,456, with `NotCommitted`.

History retirement is accepted, but it is insufficient to complete the whole workload. The original
run did not identify the remaining physical owner; the bounded follow-up below resolves that gap.

## Accepted Remaining-Pressure Diagnosis

Two unchanged-workload captures account for all remaining charge at the refusal: active memtables
hold 268,423,612 bytes / 122,859 internal records across all 115 keyspaces; the prepared batch prefix
holds 4,967 bytes / four records. Their exact sum is physical charge of 268,428,579 bytes / 122,863
records. Metadata charge is zero, every latest sealed-table count is zero and every history has
one version. There are zero live snapshots and no maintenance terminal. The second capture also
confirms rotation, flush, compaction and metadata progress are complete and idle with no waiters.

The earlier retirement correction releases 67,110,708 and 67,111,708 bytes in its first two pressure
passes, then releases zero at this later cut. The remaining cause is aggregate active-memtable
accumulation, with no unexplained additional charged historical or pinned backing. Active charge
includes internal MVCC versions and tombstones, not only current application-visible values.
Every active keyspace remains below the 67,108,864-byte individual rotation threshold; the largest
holds 66,771,824 bytes. Individual rotation therefore does not supply aggregate headroom here.

Runs `9e736b80-6a2d-4c4a-bac1-6e96220594d7` and `a311d92d-2f78-4e70-b67c-a0da0c874761` reproduce
the safe refusal after 238.516 and 230.852 seconds. The repeat adds the distinct rotation-progress
counter omitted from the first capture. Independent review reconciled every row and accepted the
diagnosis. Temporary probes were removed; Fjall again exactly matches `a035895`.
These expected failures provide diagnostic evidence, not app qualification.

## Recommended Aggregate-Pressure Correction

Define and implement bounded aggregate-pressure maintenance with snapshot-safe physical reclamation
that can finish without future application writes. Preserve genuine pins, hard limits, exact
supervisor-owned progress and pre-journal failure classification.

Rotation alone is not sufficient under current source semantics: a flush publication at sequence
`S` advances visible sequence to `S + 1`, while idle snapshot GC reaches watermark `S`. Existing
predecessor retirement requires the replacement version's sequence to be strictly below that
watermark, so waiting for the flush and refreshing GC alone can leave its predecessor charged.
The correction must resolve this progress boundary in owning design authority; do not fake a write,
raise a watermark without proof, or substitute polling for reclamation progress.

The [dependency investigation](../memory/github.com/berylorg/fjall-fork/commit/a035895a5cb694bcbfb53d0ae68c8bcd1bd7f5b1/remaining-active-memtable-pressure.md)
preserves counters, exact source identity, sequence reasoning and evidence limitations.

Preserve input sizes, image counts, repeated payloads, storage limits and service lifetime. Do not
raise the limit, reopen between cases, retry without evidence of pending progress, or reshape the
payload merely to pass qualification. Further production changes require their own design and
implementation boundary. App qualification, candidate recovery and executable bootstrap remain
unaccepted.

## Aggregate Admission Recovery Blocker

The exact snapshot-floor component is accepted in Fjall `1f4d863` and Beryl `6b5f8318`.
Eight focused and all 255 dependency cases, normal compilation, formatting and independent review
passed. Ordinary aggregate admission remains pending; the component test establishes recovery with
an explicit journal cut and does not prove that a physical-pressure trigger is sufficient.

Read-only recovery review invalidated that proposed trigger. Raw recovery admission accumulates
journal-derived payload and record charge, checks each complete prospective batch, and discards
persisted-covered charge only after a sealed journal ends (`src/recovery/journal_admission.rs`).
Prepared replay follows the same boundary (`src/recovery/journal_replay.rs`). A late journal cut
cannot remove a replay peak already present inside the file.

A supported source-derived counterexample uses default LZ4 compression and repeated ordinary
single-record batches overwriting a one-byte key with a compressible 1 MiB value. Every 64 records
crosses the 64 MiB individual memtable threshold while encoded journal size remains far below the
64,000,000-byte journal threshold. With workers completing between chunks, subsequent rotations
refresh GC and retire the previous flushed history through ordinary
`src/keyspace/maintenance.rs`. Live physical charge can remain around two chunks, below 256 MiB,
while 256 records in one journal require 256 MiB plus 256 key bytes during raw recovery admission.
This is semantic source evidence, not a newly executed reproduction. No test-only retirement or
raised limit is needed for the schedule.

Therefore a cut and write exclusion only when current physical charge is insufficient cannot
establish same-policy reopenability. Even the ordinary background path can release charge before
the capacity predicate runs. A recovery-aware admission boundary must account for journal-derived
replay charge independently of physical residency, before the batch that would exceed either limit.
The recommended next design preserves existing recovery semantics and adds bounded replay-capacity
accounting and progress selection before aggregate flushing. Exact snapshot retirement, real pins,
hard limits and pre-journal failure classification remain required.

Filtering persisted-covered records during recovery is a separate architectural alternative, not a
local shortcut: current clear replay invalidates the persisted watermark and clears the prepared
tree. A plain persisted-sequence filter can skip records using state that a replayed clear destroys.
That alternative would require its own clear-aware admission and replay authority and proof.

Dependent implementation is stopped under the Operator's technically-invalid-plan rule. Complete
the recovery-aware admission design before activating root phase 431 and Fjall phase 108. App
qualification remains paused. No aggregate-admission production changes were made.
