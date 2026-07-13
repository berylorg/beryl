# Targetless Cargo Workspace

## Removal Cut Prevented Lockfile Maintenance

Checkpoint 1 initially removed every source target from `beryl`, `beryl-app`, `beryl-model`, and `syndic-storage` while leaving their manifests in workspace membership. Cargo rejects a member manifest with no target before dependency resolution, so the later Cut 1G requirement to update `Cargo.lock` through Cargo was technically impossible.

Restoring archived modules, adding an empty runnable executable, or temporarily changing workspace membership would have created transition behavior or manifest churn contrary to the rework. Deferring all lockfile work would also leave the removal checkpoint unable to verify its declared dependency edges.

The Operator authorized permanent target code on 2026-07-13. The correction is to keep documentation-only permanent library roots, keep one permanent binary root with an explicit compile-time cutover gap, and mount no implementation until its owning target checkpoint. This makes the workspace structurally valid for Cargo without claiming that Beryl builds or runs.

Future removal-first plans must schedule permanent package-root reconstruction before any Cargo-driven manifest or lockfile verification that follows complete source-root archival.
