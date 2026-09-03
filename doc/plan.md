# Scope

Integrate Beryl V1 with the maintained Codex app-server fork without bundling or installing it: support explicit standalone Host-Windows launch, negotiate saved-path-only generated images, consume authoritative root usage-tree snapshots, and render current selected-root `main` and delegated-descendant `sub` token totals in the existing compact status line. Preserve legacy server and WSL usability by treating the fork extensions as optional, never estimating descendant usage, and retaining exact target/thread ownership and bounded resources.

# Phase 5: Project selected-root usage-tree state (finished)

`beryl-app` now seeds and streams exact selected-root usage-tree snapshots through one per-root, strictly-newer revision guard. Complete accounting, partial accounting, legacy root fallback, and root-self context occupancy remain independent; optional read failures are nonfatal, and no descendant enumeration or tree persistence was added. The 133-test focused suite, corrected 62-test status suite, formatting, and serialized all-workspace/all-target/all-feature check passed; final independent review found no issues.

# Phase 6: Render root plus descendant token counters (pending)

Render the exact compact `I/IC/O: main: … sub: …` status text from accepted selected-root state, preserving k/M/B formatting, independent unknown groups, and root-only context-space semantics.

# Phase 7: Verify the integrated Beryl/fork boundary (pending)

Build and exercise Beryl against the exact standalone fork artifact, covering startup selection, initialize capability, read/update routing, selected-thread isolation, legacy/partial states, descendant changes, and saved-path image delivery without inline result bytes.
