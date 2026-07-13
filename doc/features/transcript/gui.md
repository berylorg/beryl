# Transcript GUI

This is a normative supplemental GUI composition file for `design.md`. It owns transcript feature slot mounts and feature-specific widget configuration. Product behavior, narrative inclusion rules, provenance requirements, selection authority, and failure states remain in `design.md`; reusable viewport, embedded-panel, marker, preview, and menu mechanics remain in their canonical widget specs.

## Transcript Region

Mount-into: main-window.transcript-region

The transcript feature mounts one project-local `transcript view` in this slot. It fills the stretchable conversation-body space below the toolbar and any visible thread-lineage strip and above the lower conversation panels.

The feature configures the widget's interactive-narrative variant over the transcript host's resident presentation snapshot. It supplies which records are narrative content, their Syndic provenance, their command eligibility, and the user-visible meaning of loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback presentation states.

The transcript view also composes applicable records from `transcript.context-records` into the host-supplied ordered presentation flow. A synthetic-context contribution receives the transcript's ordinary variable-height realization, chunking, measurement, selection, accessibility, and anchor mechanics without becoming another transcript viewport or turn record.

The feature selects tail-oriented placement unless an explicit navigation command provides another target. During selected-thread activation, it keeps the prior coherent `transcript view` visible in activation-pending state until the replacement content and initial viewport state can publish together.

Large code blocks configure project-local `code panel` widgets. Large tables configure project-local `table panel` widgets. Transcript image references use the readonly-inline variant of the project-local `image marker`. The feature supplies source ranges, resource state, copy behavior, media labels, fallbacks, and command payloads; the canonical widgets own their reusable rendering and interaction mechanics.

## Transcript Menus And Previews

Mount-into: main-window.overlays

Transcript context menus are anchored through the `transcript view` to rendered content with stable geometry and provenance. They use the built-in `context menu`, `anchored context menu`, `tooltip`, and `disabled-command-tooltip` contracts.

The feature supplies Quote and `Discuss in new branch` as distinct command rows for eligible assistant reply selections. Disabled branch discussion remains visible when a close actionable gate can be explained and uses the disabled tooltip contract; selections without stable source provenance expose no branch command.

Transcript image-marker inspection opens the project-local `image preview` in `main-window.overlays`. The feature supplies the exact marker target, durable resource outcome, accessibility label, origin anchor, and local unavailable or failure meaning. The preview widget owns its fitted popup and dismissal. On close it returns focus to the exact eligible marker; if that marker is no longer realized, the feature supplies the active transcript view at its current semantic anchor, or the active thread selector trigger when the replacement transcript is still inert.

## Turn Context Menu

Mount-into: main-window.overlays

An exact historical user-input turn opens a bounded built-in `context menu` containing `Edit message`. The command uses the ordinary context-menu row treatment and the disabled-command-tooltip contract when an actionable replacement-edit gate can be explained.

Rows without exact stable Syndic provenance omit the command. Entering edit mode closes the menu; the transcript feature supplies the product-defined path dimming as presentation state on affected records without altering canonical transcript-view anatomy or adding an edit banner, placeholder row, or separate edit toolbar.
