# Transcript GUI

This is a normative supplemental GUI composition file for `design.md`. It owns transcript feature slot mounts and feature-specific widget configuration. Product behavior, narrative inclusion rules, provenance requirements, selection authority, and failure states remain in `design.md`; reusable viewport, embedded-panel, marker, preview, and menu mechanics remain in their canonical widget specs.

## Transcript Region

Mount-into: main-window.transcript-region

The transcript feature mounts one project-local `transcript view` in this slot. It fills the stretchable conversation-body space below the toolbar and any visible thread-lineage strip and above the lower conversation panels.

The feature configures the widget's interactive-narrative variant with the presented narrative
records, their stable provenance, their command availability, and the user-visible meaning of
loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback states.

For an active assistant record, newly arrived text extends the existing record without adding a
second row, overlay, or progress surface. The record presents that text on the next available GUI
frame without a typewriter animation and remains visually continuous as it becomes historical.

Every affected assistant record configures the `transcript view`'s optional noninteractive
record-status/provenance part with one feature-supplied visible and accessible label: `Repair
pending`, `Repaired from CAS history`, or `Incomplete`. The part remains adjacent to record
provenance and is included in the record's accessible description. It is not another row, overlay,
progress surface, or interactive control.

The configured part and repaired record content update together in the whole-turn replacement. The
part never advances ahead of the records whose provenance it describes.

The transcript view also composes applicable records from `transcript.context-records` into the host-supplied ordered presentation flow. A synthetic-context contribution receives the transcript's ordinary variable-height realization, chunking, measurement, selection, accessibility, and anchor mechanics without becoming another transcript viewport or turn record.

The feature selects tail-oriented placement unless an explicit navigation command provides another target. During selected-thread activation, it keeps the prior coherent `transcript view` visible in activation-pending state until the replacement content and initial viewport state can publish together.

Large code blocks configure project-local `code panel` widgets. Large tables configure project-local `table panel` widgets. Transcript image references use the readonly-inline variant of the project-local `image marker`. The transcript feature supplies source ranges, presented resource state, media labels, and fallbacks; `doc/features/image-assets/design.md` owns image command eligibility and outcomes, and the canonical widgets own their reusable rendering and interaction mechanics.

For range-backed code and table resources, the feature configures clipboard-limited Copy and
streaming `Save…` commands over the same exact source identity; it never derives either command from
painted or currently realized content.

## Transcript Menus And Previews

Mount-into: main-window.overlays

Transcript context menus are anchored through the `transcript view` to rendered content with stable geometry and provenance. They use the built-in `context menu`, `anchored context menu`, `tooltip`, and `disabled-command-tooltip` contracts.

The feature supplies Quote and `Discuss in new branch` as distinct command rows for eligible assistant reply selections. Disabled branch discussion remains visible when a close actionable gate can be explained and uses the disabled tooltip contract; selections without stable source provenance expose no branch command.

The transcript image-marker contextual surface places Copy and `Save…` as command rows in a
bounded built-in `context menu` opened by a context-menu gesture on that marker. While its image
preview is open, the preview's optional contextual-command anchor is the corresponding preview
surface: activating that anchor opens a bounded built-in `anchored context menu` containing `Copy`
followed by `Save…` as command rows. The menu remains attached to the preview anchor and separate
from the preview's close command.

These placements are a feature-local composition of canonical widgets and introduce no new menu or
image-control contract.

Transcript image-marker inspection opens the project-local `image preview` in
`main-window.overlays`. The transcript feature supplies the exact marker target, presented resource
state, accessibility label, origin anchor, and the relationship between the marker and preview
command placements. `doc/features/image-assets/design.md` owns Copy and `Save…` eligibility,
disabled and failure behavior, command outcomes, and generic focus-return semantics, including
origin eligibility. The preview widget owns its fitted overlay and dismissal.

The transcript composition supplies the stable fallback required for focus return from image
inspection, Copy, the `Save…` file picker, and either contextual command surface. When the exact
origin or preview contextual-command anchor no longer exists, focus returns to the exact eligible
realized record in the active `transcript view` at its current semantic anchor. If no such record
exists, the terminal fallback is the active `thread selector trigger`. Image inspection and its
contextual command surfaces are unavailable when that trigger cannot remain eligible for the
inspection lifetime.

## Turn Context Menu

Mount-into: main-window.overlays

An exact historical user-input turn opens a bounded built-in `context menu` containing `Edit message`. The command uses the ordinary context-menu row treatment and the disabled-command-tooltip contract when an actionable replacement-edit gate can be explained.

Rows without exact stable Syndic provenance omit the command. Entering edit mode closes the menu; the transcript feature supplies the product-defined path dimming as presentation state on affected records without altering canonical transcript-view anatomy or adding an edit banner, placeholder row, or separate edit toolbar.
