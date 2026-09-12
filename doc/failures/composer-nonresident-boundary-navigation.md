# Composer Nonresident Boundary Navigation

## Invalidated Assumption

A published Ctrl+End caret plus an idle interactive surface is enough for subsequent Shift+Left
to resolve and publish a nonresident preceding character in the bounded mounted composer.

## Evidence

After the stack corrections, the ordinary-development mounted scale witness completes its
3,145,728-byte draft setup and reaches navigation. Ctrl+End publishes a collapsed selection at
EOF; Shift+Left leaves it unchanged after 256 draw/drain rounds. Runs
`4af30ae3-88f4-49c0-a5df-fc11f9d338dd`, `d44efb39-7165-4e4f-adc5-f469de477c97` and
`a821872d-443b-4ce4-8dd4-4eb568d498b1` reproduce the selection failure without a stack overflow.

The final pre-key capture reports enabled, current-and-interactive, semantically quiescent and
fully quiescent all true. The surface has zero text/layout fragments, zero realized object gaps,
and no position mapping for byte 3,145,727. Its platform selection is nevertheless at EOF.
The fixture intentionally exercises a 64-pixel realization bound inside a 640-pixel viewport;
capacity is `ViewportExceedsRenderingCapacity` with one filler. Before and after the key there
are no active geometry jobs, pending pages, target intents or queued requests. The key produces
one response rejection classified as `Busy`; no owner error or continuation remains.

The diagnostic log is
`C:/Users/user/AppData/Local/Temp/beryl-build-memory-20260908/transfer-scale-boundary-realization-20260912.output.log`.
The test is `mounted_composer_scale::mounted_multi_mib_activation_retarget_edit_history_autosave_and_disposal_are_bounded`.

## Failure Mechanism

In the owned `gpui-text-input` fork, `record_response_rejection` maps both actual `Busy` and
`RangeTextInputError::Pending` to the `Busy` diagnostic class. `deliver_segmentation_page` consumes
the completed segmentation action before `apply_boundary` maps its offset through the current
surface's `source_position_for_byte`. That mapping uses realized fragments and object gaps.
An unavailable position returns `Pending` without retaining the navigation action or queuing
target realization. The confirmed empty mapping and subsequent quiescent rejection match this
source path; the internal completed-boundary offset itself was not instrumented.

The related EOF realization path also needs examination: a surface reported as interactive with
a logical EOF caret has no fragments for the preceding text. Additional waiting or a pre-key
quiescence gate cannot supply progress when nothing is queued. Independent read-only diagnosis
found no justification for relaxing the filler-capacity fixture or repeating the key.

## Required Follow-up

Preserve the [composer's nonresident navigation contract](../features/composer/design.md) and
shared [keyboard semantics](../input-hotkeys.md). Investigate EOF priority realization and retain
exact bounded navigation intent until its resolved position can be published. Preserve identity,
selection anchoring, cancellation, supersession and existing residency bounds; do not flatten the
draft or expand capacity to conceal the missing progress.

The Operator authorized widget correction, now tracked in phases 389 and 390. Phase 388 and
submission-handoff acceptance remain blocked; the later scale assertions have not been exercised
successfully.

## Rejected Unconditional Anchor Retention

The first proposed correction retained any fragment mapping a pending source anchor, regardless of
the requested output window, and resolved the anchor only from retained fragments. Source review
invalidated that generalization before compilation. Exact geometry also uses anchors to guide
scanning while preserving a different output window. The existing regressions
`anchors_before_and_after_nonzero_target_preserve_requested_output_window` and
`earlier_anchor_retains_only_leading_object_context_and_requested_visual_line` explicitly exercise
that distinction. Unconditional retention changes their output and could retain offscreen objects,
contrary to the fork's bounded realization contract. Restricting anchor resolution to retained
output likewise requires distinguishing these scan-only anchors first.

The trial source and unfinished regression edits were removed; the fork has no tracked diff.
No build or test was run for that proposal. AGENTS.md requires stopping when a planned approach
proves invalid. The recommended next direction is a narrow EOF/checkpoint-boundary correction
that preserves scan-only anchors and uses exact mapped geometry for widget filler placement.
Do not remove the existing window-preservation assertions to accommodate the rejected approach.

## Narrow EOF Correction Verification

The authorized correction skips terminal aggregate checkpoints for widget priority realization,
then uses mapped EOF caret geometry to reconcile the realized interval and scroll position.
The index-publication Select All successor also starts from its exact target predecessor.
Scan-only anchor retention and requested output-window rules remain unchanged.

Five focused EOF cases cover ordinary and trailing-newline text, sparse checkpoints, and Select
All while an index response is pending. All five and the existing post-validation Select All
restoration regression pass in run `d3571c0f-a0ab-42a6-a975-dac12570bf59`. Independent semantic
review found no blocker in publication, identity, restoration or capacity handling. The broader
selected geometry/widget run passes 162 cases after excluding three failures also reproduced on
the unchanged fork in baseline run `15e3fd34-4b21-4897-9c95-fc63e69cfe01`:

- `active_interaction_and_scroll_anchor_are_runtime_realization_targets`: scroll-anchor priority
  differs from the expected active-interaction priority.
- `exact_priority_after_end_object_retains_proof_for_successive_edit`: resident object count is
  four rather than one.
- `shared_large_object_presentation_is_charged_once_through_publication`: accounted bytes are
  902,984 rather than 902,104.

These baseline failures remain unresolved; the selected passing run is not a full-suite pass.
An initial regression fixture also exposed a separate pre-existing Select All failure during
the first local target response, before any index is available. The accepted pending-index test
uses the actual index-response seam. The initial-local-target case remains deferred and is not
claimed fixed by this EOF correction.

The first full mounted scale run after the widget correction overflowed
the ordinary stack in draft preparation beneath the large test entry frame. Moving unchanged
setup into a separate out-of-line fixture stage preserves its order, resource custody and
assertions; independent review found no semantic change. The resulting ordinary-stack run
`573b9103-43e8-4989-bd9a-b87759bad610` reached the existing 1,200-second monitor limit without a
test result. The monitor reaped all owned processes. Noninvasive live captures found active
candidate-page authentication and, later, exact geometry scanning; neither identifies the last
completed assertion. This is not a passing scale run or proof of one specific runtime bottleneck.

Final locked production compilation passes after all EOF changes. A separate mounted EOF test
now reuses the same setup and all pre-EOF assertions, returning at the exact caret/filler check.
The full workflow entry still runs every original assertion. Operation diagnostics distinguish
future progress from mere elapsed time. The focused mounted EOF test passes in 74.7 seconds in
run `bfaf04cf-b9f6-4def-a54c-6eeaa35536ae`, with unchanged 3 MiB setup, exact source mapping,
visible caret and filler exclusion. Independent review confirms ordinary GPUI teardown and
unchanged full-workflow coverage. Later editing, history, autosave and disposal acceptance remains
separate and unconfirmed.

The accepted fork revision is `9759488930b87ed5ee96da54f002a3c71c06e1da`. Beryl's manifest and
canonical lockfile pin it. Locked metadata and the focused production check pass after the pin;
the language server was restarted successfully under the repository Cargo-model rule. The
mounted fixture remains with the pending submission/scale test changes, while the independently
verified widget correction and dependency pin form this accepted boundary.
