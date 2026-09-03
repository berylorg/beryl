# Scope

Integrate Beryl V1 with the maintained Codex app-server fork without bundling or installing it: support explicit standalone Host-Windows launch, negotiate saved-path-only generated images, consume authoritative root usage-tree snapshots, and render current selected-root `main` and delegated-descendant `sub` token totals in the existing compact status line. Preserve legacy server and WSL usability by treating the fork extensions as optional, never estimating descendant usage, and retaining exact target/thread ownership and bounded resources.

# Phase 2: Launch an explicitly selected standalone app-server (finished)

The exact standalone Host-Windows launch path now flows from CLI through application bootstrap to target-local backend launch, invokes the server without the CLI-only subcommand, preserves existing Codex CLI and WSL forms, and fails invalid or missing configured paths without fallback. Formatting, 57 backend launch/protocol tests, 10 bootstrap/propagation tests, 14 CLI tests, and the all-targets workspace check passed; repeat independent review found no issues. Real-fork launch remains Phase 7 integration acceptance.

# Phase 3: Negotiate saved-path-only image delivery (pending)

Make every Beryl backend session request `savedPathOnly`, preserve legacy-server compatibility, and verify live/history generated-image handling continues through authoritative saved paths without requiring inline bytes.

# Phase 4: Normalize authoritative usage-tree snapshots (pending)

Add the fork's schema-version-1 read and notification surfaces to `beryl-backend`, including unsupported-method distinction, root identity, revision, completeness, and exact totals.

# Phase 5: Project selected-root usage-tree state (pending)

Seed and update selected-root usage-tree state without child enumeration, reject stale/cross-root updates, and retain honest legacy and partial fallbacks with focused state-transition tests.

# Phase 6: Render root plus descendant token counters (pending)

Render the exact compact `I/IC/O: main: … sub: …` status text from accepted selected-root state, preserving k/M/B formatting, independent unknown groups, and root-only context-space semantics.

# Phase 7: Verify the integrated Beryl/fork boundary (pending)

Build and exercise Beryl against the exact standalone fork artifact, covering startup selection, initialize capability, read/update routing, selected-thread isolation, legacy/partial states, descendant changes, and saved-path image delivery without inline result bytes.
