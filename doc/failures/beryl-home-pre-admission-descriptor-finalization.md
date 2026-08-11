# Beryl-Home Pre-Admission Descriptor Finalization

## Invalidated Approach

Require every potentially indeterminate command to seal its exact old state, intended new state,
and concrete receipt revisions before it acquires the serialized writer.

## Evidence

`CurrentDomainCommand` intentionally captures physical home and domain revisions only after writer
admission. Some mutations also derive intended values from admitted reads; settings mutation, for
example, reads the current record before advancing its record revision.

## Why It Failed

The exact receipt and some exact new-state facts do not exist before admission. Releasing and
reacquiring the writer after observing them would lose their revision basis, while moving those
reads earlier would turn current-domain commands into stale caller-fenced commands.

## Course Correction

Before writer admission, a command reserves its operation scope and a conservative descriptor-byte
budget derived from command-owned identities and declared schema limits. Under writer admission it
materializes the exact old state, intended new state, and intended receipt facts into that
reservation before batch construction or any Fjall mutation.

This preserves pre-admission cancellation and bounded resource admission while retaining current-
domain concurrency semantics and exact indeterminate reconciliation evidence.

## Affected Authority

- `doc/systems/beryl-home-storage/design.md`
- `crates/beryl-home-store/doc/design.md`
- `doc/plan.md`, Phase 100
