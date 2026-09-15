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

## Recommended Correction

Define and implement bounded snapshot-safe version-history retirement under physical memtable
pressure, using the owned dependency's existing maintenance custody before journal admission.
Preserve genuine pins, hard limits, bounded metadata traversal and typed refusal when reclamation
cannot provide capacity. Reassess aggregate active-table rotation only if pressure remains after
eligible history retirement; that broader mechanism is not yet justified by this reproduction.

Preserve input sizes, image counts, repeated payloads, storage limits and service lifetime. Do not
raise the limit, reopen between cases, retry without evidence of pending progress, or reshape the
payload merely to pass qualification. Diagnosis is accepted; the production correction requires
its separate design and implementation boundary. App qualification, candidate recovery and
executable bootstrap remain unaccepted.
