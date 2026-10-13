# Captured Branch Worker Lifetime

Mounted branch creation qualification on 2026-10-13 invalidated the assumption that retaining a
captured request in a window task was enough to protect its asynchronous workflow. Independent
review of `main_window/shell/host/branch_discussion.rs` found that an untracked background job could
outlive view drainage and retain the retired Home reader. Changing a menu label did not preserve
selection against pointer input, and retaining a terminal operation as a global command gate
prevented unrelated healthy captures. Writer-side source rejection also did not satisfy visible
eligibility for nonterminal assistant text.

The workflow must use the existing view worker lifetime, generation fences and source retirement;
retain exact captured bytes and operation outcome evidence separately from disposable reader
access; gate selection interaction while classification is pending; and close only the exact
failed request scope. Readonly off-GUI source authentication qualifies the visible menu before
creation. Durable creation and ordinary activation remain separate elections, and activation Retry
retains the original created discussion identity.

Tracking ownership still does not admit capacity. `WorkerLifetime::track` alone increments a
lifetime count: an otherwise permitted seven retained resources plus a two-resource Running poll
already fills the nine-worker boundary. Branch qualification, creation and reconciliation must
observe the existing admission bound before spawning. Dropping a superseded GUI task cannot be
treated as proof that its synchronous worker has stopped.

Source eligibility cannot equate `TurnLifecycle::is_proven_terminal` with branch eligibility:
that general lifecycle predicate includes `Incomplete`. A source item finalized before incomplete
convergence still violates the transcript feature's explicit incomplete-output exclusion.
Preparation and writer authentication must enforce that narrower boundary together. Native live
source fixtures must also append their active turn after bootstrap; startup may legitimately settle
a preexisting orphan before the menu is exercised.

Nested Code input requires the same pending gate as narrative input. After Escape dismisses a
busy menu and restores Code focus, a nested PageDown or End handler can consume the event before
the outer transcript gate. Its own key and wheel handlers must preserve the captured viewport
while classification is pending. Copy and menu reopening retain their existing permissions.
Code menu eligibility must also qualify the current measured layout rather than cached prepared
fragments after a viewport change. Exact captured bytes alone do not establish current geometry.

Collision qualification originally required successful ordinary activation Retry while asserting
that the original collision-closed Home reconciliation scope remained installed. Those assertions
conflict with the existing [coherent election contract](../../crates/beryl-home-store/doc/design-atomic-commands.md#coherent-in-memory-election):
ordinary activation requires every scope vacant, and structural `Healthy` alone does not establish
that condition. The unrelated capture can still create its exact durable catalog-discoverable
discussion. Its activation may fail coherently; native Retry must preserve that child and source
selection without repeating creation. Immediate success under that retained scope is not promised
by Retry. Do not bypass or weaken Home election to satisfy an unsupported test expectation.

These are implementation corrections under the [branch workflow](../features/branch-discussions/design.md#discuss-in-new-branch),
[adapter boundary](../../crates/beryl-app/doc/design-feature-adapters.md#adapter-rule) and
[shell lifecycle](../../crates/beryl-app/doc/design-shell-lifecycle.md), not a new worker registry
or product decision. Native failure/Retry, collision, retirement and capacity qualification now
pass together in the [mounted acceptance](../audits/transcript-mounting-readiness.md#mounted-captured-branch-discussion-creation-acceptance).
