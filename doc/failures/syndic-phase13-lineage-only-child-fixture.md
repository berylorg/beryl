# Syndic Phase 13 Lineage-Only Child Fixture

## Scope

Phase 13 end-to-end app verification of inherited image-label collision checks.

## Invalidated Approach

A proposed app test created an ordinary root thread, then used the physical fixture seam to rewrite
only its `ThreadRecord` with a parent, lineage digest, ancestor skip, and inherited label frontier.

## Evidence

The inherited same-asset and different-asset checks passed, but independent review found the stored
child had `parent_thread_id = Some` and `context_owner_id = None`. Syndic reopen validation permits
only a root with neither value or a child with both values, a matching context envelope, and a
matching parent index. A real child idle submission also moves that context authority atomically.

A Phase 61 activity-handoff fixture exposed the complementary mistake: `CreateThread::from_tail`
copies one exact selected tail into a new independent ordinary thread, but intentionally retains
root lineage with no parent or context owner. It therefore cannot stand in for a discussion child.

## Why It Failed

Lineage facts alone are not a valid discussion-branch state. The fixture exercised the app's
bounded origin lookup and Asset point-read join, but skipped production branch semantics and the
context move performed by real child admission. Passing on production-unreachable state cannot
close an end-to-end branch-inheritance gate.

## Course Correction

The invalid fixture and its two tests were removed. End-to-end branch proof must start from a
semantically complete child produced by the authoritative branch workflow, or from an exact fixture
that includes and reopens every required context, parent-index, source-history, and reverse-
agreement record. A lineage-only fixture must never be accepted as completion evidence.
Thread-from-tail creation is likewise valid only for its independent-thread contract, not as a
shortcut to child lineage.
