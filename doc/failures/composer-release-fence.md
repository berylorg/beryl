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

Phase 296 must restore release progress through the existing exact owner/dispatch lifecycle while
preserving active-flight and semantic settlement. Its acceptance includes the unchanged sparse-index
assertion, exact successor identity, predecessor release, and late-response safety, followed by the
affected aggregate and independent review before Phase 290. Phase 295's initial-candidate custody
boundary was independently accepted; this failure is recorded separately and is not waived.
