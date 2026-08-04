# Name

Canonical name: thread-root picker

Sometimes known as: unified thread management flyout, thread switcher flyout, runtime/root picker

# Purpose

Presents one reusable anchored flyout shell for searching and selecting Beryl threads or roots while keeping runtime/root creation controls, collection scope, focus, and bounded scrolling stable across modes.

# References

Contracts:

- expected-action-availability

Widgets:

- command button
- single-line text field
- scrollbar
- tooltip

# Anatomy

The thread-root picker consists of:

- An anchored flyout frame with a fixed outer footprint.
- A header containing a title, optional helper text, and an optional return command.
- A search-variant single-line text field with a leading search icon.
- A plain-text collection heading that names the current collection and scope.
- A primary collection viewport with an external scrollbar.
- Fixed-height collection rows with a leading icon region, primary label, secondary label, and optional trailing status or command region.
- A runtime/root section heading.
- A bounded runtime viewport with an external scrollbar when needed.
- Fixed-height runtime rows with a runtime label, compact metadata, and owner-supplied commands.
- An Add runtime command below the runtime viewport.
- An optional footer containing the owning feature's confirmation command.

The flyout frame, header, search field, runtime/root section, and optional footer retain stable identity while the primary collection changes. Replacing the primary collection does not replace or resize the flyout frame.

The widget owns collection presentation and interaction mechanics. The owning feature supplies a
revision-bound collection identity, total row count, bounded resident row pages, stable row
identities, labels, icons, status text, commands, selection semantics, query outcomes, and commit
effects. The widget never receives or retains a complete caller-unbounded collection.

# Look

The widget reads as one compact anchored flyout with a clear vertical hierarchy: header, search, primary collection, runtime/root controls, and optional confirmation footer.

Collection scope is part of the plain section heading. Scope is not repeated as a chip, pill, badge, rounded surface, or row decoration.

Collection and runtime rows use consistent inset surfaces. Hover, focus, selected, current, pending, and unavailable treatments preserve row geometry. Selection uses the full-row selected treatment rather than a checkmark.

Primary and secondary row labels have distinct emphasis. Leading icons distinguish owner-supplied row kinds, while trailing status remains aligned independently of label length.

The active theme may replace all color, typography, border, shadow, and metric fallbacks through the widget's UI roles.

# States

The widget supports closed, open, clamped, loading, ready, searching, empty, collection-transition, no-selection, selection-pending, commit-pending, and commit-failed states.

Collection rows support normal, hover, focused, selected, current, unavailable, and activation-pending states. Runtime rows additionally support active-scope and readonly-scope states.

Loading preserves the complete flyout anatomy and replaces only the unavailable collection or registry body with its bounded loading presentation. Empty search results remain inside the primary collection viewport.

Selection and keyboard focus are separate states. Focus movement never implies selection or activation.

Unavailable and pending commands remain visible, disabled, and explanatory according to `expected-action-availability`.

# Interaction

Opening the picker establishes one owning trigger. Dismissal returns focus to that exact trigger unless the owning feature successfully activates another window-level target.

The owning feature chooses whether initial focus enters the search field or a collection row according to the invocation path. The widget then owns focus traversal among its visible header command, search field, collection rows and row commands, runtime rows and commands, Add runtime command, and optional footer command.

Up and Down move focus between enabled rows in the currently focused collection. Home and End move to the first and last eligible row. Page Up and Page Down move by one visible collection page while preserving a valid focused row. Enter activates the focused row or focused command; Space activates focused command buttons.

Tab and Shift+Tab move through the flyout's focusable regions without moving collection selection. Pointer activation targets the exact row or command under the pointer.

Escape and outside-pointer dismissal close the flyout without implicitly committing a pending selection. A feature-owned platform dialog invoked from a picker command owns Escape while that native dialog is open; cancellation returns to the unchanged picker and invoking command.

The primary collection may use immediate activation or pending selection. In immediate activation, row activation invokes the owner-supplied exact target command. In pending selection, row activation changes the selected-row state and leaves commit to the footer command.

The footer command remains disabled with an explanation when no valid selection exists. While commit is pending, it remains visible and disabled, and duplicate pointer, keyboard, or programmatic acceptance cannot start another commit.

Search text requests an owner-supplied revision-bound query for the current collection and never
filters by scanning all rows inside the widget. Changing the collection, query, or scope uses a
stable collection key and query revision so the widget can preserve or intentionally reset search,
focus, selection, and scroll state according to the owning feature's declared configuration.

Primary and runtime collections use fixed-height virtualization. Each viewport stores total row count separately and realizes only visible rows plus at most four overscan rows before and four after the visible range.

Navigation into a nonresident range requests the bounded page containing the intended stable row and
preserves the last coherent focus and scroll state while that page is pending. Home, End, Page Up,
Page Down, selected-row reveal, and scrollbar movement do not cause eager construction of
intervening rows.

Every row has an owner-supplied stable identity independent of visible index. Focus, selection, current state, command dispatch, and selected-row reveal follow that identity across filtering, viewport entry, and viewport exit.

Each stable collection key owns its scroll position and focused-row identity. Returning to a previously visited collection restores those facts when the rows still exist; otherwise the widget resolves to the nearest valid owner-supplied initial row without activating it.

A tooltip stays anchored while its owning row remains realized. When virtualization removes that row, the widget closes the tooltip intentionally instead of retaining an offscreen row solely as an anchor.

Row hover, focus, selection, current, pending, and unavailable changes never alter fixed row height or total scroll geometry.

Content-free diagnostics expose widget instance id, collection key, query revision, total row count,
resident page count, pending page count, realized row count, visible range, overscan count,
fixed row-height variant, scroll offset, focused stable row id, selected stable row id, and tooltip
anchor presence. Diagnostics never include titles, paths, search text, labels, or tooltip content.

# Layout

The flyout is anchored below or near its owning trigger, remains inside the owning window overlay, and flips or shifts when needed without losing trigger association.

The outer footprint is fixed and non-user-resizable. Every feature variant uses the same outer width and height; optional footer presence reallocates bounded internal collection height instead of stretching the flyout.

The header, search field, headings, collection viewport, runtime/root section, Add runtime command, and optional footer form one vertical stack. The primary collection receives the stretchable bounded middle allocation. Header, search, section headings, Add runtime, and footer remain reachable without scrolling the entire flyout.

Runtime rows always form one vertical list. They never reflow into horizontal columns when unused inline space is available.

The primary collection and runtime registry own independent vertical scroll state and independent external scrollbar widgets. The flyout itself does not own a third vertical scroll surface and never scrolls horizontally.

Collection rows remain single-height. Primary and secondary labels truncate within their allocated region; trailing status or commands retain their trailing alignment. The owning feature supplies complete accessibility text and tooltips for truncated values.

The window minimum-size contract must accommodate the fixed picker footprint. Clamping changes placement, not the widget's internal geometry.

# Variants

Immediate selection activates an owner-supplied row command without a footer confirmation step.

Confirmed selection keeps a pending selected row and exposes the footer confirmation region.

The primary collection may be configured with thread-row or root-row presentation. Runtime rows remain available in either selection variant.

Default variant: immediate selection with thread-row presentation.

# UI Roles

```css
.thread-root-picker {
  --width: 732px;
  --height: 616px;
  --padding-x: 26px;
  --padding-y: 18px;
  --gap: 12px;
  --radius: 10px;
  --border-width: 1px;
  --background: #101827;
  --foreground: #f3f7fb;
  --border-color: #3a4860;
  --shadow: 0 14px 36px rgba(0, 0, 0, 0.55);
}

.thread-root-picker__header {
  --height: 64px;
  --gap: 4px;
}

.thread-root-picker__title {
  --foreground: #f3f7fb;
  --font-size: 19px;
  --font-weight: 650;
}

.thread-root-picker__helper {
  --foreground: #92a2b7;
  --font-size: 12px;
  --line-height: 16px;
}

.thread-root-picker__collection-heading,
.thread-root-picker__runtime-heading {
  --height: 16px;
  --foreground: #7f8ea3;
  --font-size: 11px;
  --font-weight: 700;
  --letter-spacing: 1.1px;
}

.thread-root-picker__collection-viewport {
  --height: 228px;
  --row-gap: 6px;
}

.thread-root-picker__collection-viewport[data-variant~="confirmed-selection"] {
  --height: 202px;
}

.thread-root-picker__row {
  --height: 48px;
  --padding-x: 14px;
  --gap: 10px;
  --radius: 7px;
  --border-width: 1px;
  --background: #111827;
  --foreground: #e9eef5;
  --border-color: #2f3b52;
}

.thread-root-picker__row[data-variant~="thread"] {
  --height: 52px;
}

.thread-root-picker__row[data-state~="hover"] {
  --background: #141d2d;
  --border-color: #475569;
}

.thread-root-picker__row[data-state~="focused"] {
  --ring-width: 2px;
  --ring-color: #38bdf8;
  --ring-offset: 0px;
}

.thread-root-picker__row[data-state~="selected"] {
  --background: #173a5e;
  --foreground: #f3f7fb;
  --border-color: #38bdf8;
}

.thread-root-picker__row[data-state~="current"] {
  --background: #131b29;
  --foreground: #f3f7fb;
  --border-color: #3b475d;
}

.thread-root-picker__row[data-state~="unavailable"] {
  --background: #111827;
  --foreground: #7f8ea3;
  --border-color: #2b3547;
  --opacity: 0.72;
}

.thread-root-picker__row-icon {
  --size: 18px;
  --foreground: #7dd3fc;
}

.thread-root-picker__row-primary {
  --foreground: #e9eef5;
  --font-size: 14px;
  --font-weight: 520;
}

.thread-root-picker__row-secondary {
  --foreground: #92a2b7;
  --font-size: 12px;
  --font-weight: 400;
}

.thread-root-picker__row-status {
  --foreground: #7dd3fc;
  --font-size: 10px;
  --font-weight: 650;
}

.thread-root-picker__runtime-viewport {
  --height: 102px;
  --row-gap: 6px;
}

.thread-root-picker__runtime-row {
  --height: 48px;
  --padding-x: 14px;
  --gap: 10px;
  --radius: 7px;
  --border-width: 1px;
  --background: #111827;
  --foreground: #e9eef5;
  --border-color: #2f3b52;
}

.thread-root-picker__runtime-row[data-state~="active-scope"] {
  --background: #173a5e;
  --foreground: #f3f7fb;
  --border-color: #38bdf8;
}

.thread-root-picker__runtime-label {
  --foreground: #e9eef5;
  --font-size: 13px;
  --font-weight: 600;
}

.thread-root-picker__runtime-metadata {
  --foreground: #92a2b7;
  --font-size: 11px;
  --font-weight: 400;
}

.thread-root-picker__footer {
  --height: 60px;
  --padding-top: 12px;
  --divider-width: 1px;
  --divider-color: #2a3549;
}

.thread-root-picker[data-state~="loading"] {
  --opacity: 0.72;
}

.thread-root-picker[data-state~="commit-pending"] {
  --opacity: 0.88;
}
```
