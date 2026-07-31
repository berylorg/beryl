---
name: failure-logging
description: Log significant invalidated approaches and course corrections. Use when implementation, design, dependency exploration, migration, live testing, test execution, or operations reveal that an approach is technically invalid, unsafe, too costly, or contrary to design/plan authority and the lesson belongs under doc/failures/<scope>.md for future reasoning.
---

# Failure Logging

## Core Rule

When an implementation, live-test, dependency, or design approach is invalidated and the course correction is significant enough to preserve for future reasoning, log it under `doc/failures/<scope>.md` unless the project explicitly declares another failure-log path.

Failure logs preserve durable lessons. They do not replace design docs, implementation plans, or bug trackers. If a failure changes target-state design, update the authoritative design docs. If it blocks implementation, update `doc/plan.md`.

Keep one record per durable root failure. When later work exposes another symptom, invalidates an
intermediate correction, or completes the same lesson, update and compact the existing record
instead of adding a chronological sibling. Preserve the final invalidated assumption, decisive
evidence, accepted correction, and unresolved risk; remove routine intermediate attempts and
superseded test inventories.

## When To Log

Log a failure when:

- A plan phase cannot work for a technical reason.
- A design assumption is contradicted by source code, tests, live behavior, upstream docs, or research.
- A dependency behaves differently than expected in an integration-relevant way.
- A migration, copy, restore, retry, cancellation, or rollback approach loses identity, state, provenance, permissions, or user intent.
- A performance or resource-use assumption fails under realistic conditions.
- A live-test path exposes a recurring or non-obvious hazard.
- The project must remember why a plausible approach was rejected.

Do not log routine syntax errors, formatting mistakes, one-off environment issues, or small bugs whose lesson is obvious from the final diff.

Delete a failure record when later review establishes with high confidence that it contains only
those excluded categories and no authoritative document relies on it. If another record already
owns the same lesson, merge the durable facts into that survivor and update inbound references
before deleting the duplicate.

## Entry Content

Keep entries concise and actionable:

- Scope or domain.
- Attempted or proposed approach.
- Evidence that invalidated it: command, test, source file, source doc, trace, or observed behavior.
- Why it failed.
- Course correction.
- Affected design docs, plan phases, tests, or exploration memory notes.
- Remaining risks or follow-up questions.

Avoid broad transcripts and speculative commentary.

## Workflow

1. Stop relying on the invalid approach.
2. Decide whether the lesson is durable enough for `doc/failures/<scope>.md`.
3. Update `doc/plan.md` if work is blocked or the implementation sequence changes.
4. Update authoritative design docs if target state changes.
5. Add or update the one root failure note with evidence and course correction; merge or compact
   overlapping history rather than accumulating attempt diaries.
6. Resume only after the contradiction or invalid assumption is resolved according to authority.
