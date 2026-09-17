# Research Cargo Isolation

The disposable subscription research helper initially assumed its standalone workspace isolated
Cargo's lockfile. On 2026-09-17, `cargo generate-lockfile --offline --manifest-path <helper>` inherited
the root local resolver.lockfile-path override and replaced the derived `.cargo/local/Cargo.lock`
with the helper's small graph; the following locked validation exposed the mismatch.

A standalone manifest and separate target directory do not isolate inherited Cargo configuration.
Before any write, inspect the effective lockfile path and explicitly override it to the task-owned
helper path. Apply that override consistently to lock generation, metadata, checks, builds and tests.
Use a neutral invocation directory and an explicit helper Cargo configuration to avoid inheriting
unrelated local dependency patches as well; preserve the required compiler/build settings there.

The affected derived root lockfile was reconstructed from the unchanged tracked Cargo.lock with
the existing local patches, using offline metadata resolution. Full metadata reported the root's
763-package graph; locked no-deps metadata then passed. This restores a coherent derived graph,
not a claimed byte-for-byte restoration of the prior untracked cache. No tracked root manifest,
tracked lockfile, production source or credentials changed, and no software was downloaded.

The research helper remains isolated under ignored localtest; its source and build artifacts are
temporary. This lesson changes research-tool invocation, not production architecture or the
implementation hold in doc/plan.md.
