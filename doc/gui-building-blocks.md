# GUI Building Blocks

This document defines reusable GUI vocabulary for clickable and informational interface pieces. The terms are app-neutral unless an example explicitly names Beryl.

## Core Distinctions

Use **control** as the broad term for an interactive interface element.

Use **command** for the abstract operation a user can invoke, independent of presentation. For example, `New Thread`, `Compact`, `Copy image`, and `Cancel` are commands.

Use **button** only for the button visual/control family, not for every clickable element. A clickable row, strip segment, title surface, or menu item is not a button merely because it can be activated.

Describe an interactive element by both:

- its visual family, such as button, row, segment, or title control
- its behavior, such as direct action, selector, flyout trigger, or action-menu trigger

## Surfaces and Panels

A **surface** is a bounded visual plane that carries content. It may have its own background color, border, shadow, or other visual separation from the area behind it. Surface describes visual treatment, not behavior.

A **framed surface** is a surface whose boundary is intentionally visible, usually through a border, background contrast, shadow, or similar treatment.

A **panel** is a self-contained region that groups related content or controls. A panel may use framed-surface treatment when it should read as a plate sitting on a wider background area, but a panel is defined by its grouping role rather than by border styling alone.

A **pane** is a structural region of a window or split layout. A pane may contain panels or surfaces, but it does not imply a bordered or plate-like visual treatment.

A **card** is a compact framed surface for one repeated item or object summary. Prefer `card` only when the design intentionally uses separate item plates. For a single-column list whose items are separated by one thin divider, use list and row terminology instead of card terminology.

A **divided list** is a list surface whose rows are separated by thin row dividers so the rows read as parts of one continuous list rather than as independent panels or cards.

## Buttons

A **command button** is a standalone button-looking control that directly invokes one command.

Examples: `OK`, `Cancel`, `New Thread`.

A **toggle button** is a standalone button-looking control with persistent on/off state.

A **labeled cycle button** is a button-looking control that displays a stable label part and a current value part inside one button, separated visually, such as `Activity | Auto`. Activating it advances the value through a finite ordered set of allowed values.

The internal divider separates the label part from the value part; it does not create separate activation targets. A labeled cycle button is not a split button, because it has one activation behavior and no adjacent menu or flyout trigger area.

A **menu button** or **flyout button** is a button-looking control whose activation opens a menu or flyout instead of directly invoking a primary command.

A **split button** combines a direct primary command area with an adjacent menu or flyout trigger area for related alternatives.

## Triggers

A **flyout trigger** is any interactive element that opens a transient surface. It may be visually shaped as a button, title control, row, icon, or segment.

A **selector trigger** is a flyout trigger whose opened surface lets the user choose the current value or target.

Example: an active thread title that opens a thread selector is a selector trigger, not a command button.

An **action-menu trigger** is a flyout trigger whose opened surface contains commands related to the trigger's current object or displayed state.

Example: a live context readout that opens an operations menu containing `Compact` is an action-menu trigger.

A **hold-for-action trigger** is an interactive element whose command is invoked only after the user holds activation on the same target for a required duration. Use this behavior for dangerous or hard-to-recover commands where accidental activation would be costly.

A hold-for-action trigger may be visually shaped as a command button, action row, menu item, or another command-capable control. Do not call the element a button merely because it supports hold-for-action behavior.

While the hold is in progress, the target should show continuous progress feedback, such as a background fill that advances across the control. The progress feedback means the hold is accumulating; the command has not run until the hold completes.

The hold cancels without invoking the command when the user releases early, moves outside the target, closes the containing surface, changes focus in a way that invalidates the interaction, the target becomes disabled, or the command target is no longer the same stable object. Keyboard activation should provide an equivalent timed hold affordance and equivalent cancellation path when the control is keyboard reachable.

## Dropdowns

A **dropdown** is an anchored transient surface that opens from a trigger and visually attaches to that trigger, commonly below it. Dropdown describes placement and attachment, not content.

A **selector dropdown** is a dropdown whose rows choose the current value or target. Its trigger is a selector trigger, and its rows are selector rows.

A **command dropdown** is a command menu presented with dropdown attachment to its trigger.

Avoid **dropdown menu** as a generic term. Use `selector dropdown` when the surface chooses a value or target, and use `command menu` or `action menu` when the surface presents commands as menu items.

An attached dropdown should visually read as one control with its trigger, rather than as a detached popup. When the trigger and opened surface share a boundary, their outer walls should align unless a specific platform style requires a different attachment treatment.

## Menus

A **menu** is a transient command surface made of menu items.

A **command menu** or **action menu** is a menu opened to present commands. It may be opened by primary click, keyboard activation, or another explicit command gesture.

A **context menu** is a menu opened for a specific object or location through a context-menu gesture, such as secondary click or a keyboard context-menu command. Avoid using `context menu` merely to mean "a menu whose contents are relevant to the current state" when it is opened by primary click.

A **menu item** is a row inside a menu. It may invoke a command, toggle state, select an option, or open a submenu.

A **context menu item** is a menu item shown inside a context menu.

Menu items are row-shaped menu controls, not buttons. Their visual treatment is menu-row treatment, such as optional icon, checkmark, submenu indicator, and hover or selection highlight.

## Rows

An **action row** is a row-shaped control outside a formal menu that directly invokes a command.

A **selector row** is a row-shaped control inside a selector or list surface. Activating it chooses, opens, or focuses the represented item according to that surface's rules.

A **menu item** is not just any action row. Reserve `menu item` for rows that live inside menus.

## Text Inputs

An **input field** is a value-editing control. Use a more specific term when the value type or editing model matters.

A **text field** is a single-line text input control.

Examples: name fields, search fields, short path fields, and single-line setting values.

A **text area** is a multiline text input control for longer free-form text.

Examples: notes, instructions, descriptions, and message drafts.

A **color input** is a color-valued input control. It may combine a text field for a canonical color value, a swatch that previews the current valid color, and a color picker opened from the input.

A **color swatch** is the small visual sample of a color value. If activating it opens a picker, the swatch is also a flyout trigger.

A **color picker** is the surface used to choose or adjust a color. It may appear as a popup, flyout, or embedded panel depending on the application.

## Binary State Controls

A **switch** or **toggle switch** is a binary on/off control. Its primary meaning is the current state, not a one-time command.

Use switch terminology for settings that read naturally as enabled or disabled.

Example: an `End-turn sound` setting could use a switch for whether the sound is enabled.

A **checkbox** is also a binary state control, but it usually appears as a form/list option rather than an on/off device. Prefer checkbox terminology when the control represents selected/not-selected membership or a set of independent options.

Do not call a switch a button merely because it can be clicked. A switch is a state control; a command button invokes an action.

## Readouts

A **readout strip** is a linear container for frequently updated informational segments. Segments are often separated by dividers.

A **readout segment** is one informational partition inside a readout strip. Its primary job is to display current state or a live value.

An **interactive readout segment** is a readout segment with activation behavior.

A **readout selector** is an interactive readout segment that opens a selector or flyout for changing the value displayed by that segment.

An **action-menu readout segment** is an interactive readout segment that opens a command menu related to the displayed state.

Example: a context-usage readout that opens a menu containing `Compact` is an action-menu readout segment.

A **readout action segment** is an interactive readout segment that directly invokes one command without opening a menu.

Avoid **segmented control** for readout strips. A segmented control usually means a mutually exclusive choice control, not an informational strip of live values.

## Notification Popups

A **notification popup** is a transient, non-modal message surface that reports information, warnings, errors, or recovery actions without replacing the current workspace or view.

A **toast** is a notification popup that appears temporarily, usually near an edge or corner of the window or screen. Use `toast` when the popup is lightweight, time-bound, and not part of the main layout.

An **in-app notification popup** is a notification popup owned by the application window rather than by the operating system notification center.

A **notice** is a notification popup whose content is meant to be read as localized application state or recovery information. Beryl's top-right surface notices are in-app notification popups.

Notification popups are not menus. If they expose commands such as close, retry, or details, those commands are controls inside the notification popup.

## Scrollbars and Scrolling

A **scrollable surface** or **scroll container** is a bounded area whose content can move because the content is larger than the visible area.

The **viewport** is the visible portion of a scrollable surface.

The **scroll extent** is the full scrollable content range, including content outside the viewport.

The **scroll position** is the current offset of the viewport within the scroll extent.

**Scrolling** is the interaction that changes the scroll position. Scrolling may be controlled by mouse wheel, touchpad, touchscreen gesture, keyboard commands, programmatic focus movement, or direct manipulation of a scrollbar.

A **scrollbar** is the visual/control element associated with one scroll axis. It provides position feedback and may also let the user manipulate the scroll position directly.

A **vertical scrollbar** controls vertical scrolling.

A **horizontal scrollbar** controls horizontal scrolling.

A **scrollbar thumb** is the draggable part of a scrollbar. Its position represents the current scroll position, and its size may represent the visible viewport's proportion of the scroll extent.

A **scrollbar track** is the area along which the thumb moves. Some visual styles render the track prominently; others make it subtle or invisible.

A **scrollbar affordance** is any visible cue that scrolling is available or currently happening, including a thumb-only scrollbar that appears during pointer movement or active scrolling.

Do not describe mouse wheel, touchpad, or keyboard scrolling as using the scrollbar. Those inputs scroll the scrollable surface; the scrollbar reflects the resulting position and may provide an additional direct manipulation control.

## Bars and Strips

A **bar** is a linear application chrome region, usually anchored to an edge of a window or panel.

A **strip** is a generic linear container. Use `strip` when the container's role is structural or app-specific, and use a more specific term such as `toolbar` or `status bar` when the role is common and clear.

A **toolbar** is a bar that exposes frequently used commands, tools, modes, or selectors. It is primarily a command surface, and it may contain command buttons, menu buttons, split buttons, toggle buttons, selector triggers, separators, and grouped controls.

A **toolbar separator** is a visual divider that separates groups of related toolbar controls.

A **status bar** is a bar that presents current application, document, selection, task, or environment state. It is primarily informational, and it commonly contains readout segments.

An interactive status bar segment is still a readout segment first. Qualify it by behavior, such as `readout selector`, `action-menu readout segment`, or `readout action segment`.

Do not use `toolbar` for a bar whose primary role is live state display. Do not use `status bar` for a bar whose primary role is command access.

## Settings Surfaces

A **settings window** or **preferences window** is an auxiliary window for viewing and changing application preferences.

Use `settings window` when the application vocabulary says settings. Use `preferences window` when matching a platform or product vocabulary that uses preferences.

A **settings section** is a named group of related settings.

A **settings navigation** is the control or surface used to switch between settings sections. It may be a sidebar, tab list, list, or column depending on the application.

A **settings pane** is the visible content area for one settings section.

A **settings row** is a row in a settings pane that pairs a setting label and optional help text with the control used to edit that setting.

A **setting label** names the setting.

A **setting description** or **setting help text** explains the consequence, scope, or valid use of a setting.

A **setting value control** is the control that edits the setting value, such as a text field, text area, color input, switch, checkbox, selector trigger, or action button.

An **Apply button** commits staged settings without necessarily closing the settings window.

## Column Browsers

A **column browser** is a navigation surface that represents traversal as a left-to-right trail of columns. Selecting an item in one column opens the next column to the right.

The pattern is also known as **Miller columns**.

A **browser column** is one vertical list inside a column browser.

A **root column** is the first browser column. It lists the entry points for traversal.

A **successor column** is a browser column opened from the current selection in the previous column.

A **column trail** is the ordered set of visible browser columns that represents the current traversal path.

A **column row** is a row inside a browser column.

A **branching row** is a column row that can open a successor column.

A **terminal row** is a column row that does not open a successor column. Activating it may still invoke a command or select a target, depending on the browser's domain.

A **selection path** is the ordered domain-item path represented by the selected rows across the column trail.

A **column browser viewport** is the visible area containing the column trail. It may own horizontal scrolling when the column trail exceeds the available width, while each browser column may own its own vertical scrolling.

A **hierarchical column browser** adapts the column-browser pattern to tree-like data, where each successor column shows children of the selected row.

A **columnar graph browser** adapts the column-browser pattern to graph-like data, where each successor column shows domain-defined adjacent, linked, or expanded items for the selected row. The visible column trail is a navigation projection, not a requirement that the underlying data be a tree.

Use `column browser` for the generic interaction pattern. Use `columnar graph browser` when the browsed data is graph-shaped and may contain cross-links, multiple parents, cycles, or terminal link rows.

## Naming Pattern

Prefer names that combine visual family and behavior:

- `command button`
- `menu button`
- `labeled cycle button`
- `framed surface`
- `panel`
- `pane`
- `card`
- `divided list`
- `selector trigger`
- `action-menu trigger`
- `hold-for-action trigger`
- `selector dropdown`
- `selector row`
- `action row`
- `text field`
- `text area`
- `color input`
- `toggle switch`
- `readout selector`
- `action-menu readout segment`
- `readout action segment`
- `notification popup`
- `toast`
- `scrollable surface`
- `scrollbar`
- `vertical scrollbar`
- `horizontal scrollbar`
- `scrollbar thumb`
- `toolbar`
- `status bar`
- `settings window`
- `settings row`
- `column browser`
- `browser column`
- `columnar graph browser`

This keeps terminology precise when an element is clickable but intentionally does not use button chrome.
