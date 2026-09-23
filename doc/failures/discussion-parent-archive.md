# Archived Parent Of An Unresolved Discussion

## Invalidated Assumption

Parent existence and an idle execution gate do not imply that a discussion handoff destination can
accept another turn. A parent may itself be a discussion that has resolved and become archived.
Treating that state as temporary busy work leaves its child handoff waiting forever; admitting
input would violate permanent readonly archive behavior.

## Evidence

- The [branch feature](../features/branch-discussions/design.md#discuss-in-new-branch) binds a new
  discussion to its exact source thread without restricting sources to top-level threads.
  `syndic-storage/src/discussion_source.rs::authenticate` likewise authenticates assistant
  selection provenance without requiring a root-only source.
- [Archive behavior](../features/branch-discussions/design.md#completion-and-navigation) is readonly
  and the [handoff system](../systems/branch-discussion-handoff/design.md#success-archive-and-failure)
  archives a discussion after its own parent handoff succeeds. Neither contract makes archive
  depend on unresolved child discussions.
- `mutation/discussion_handoff.rs` validates the exact resolving attempt and local archive state;
  its release/archive participant does not gate publication on descendant work.
- `mutation/discussion_mutation.rs::require_editable` rejects archived discussion input. The
  handoff contract also forbids redirecting a child resolution to an ancestor or replacement.
- The existing failure matrix covers a missing parent and temporary unavailability, but does not
  classify a valid permanently archived parent or specify a descendant prerequisite for archive.

## Concrete Case

Main discussion M has a branch A, and A has a branch B. A resolves into M and becomes archived while
B remains open. B later resolves into its immutable parent A. A still exists, but cannot accept
the new handoff turn. This can also race an already-admitted B job before its parent input exists.

## Correction Derived From Existing Failure Policy

Preserve one-way archive and independent discussion lifecycles: while A's own resolution
is pending, B waits; if A archives before B creates parent input, reject a fresh B resolution or
terminally fail its already-admitted attempt and atomically release B's gate. B stays editable and
unarchived, retains its resolution evidence, and is never silently redirected. A distinct archived-
parent reason should identify this normal lifecycle outcome rather than claim corruption.

Independent readiness review identified the feature's existing rule that any non-collision
unrecoverable post-admission failure ends the attempt without archive. That rule resolves the
product disposition; descendant blocking would introduce new behavior rather than implement it.
Feature, handoff system and State failure authority now specify `ParentArchived` before parent
input exists, with a pending parent's resolution treated as temporary ineligibility. Phase 517
must complete the exact parent-proof and generated-input boundary before implementation.
