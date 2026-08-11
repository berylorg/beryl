# Status Line GUI

This is a normative supplemental GUI composition file for `design.md`. It owns status-line slot mounts, layout relationships, and widget composition. Product behavior, metadata authority, menu gating, compaction, stop semantics, and failure reporting remain in `design.md`.

## Status Line Strip

Mount-into: main-window.status-line

The feature mounts one bundled `segmented status bar` as the fixed bottom strip below the composer. This is a feature configuration of the bundled widget rather than a Beryl-specific status widget: the built-in owns strip, segment, divider, focus, open, disabled, and truncation mechanics, while `design.md` owns values, availability, and operations.

The feature configures three stable left-to-right segment identities:

- Model and reasoning.
- Context space and rate-limit status.
- Turn state plus transcript-view turn position.

Model/reasoning and context are action-menu segments when their exact operations are available. The
turn segment is an action-menu segment whenever `design.md` makes its exact operation or
stopping-feedback menu eligible; its `View` subsegment remains passive and never becomes a
separate activation target.

Segments retain identity and geometry while displayed values change, including while an anchored
context menu is open. `Unknown`, `compacting`, `repair pending`, `repaired`, `incomplete`, `unknown
terminal`, `working`, `ok`, `error`, `interrupted`, and `View` values update inside the existing
segment geometry. Truncated exact values use the bundled `tooltip` without becoming a second status
source.

When an operation is unavailable, its segment remains passive or visibly disabled according to `design.md`. Any visible disabled command-capable segment uses `disabled-command-tooltip` with the closest exact gate; it never opens an empty menu.

## Status Operation Menus

Mount-into: main-window.overlays

Each status operation menu uses the bundled `anchored context menu`, whose row presentation follows
the bundled `context menu`. The model/reasoning menu configures the `virtualized-collection`
variant with stable option identities, logical focus, selected-row reveal, and bounded realization.
The context-operation menu configures the static bounded `Compact` command row. The turn-operation menu configures one
static bounded `Soft stop` command row and no other stop control.

Within the model/reasoning menu, the feature maps initial-query loading to a bounded noninteractive
text header row, and an exact successful zero-result query to a bounded noninteractive empty-result
text header row. Initial-query failure maps to a bounded failure text header row followed by a
`Retry` command row instead of model option rows. Later-page failure preserves the already presented
option and selection rows and adds a bounded incomplete-result text header row followed by the
`Retry` command row.

While Retry is pending, that same row remains present, visibly disabled, and uses
`disabled-command-tooltip` with the design-supplied pending explanation. Repeated failure updates
the same feedback and row; successful retry returns the same anchored menu to its progressive option
presentation. The feature supplies the exact query scope, feedback, Retry availability, and command
effect from `design.md`; the canonical menu owns row focus, activation, and bounds.

The feature supplies row labels, selected values, supported combinations, command effects,
in-flight presentation, failures, and closest disabled reasons. It consumes the CAS-live system's
opaque eligibility and feedback identities and never reconstructs exact target or operation
identity from visible values. Built-in widgets own menu bounds, row focus, dismissal,
duplicate-activation suppression, tooltip presentation, and focus return.

Anchored context menus remain anchored to the same stable segment instance while its displayed value changes and
remain within the OS window bounds. When `design.md` makes a menu ineligible, the composition
removes it through the bundled widget's dismissal and focus-return mechanics.

## Stop-Feedback Notice Contribution

This feature mounts no `main-window notice`. For the fallback state owned by `design.md`, it supplies
the Notifications arbiter with one owner-configured record containing stable exact-operation
identity, bounded title and detail, severity and dismissal variants, and no independent overlay
geometry. Notifications composes that record into its sole visible notice instance.
