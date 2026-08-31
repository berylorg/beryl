# Scope

Resume Checkpoint 4 of the Beryl-home architectural rework tracked by
`doc/rework/beryl-home/REWORK.md`. Protect concrete supported-envelope consequences without
duplicating dependency guarantees, runtime validation, or review machinery.

Preserve the architecture that has real product cost and purpose: bounded GUI/editor/transcript
streaming, explicit Beryl-owned resource limits, atomic durable mutation, exact acknowledgement-loss
reconciliation, Syndic/CAS identity and external-effect fencing, cross-domain asset proof, and
bounded recovery. Do not infer whole-store validation, continuous semantic proof, blanket
adversarial review, arbitrary-scale support, or duplicated consumer checks from persistence or an
unqualified verification requirement.

Keep only the just-completed draft-marker boundary and the next bounded composer-paging
reconciliation slice in the durable plan. Later GUI, repair, recovery, branch, asset, integration,
and closure boundaries remain represented by the active rework tracker until their own bounded
slices are ready. Functional checks run normally; sustained stress or performance work requires a
concrete unresolved question and prior Operator AC-power coordination.

# Phase 226: Integrate Exact Historical Draft-Marker Adoption (finished)

Integrated opaque marker-bearing undo/redo selection, exact immutable marker-root authentication,
writer-time monotonic protection containment, atomic candidate/history/settlement/frontier
adoption, and full-closure acknowledgement reconciliation without scans, inverse content, ordinary
admission, Asset work, or public structural custody. Focused Beryl tests passed 12/12, the retained
historical and marker-admission regression passed 103/103, feature and non-feature checks passed,
and fresh independent semantic review found no correctness issue or unnecessary production
complexity.

# Phase 227: Reconcile The First Cursor-Paged Composer Slice (pending)

Reconcile the remaining whole-payload composer mutation and residency tracker item against current
feature, system, package, GUI, `gpui-text-input`, and live-source authority. Select the smallest
end-to-end implementation boundary that removes one live whole-payload assumption through bounded
cursor paging while preserving exact mutation identity, editor-visible behavior, focus/selection,
and owned-resource release.

Do not prebuild autosave, submission, compact restoration, very-large-draft evidence, or a
compatibility bridge merely because they share the broader tracker item. Record the exact supported
envelope, effective engineering-rigor consequences, source seams, tests, and completion review in a
concrete next implementation phase; stop and surface any authority contradiction or technically
invalid assumed seam rather than inventing an adapter.
