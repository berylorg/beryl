# Scope

Integrate Beryl V1 with the maintained Codex app-server fork without bundling or installing it: support explicit standalone Host-Windows launch, negotiate saved-path-only generated images, consume authoritative root usage-tree snapshots, and render current selected-root `main` and delegated-descendant `sub` token totals in the existing compact status line. Keep fork-owned accounting out of upstream SQLite schema and migration namespaces. Preserve legacy server and WSL usability by treating the fork extensions as optional, never estimating descendant usage, and retaining exact target/thread ownership and bounded resources.

# Phase 9: Diagnose shared-home state-runtime initialization (finished)

The [failure record](failures/codex-fork-shared-home-compatibility.md) now identifies deterministic SQLx checksum drift: the active shared home's successful migration rows contain CRLF-derived checksums, while the current fork embeds LF-identical SQL and therefore rejects every known migration before optional Beryl accounting initialization. A content-free read-only probe covered all five fatal databases, representative byte-level hashes proved the newline transformation, and a vanilla/fork A/B excluded general shared-home unavailability. No production source or shared database content was changed.

# Phase 10: Verify the integrated Beryl/fork boundary (pending)

After the diagnosed incompatibility is resolved through separately accepted work, reuse the current-HEAD standalone artifact and retained Beryl live smoke against the active shared Codex home. The existing focused evidence is 40 passing fork tests and 192 passing Beryl tests; live acceptance still requires a successful no-model-turn initialize and usage-tree capability probe through Beryl's exact Host-Windows standalone override.
