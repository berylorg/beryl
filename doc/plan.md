# Scope

Integrate Beryl V1 with the maintained Codex app-server fork without bundling or installing it: support explicit standalone Host-Windows launch, negotiate saved-path-only generated images, consume authoritative root usage-tree snapshots, and render current selected-root `main` and delegated-descendant `sub` token totals in the existing compact status line. Keep fork-owned accounting out of upstream SQLite schema and migration namespaces. Preserve legacy server and WSL usability by treating the fork extensions as optional, never estimating descendant usage, and retaining exact target/thread ownership and bounded resources.

# Phase 7: Isolate fork-owned usage accounting storage (finished)

Fork-owned usage accounting now uses its own configured-home database and migration ledger, with upstream-state isolation, accounting-local atomicity, root-scoped response ownership, bounded multi-runtime revision observation, initialization-outage fallback, and usage-first retryable deletion. Focused verification passed for `codex-state` (8), `codex-core` (3), and app-server (2), together with formatting and scoped diff checks; the Windows Core run used a command-scoped larger test stack after the default harness stack overflowed. Independent completion review found no blocking or non-blocking issues under the lightweight v0.1 rigor contract.

# Phase 8: Cut over the prototype shared migration safely (pending)

Import any exact pre-separation fork migration-53 accounting state into the separate database and remove only the checksum-proven fork schema and ledger entry from `state_5.sqlite`. Prove semantic preservation, interruption-safe idempotent retry, refusal of ambiguous or upstream-owned version 53, and the declared exclusion of concurrently running old modified binaries.

# Phase 9: Verify the integrated Beryl/fork boundary (pending)

Build the corrected standalone fork artifact and exercise it through Beryl's supported Host-Windows override against the active shared Codex home. Cover startup selection, initialize capability, usage-tree read/update and reconnect routing, selected-thread isolation, complete/partial states, descendant changes, vanilla-CAS coexistence, and saved-path image delivery without inline result bytes; finish the retained opt-in live smoke without starting a model turn.
