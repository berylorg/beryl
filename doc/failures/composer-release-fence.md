# Composer Release-Fence Progress

Phase 295 verification exposed an existing failure in
`phase186_pending_composer_activation::current_predecessor_release_does_not_wait_for_its_remaining_sparse_index`.
The successor becomes ready, but ordinary stepping remains at `WidgetFencePending`. Draining
scheduled work allows admission only after the predecessor becomes fully quiescent, which fails
the original assertion requiring admission while its sparse index is still estimated.

## Evidence And Invalidated Assumption

`conversation_composer_owner/lifecycle.rs::widget_release_ready` requires no active dispatch flight
and semantic quiescence. It does not explicitly require full widget quiescence. However,
`conversation_composer_owner/dispatch.rs::pump_one` keeps taking nonsemantic requests during
Fencing until the widget is fully quiescent, and completion schedules another pump. Continuing
index/page work can therefore reoccupy the flight slot before release observes its cut. Independent
review confirmed both production files have no working-tree changes from Phase 295.

The [initial-custody aggregate](initial-composer-custody.md) passed 71/73, with this case and an
obsolete pending-editor geometry expectation failing. Correcting the geometry expectation passed
its targeted case. Giving `StableMountRoot` its intended 64 px height removed invalid zero-height
allocation but did not resolve release progress. Increasing admission iterations did not establish
the required behavior and was discarded. The original 16-iteration bound and immediate
nonquiescent/estimated assertions remain.

Diagnostic run `351bb866-eef4-470b-a0f2-321b20374f74` completed in 10.743 seconds with one failure,
exit 100: executor draining admitted the successor only after the predecessor was quiescent.
That diagnostic scheduling experiment was removed. No release-fence production fix was attempted.

## Correction Boundary

The external text-input unmount contract cancels cancellable page, segmentation, clipboard, and
geometry jobs while preserving exact admitted-operation settlement and rejecting obsolete results.
Beryl's [composer GUI](../features/composer/gui.md) adopts that contract. The exact
before-index-completion assertion is retained executable regression evidence; restoration-seed
quiescence is a separate contract and must not be applied to ordinary release.

## Accepted Correction And Verification

Phase 296 distinguishes ordinary release from native-lineage restoration fencing. Ordinary pumping
stops admitting realization requests at semantic quiescence; exact active-flight settlement and
widget disposal cleanup remain required. Native-lineage pumping retains full quiescence for its
compact seed. Construction, ordinary fence entry, and resume reset the fence purpose. Independent
review accepted the ownership and readiness paths without weakening the original release assertion.

The original sparse-index test passed with its original 16-advance limit and immediate
nonquiescent/estimated predecessor assertions. New real-mounted regressions verify restoration
abort followed by ordinary release, and exact admitted-edit/active-flight settlement before
readiness and coherent resume. Existing pending-flight tests retain cancellation and late-result
cleanup coverage.

`cargo check -p beryl-app --lib --features test-faults --locked` passed. The final aggregate was:

```text
cargo nextest run -p beryl-app --test phase295_initial_composer --test phase289_main_window_shell --test phase285_selected_composer_preparation --test phase238_window_abandonment --test phase236_window_acquisition --test phase141_syndic_composer_host --test phase177_main_window_composer_slot --test phase186_pending_composer_activation --features test-faults --locked --test-threads 1 --no-fail-fast --status-level pass
```

Run `8fb0e267-3d02-41e1-9a0b-f586a8512ea1` passed 75/75, zero skipped, in 129.266 seconds, exit 0.
An earlier compilation attempt reported inconsistent crate artifacts; one ordinary Cargo check
and aggregate retry recovered without cleaning shared artifacts or changing dependencies.

Supplemental run `b7dfe9b4-5529-4cb3-b40c-5adeb1cd121d` passed seven of eleven cases: both new
regressions and five native-lineage checks. Two Phase 179 clipboard cases failed before GPUI because
their Phase 166 fixture stages marker effects without the accepted writer-admission requirement;
they provide no release-path evidence and remain with the separately tracked marker work. Independent
review accepted the new Phase 186 admitted-flight and existing late-flight cases as the relevant
Phase 296 evidence instead.

The other two supplemental cases exposed a distinct existing native-lineage suspension-publication
failure. The [publication record](native-lineage-seed-publication.md) retains exact evidence and
Phase 297 before New Window integration. They are not claimed as passing. That acquisition/rollback
sequence is unchanged by Phase 296, whose native-lineage purpose preserves prior full-quiescence
behavior. Final formatting and scoped whitespace checks passed; no temporary production traces remain.
