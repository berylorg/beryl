# Invalidated Approach

Phase 61 initially treated warnings-denied package-wide Clippy as a clean completion gate for
`syndic-storage`.

# Why It Failed

The all-feature library invocation reached pre-existing findings outside accepted-promotion
reconciliation: type complexity, iterator clone style, redundant locals, clone-on-copy,
too-many-arguments, double-must-use, and needless-borrow. The affected files belong to admission,
native projection, provider observation, content/range reads, recovery, record construction, and
validation. The unrestricted output reported no finding in the Phase 61 promotion files.

Rewriting those unrelated dirty modules inside the promotion slice would widen ownership and make
the verification gate drive unrelated implementation changes.

# Architectural Correction

Keep the broad Clippy output as baseline evidence. For Phase 61, run all-feature library Clippy with
`--no-deps`, deny every warning, and allow only those seven already-observed lint classes. This
still rejects any other package warning and verifies the promotion implementation without changing
unrelated architecture.

The allowance is verification scope, not permanent source authority. The owning implementation
slice or final cleanup must either resolve or deliberately configure the broader baseline.

# Reusable Lesson

Warnings-denied linting is useful only when its target membership matches the active slice. When a
dirty rework tree already has unrelated lint debt, preserve the exact baseline, prove the changed
files add no finding, and do not manufacture a green result by editing unrelated subsystems.
