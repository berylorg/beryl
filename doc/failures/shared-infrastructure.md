# Shared Infrastructure Accounting Scope Correction

## Invalidated Approach

On 2026-09-29, the main agent added root phase 843 / widget phase 99 to inventory immutable
prepublication environments and shared cleanup infrastructure. Review found omissions in that
inventory, and the agent incorrectly escalated them as a technical blocker and proposed more
allocation-identity machinery.

The Operator pointed out that the existing [bounded-resource authority](../systems/bounded-resource-dataflow/design.md)
already excludes exact accounting of ordinary structs, control records, handles and dependency
bookkeeping. The intended risk boundary is growing content, working sets, queues, caches and
retained work. This was an authority-application failure, not a new requirement or a technical
blocker in the intended design. Reviews must establish that a finding violates the applicable
scope before requesting more machinery.

## Correction

The unaccepted environment/ledger measurement API and its four tests were removed before any
commit. Phases 843/99 were abandoned. No allocation registry or exhaustive UI configuration
inventory is required. Existing fixed settlement/cleanup slot limits and explicit release remain
required, as does the host's one-flight-per-resident lifecycle.

Root 842 / widget 98 instead use the existing resident working-set measurement and ordinary
session ceiling: subtract the protected predecessor from the supplied combined capacity, clamp
successor capacity to configured limits, and bind the resulting reservation to the exact
protection cut and session generation. Old paint remains retained on refusal and cancellation.
Final adoption and host flight integration remain separate acceptance boundaries.

## Verification

The corrected implementation passed all 55 prepublication integration tests in nextest run
`d7b68309-bc9f-4d9c-91fb-26a4779a9fa1`, plus default-feature compilation. Tests cover exact initial
fit, byte/item shortages, fixed ceilings, changed positions, foreign/invalidated/stale cuts,
configured ceilings, completed/cancelled successor cleanup and unchanged predecessor ownership,
paint and focus. Independent lifecycle/resource review found no blocking production issue;
its test-call correction was applied before the final successful run.

Commands were `cargo nextest run --locked --features test-support --test prepublication
--target-dir .codex-phase255-target --test-threads 2 --no-fail-fast --status-level fail
--final-status-level fail` and `cargo check --locked --test prepublication
--target-dir .codex-phase255-target`, with one compiler job, LLVM linking, no debug information
and no incremental compilation. Existing shared build-cache state and unrelated work were preserved.
