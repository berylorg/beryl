# Shared UI Contract

This document defines shared window, widget, appearance, and scrolling mechanics referenced by root and feature design docs. Feature-specific UI behavior belongs in the owning `doc/features/<feature>/design.md` file.

The terms `stretch`, `fixed`, `anchored`, `overlay`, and `scrollable` describe target runtime behavior, not a required implementation technique.

## Feature UI Authority

- Workspace picker and workspace member UI are defined in `doc/features/workspaces/design.md`.
- Backend-unavailable and connection recovery UI behavior is defined in `doc/features/backend-runtime-recovery/design.md`.
- Thread selector and thread history navigation UI are defined in `doc/features/conversation-threads/design.md`.
- User input panel UI is defined in `doc/features/composer/design.md`.
- Transcript region, media, quote popup, and transcript turn context menu UI are defined in `doc/features/transcript/design.md`.
- Status line and status popups are defined in `doc/features/status-line/design.md`.
- Activity panel UI is defined in `doc/features/activity-panel/design.md`.
- Graph overlay, graph rows, and graph context menus are defined in `doc/features/semantic-graph/design.md`.
- Settings window, settings rows, and color inputs are defined in `doc/features/settings/design.md`.
- Theme editor and theme candidate code panels are defined in `doc/features/theming/design.md`.
- Surface notices are defined in `doc/features/notifications/design.md`.
- Diagnostic child visible-control behavior is defined in `doc/features/diagnostics/design.md`.

## Global Window Rules

- The main workspace window includes a toolbar strip anchored to the top edge of the OS window.
- A top-level auxiliary window, such as the settings window, may define its own dedicated chrome and does not inherit the main workspace toolbar strip.
- When a window includes a toolbar strip, that strip stretches horizontally with the OS window and does not automatically resize vertically.
- The main OS window must not rely on outer window-content scrolling to keep primary widgets reachable during normal operation.
- Only explicitly designated child panels may own vertical scrolling.
- The minimum OS window size is derived from the minimum sizes of currently visible child widgets so pinned controls do not move off-viewport.
- The main workspace window must preserve visibility of the pinned toolbar strip, thread strip, pinned user input panel, visible activity panel, and status line strip within the OS window.

## Shared Terms

- `OS window` is the native top-level application window.
- `Toolbar strip` is a fixed-height top row reserved for global controls such as settings and workspace actions.
- `Thread strip` is the fixed-height row beneath the toolbar for thread creation, thread history navigation, and the active thread title selector.
- `Conversation column` is the workspace area beneath the thread strip that contains transcript, optional activity panel, and user input panel.
- `Transcript region` is the stretchable workspace area that shows the active Codex thread.
- `Activity panel` is the optional strip that shows selected-thread in-memory activity.
- `User input panel` is the pinned composer panel.
- `Status line strip` is the fixed bottom row for compact backend and turn status metadata.
- `Popup widget` is a bounded transient surface layered above the main workspace window without replacing it.
- `Context menu widget` is a bounded transient command surface opened from a specific row or control, with optional submenus.
- `Column selector widget` is a reusable horizontally branching column-selection widget used by domain-specific surfaces.
- `Column selector container` is the horizontally scrollable area that owns side-by-side columns.
- `Column selector column` is one fixed-width vertically scrollable explorer viewport inside a column selector widget.
- `Code panel widget` is the reusable plain-text monospace widget for code-like blocks.

## Main Workspace Shell

- The main workspace window is a pinned toolbar strip above a workspace body and a fixed status line strip anchored to the OS window bottom edge.
- The workspace body contains a thread strip above the conversation column.
- The conversation column is vertically stacked with a stretchable transcript region, optional activity panel, and pinned user input panel above the status line strip.
- Freshly created workspaces render through the same main workspace shell as initialized workspaces on a pending-new-thread draft.
- Runtime, member, and backend-availability recovery states may disable submission or show localized recovery information, but they do not replace the main workspace shell with a separate fresh-startup screen.
- The toolbar strip is fixed-height, stretches horizontally, and is controls-only.
- The toolbar does not reserve persistent static workspace-name text, thread-count text, visible graph-hotkey labels, or non-interactive status chips.
- Feature-owned toolbar controls include the workspace picker control from `doc/features/workspaces/design.md`, branch breadcrumbs from `doc/features/conversation-threads/design.md`, the graph overlay toggle from `doc/features/semantic-graph/design.md`, and the settings control from `doc/features/settings/design.md`.
- The main workspace toolbar arranges Workspaces and optional content-sized branch breadcrumbs at the leading edge, reserves flexible space in the middle, and aligns Graph and Settings controls to the trailing edge. While an asynchronous thread activation is pending, toolbar breadcrumbs keep rendering from the last selected-thread projection until the new selected thread is applied.
- The thread strip is fixed-height beneath the toolbar, stretches horizontally, and keeps long thread labels from causing outer scrolling.
- The thread strip includes feature-owned thread controls from `doc/features/conversation-threads/design.md`, including `New Thread`, backward/forward thread-navigation controls, and the active thread title selector. Branch breadcrumbs and static runtime-context labels belong outside the thread strip; the thread strip must not render `wsl-linux:<distro>` or other runtime labels before the active thread title selector.
- The conversation column itself is not a scrolling surface. Its child transcript region, activity panel, and user input panel follow their own feature contracts.
- When an asynchronous operation changes the selected thread, established selected-thread chrome and the transcript region keep rendering the previous coherent state until the replacement thread history is ready to apply. Progress feedback belongs in status, notices, or other localized affordances that do not replace existing labels or content with transient `Opening ...` or loading placeholders.
- Applying a newly selected thread is a single UI transaction: the selected-thread chrome, transcript rows, and initial transcript viewport state are chosen before the new transcript becomes visible. The transcript renderer may measure rows and reconcile ordinary live layout, but it must not install a second selected-thread-activation scroll position through prepaint, deferred, or post-frame work after the first new-thread frame.

## Appearance

- The appearance theme model, role schema, value resolution, theme repository, and theme editor are defined in `doc/features/theming/design.md`.
- Shared UI widgets consume resolved appearance values from the active theme. They must not define a second theme schema or infer unsupported role-property combinations.
- A visual constant may remain outside a named theme role only when it is not user-visible, is derived from a themed property, or is explicitly documented as fixed geometry rather than appearance.

## Button Geometry

- Beryl-owned buttons share one app-wide button geometry contract independent of primary or secondary color roles.
- Button outer height is one shared command-control height.
- Button labels use standard UI font family, shared button-label size, shared button-label line height, and active button role font weight.
- Internal padding is centralized separately for vertical and horizontal axes.
- Normal text-labeled command buttons use the shared horizontal padding exactly and remain content-sized unless a specific finite-label contract reserves width for label changes. They must not add fixed leading chrome width merely to make unrelated controls appear equal-width.
- Square or icon-like command buttons may override horizontal padding or width only as needed to preserve a square footprint.
- Text buttons and icon-only buttons share the same outer height and corner shape.
- Button containers must preserve their own outer border and label padding under bounded-width truncation; truncation may shorten label text but must not clip, mask, or hide the command button's right or bottom edge.
- Buttons whose visible text comes from a known finite cycling or toggle label set reserve width for the longest label in that set.
- Button geometry is invariant across normal, hover, pressed, active, and disabled states.
- Interaction states must not change width, height, padding, border width, font size, line height, font weight, transform, shadow, or flex sizing.
- Action rows that directly execute commands are buttons for this contract even when they appear inside popups or lists.
- Selector rows, data rows, status messages, and active title selectors are controls rather than command buttons, but clickable title-style controls in fixed chrome align to shared button height, label typography, and corner shape where applicable.
- Rounded corners for Beryl-owned buttons and other rounded widgets come from one shared corner-shape value unless a specific widget contract requires otherwise.

## Action Availability

- When an action button, action row, or action menu item is normally part of a surface for the current object, it remains visible when temporarily unavailable.
- Temporarily unavailable actions render disabled rather than disappearing, and expose a tooltip or equivalent local affordance explaining the specific reason the action is unavailable.
- The unavailable reason should name the closest actionable Beryl state or gate, such as a pending operation, missing capability, stale projection, incomplete metadata, or invalid current selection.
- Actions may be absent when the action is not part of the current object's surface, the user is not in the action's context, or the owning feature intentionally uses progressive disclosure for actions the user would not reasonably expect there.
- Disabled action controls must not execute commands through pointer, keyboard, or programmatic menu acceptance paths.

## Popup And Context Menu Widgets

- Popup widgets are bounded transient surfaces layered above the main workspace window while leaving the underlying window intact.
- Popups remain clamped within OS window bounds.
- Popups may own internal scrolling when their content exceeds their bounds.
- Context menu widgets are bounded transient command surfaces opened from specific rows or controls.
- Context menus and submenus remain clamped within OS window bounds and may own internal scrolling when row content exceeds bounded height.
- Context menu items follow the shared action availability rule: expected menu actions remain visible as disabled rows with specific unavailable-reason tooltips.
- A popup or context menu closes according to the owning feature's contract, but outside click and `Escape` are the default shared closing gestures unless overridden by a more specific active interaction.

## Column Selector Widget

- The reusable column selector owns column-trail presentation, selected-row state, caller-supplied row expansion state, and scroll affordances.
- Callers own row domain model, labels, commands, and activation semantics.
- Selecting a branching row truncates columns to its right and opens the next column from that row's target.
- Selecting a terminal row does not open a next column unless the caller defines it as branching.
- The column selector container owns horizontal scrolling when columns exceed visible width.
- Each column owns its own vertical scrolling beneath a fixed one-line header.
- Selector surfaces support `Escape` to close, `Up` and `Down` within the active column, `Left` and `Right` across available columns, and `Enter` to invoke caller-defined activation.
- Single-click selects a row and may open the next column when branching.
- Double-click invokes the selected row's caller-defined primary action when one exists.
- Only one column-selector surface is interactive at a time; opening one closes other column-selector surfaces and their context menus according to feature contracts.

## Code Panel Widget

- Code-like presentation blocks, including transcript Markdown code blocks and diagnostic command/output/patch panels, render through the shared code panel widget.
- The code panel boundary accepts plain text plus an optional language or syntax label.
- Callers route supported labels through Beryl-owned off-render syntax lookup before rendering.
- Syntax highlighting is parser-backed and source-preserving: parser output assigns token roles to source ranges and rendering maps those roles through appearance settings.
- Languages or labels without a registered parser render as plain text.
- The widget supports inline mode for unboxed fragments and bordered mode for standalone panels.
- The widget's own copy action copies bare plain text. Transcript selection that spans a Markdown code block may copy fenced Markdown source through the transcript feature.
- The widget supports smart-wrap and no-wrap modes.
- Smart-wrap prefers breaks on spaces, commas, and semicolons before forcing a split at the last fitting symbol.
- No-wrap enables horizontal scrolling instead of soft line breaks.
- The widget may expose an optional header strip with generic small actions such as `Expand`, `Collapse`, `Soft Wrap`, and `Copy`.
- In bordered mode, the widget may expose a draggable lower edge for vertical resizing within surrounding layout bounds.
- Scrollable code panels use the shared scrollbar affordance.
- A scrollable code panel nested inside transcript does not take vertical pointer-wheel ownership merely because the pointer hovers over it.
- Clicking a nested scrollable code panel selects it for vertical pointer-wheel ownership.
- While selected, vertical wheel input over that code panel scrolls only the panel and must not co-scroll the outer transcript.
- Pressing `Escape` does not deselect a nested code panel for pointer-wheel ownership.

## Scroll Ownership

- Beryl uses one shared app-wide scrollbar affordance rather than per-surface custom chrome.
- The shared scrollbar affordance is backed by reusable app-neutral scrollbar primitives that own chrome visibility and direct manipulation.
- Beryl surfaces own viewport routing and scroll-state semantics around the shared scrollbar.
- Every Beryl surface that owns scrolling must render the shared scrollbar affordance.
- The shared scrollbar renders only a thumb; the full outline or track remains visually invisible.
- The thumb appears only after pointer movement or active scrolling within the owning scrollable area and only while the surface has overflow.
- After pointer movement and scrolling stop, the thumb fades in and out around a short inactivity delay using render-frame-driven opacity interpolation.
- A scrollbar thumb is draggable by pointer press-and-hold on every rendered scrollbar axis.
- Dragging preserves the pointer grab offset within the thumb until pointer release or cancellation.
- A vertical scrollbar owns an invisible interaction lane. Clicking the lane outside the current thumb scrolls one viewport page toward the click.
- Direct scrollbar interactions route through the owning scroll surface's scroll-state callbacks.
- Shared scrollbar fade and activity timing comes from reusable scrollbar chrome. Owning surfaces report pointer movement, wheel scrolling, and other viewport activity into that chrome.
- Keyboard scrolling commands act on the scrollable viewport selected by focus or shell routing, not on scrollbar chrome.
- Pointer-wheel and touchpad scrolling act on the routed scrollable viewport, while thumb dragging and lane clicks originate from scrollbar chrome.
- Streaming scroll surfaces may opt into a bounded virtual trailing scroll allowance that increases scroll range without adding fake content.
- Virtual trailing allowance is capped by the owning viewport and caller's visual anchor so at least one real content line remains visible for orientation.
- Scrollbar geometry reflects virtual trailing allowance, but content item counts, visible ranges, and preserved anchors remain based on real content only.
- Virtual trailing allowance is provided by Beryl-owned scroll/list support layered on `gpui`, not by changing third-party `gpui` list primitives.
- Pointer movement over an overflowed scrollable surface may reveal that surface's scrollbar even when that surface does not currently own pointer-wheel scrolling.
- The toolbar strip, user input panel, activity panel, and status line strip remain pinned rather than becoming general outer scrolling surfaces.

## Small-Window Behavior

- The workspace window preserves pinned toolbar, thread strip, user input panel, visible activity panel, and status line within OS window bounds.
- Feature-owned overlays and popups remain bounded instead of pushing pinned strips or active transcript off-screen.
- The minimum OS window size for the main workspace window derives from minimum sizes of the visible toolbar strip, thread strip, conversation column, transcript region, visible activity panel, user input panel, and status line strip.
