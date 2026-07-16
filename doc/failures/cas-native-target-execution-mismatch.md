# CAS Native Target Execution Mismatch

## Scope

Checkpoint 3 Phase 10 native-projection planning across `syndic-storage` and `beryl-app`.

## Invalidated Approach

The planner returned `SourceExecutionMismatch` with no source whenever a correlated native source
used another execution binding. That was correct for an ancestor owned by another thread, but it
also discarded the exact current target-thread binding when that binding already represented the
required prefix.

## Evidence And Failure

`classify_native_projection` checked current-target reuse only after requiring execution equality.
The later correlated-source mismatch path then returned `source: None`. The app coordinator
interprets no source as permission to build a fresh recovery target without first retiring any
target reservation. Publishing that target over the still-valid target-owned CAS reservation is
rejected by binding-transition authority.

## Course Correction

Classify an exact-prefix current target before execution and tool-profile eligibility. Return that
target as the typed unavailable source for an execution or profile mismatch, so the coordinator
retires only target-owned authority before recovery. A mismatched ancestor source remains absent
from the unavailable plan and is never mutated.

The coordinator must carry the source binding's exact execution identity and the typed unavailable
reason into stale provenance rather than substituting the requested execution or assuming every
target retirement is a tool-profile mismatch.

## Affected Authority

- `doc/plan.md`, Phase 10.
- `doc/rework/beryl-home/REWORK.md`, Checkpoint 3 projection completion.
- `crates/syndic-storage/src/native_projection/classify.rs`.
- `crates/beryl-app/src/cas_projection/execute.rs`.
- Focused native-planning and coordinator verification.
