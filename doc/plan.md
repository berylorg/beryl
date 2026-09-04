# Scope

Integrate Beryl V1 with the maintained Codex app-server fork without bundling or installing it: support explicit standalone Host-Windows launch, negotiate saved-path-only generated images, consume authoritative root usage-tree snapshots, and render current selected-root `main` and delegated-descendant `sub` token totals in the existing compact status line. Keep fork-owned accounting out of upstream SQLite schema and migration namespaces. Preserve legacy server and WSL usability by treating the fork extensions as optional, never estimating descendant usage, and retaining exact target/thread ownership and bounded resources.

# Phase 8: Cut over the prototype shared migration safely (finished)

The fork now recognizes only the exact pre-separation migration-53 checksum, canonical ledger state, complete schema, and semantically valid records; it copies or verifies them in the separate store before exact transactional source cleanup. Refused states remain untouched, interrupted publication retries idempotently, and source-first serialization prevents stale current initializers from rejecting legitimate newer destination activity. Formatting, scoped diff checks, 11 focused cutover tests, and one runtime-fallback test passed; independent completion review accepted the durable-state removal boundary with no remaining findings.

# Phase 9: Verify the integrated Beryl/fork boundary (pending)

Build the corrected standalone fork artifact and exercise it through Beryl's supported Host-Windows override against the active shared Codex home. Cover startup selection, initialize capability, usage-tree read/update and reconnect routing, selected-thread isolation, complete/partial states, descendant changes, vanilla-CAS coexistence, and saved-path image delivery without inline result bytes; finish the retained opt-in live smoke without starting a model turn.
