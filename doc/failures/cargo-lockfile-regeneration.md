# Cargo Lockfile Regeneration

## Whole-Workspace Regeneration During A Removal Cut

On 2026-07-13, Beryl-home rework Cut 1G removed `redb`, `deunicode`, the app's direct `syndic-storage` edge, and the old Syndic store's direct Fjall edge. Running `cargo generate-lockfile` after those manifest edits did remove the obsolete dependency graph, but it also re-resolved the complete workspace and upgraded many unrelated compatible packages.

That churn violates a removal cut whose lockfile diff must be attributable only to the declared manifest-edge removals. A freshly generated lockfile is therefore not equivalent to an incremental update of the existing accepted lockfile, even when both satisfy the manifests.

The correction is to restore only the known command-generated `Cargo.lock` rewrite, preserve the existing pinned graph as input, and invoke Cargo's ordinary incremental resolution path. The resulting diff must then be inspected to prove that it changes only affected workspace dependency lists and packages made unreachable by the declared removals.

Do not hand-edit `Cargo.lock`, retain obsolete dependencies to suppress resolution, or accept unrelated upgrades merely because `cargo generate-lockfile` succeeded.
