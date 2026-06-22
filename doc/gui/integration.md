# GUI Integration

# Windows

## Main Workspace Window

The main workspace window is the primary Beryl OS window.

Its top-level layout is a fixed toolbar strip, a stretchable workspace body, and a fixed status line strip anchored to the OS window bottom edge. The workspace body places the thread strip above the conversation column. The conversation column stacks the stretchable transcript region, optional bounded activity panel, and pinned user input panel.

The minimum OS window size is derived from the minimum sizes of currently visible child widgets so pinned controls remain reachable. The main window content does not own vertical scrolling during normal operation; only explicitly designated child panels or regions own vertical scrolling.

Feature-owned overlays, popups, menus, notices, and previews remain bounded within the OS window or the owning feature-declared region.

### Slots

#### Slot: main-window.toolbar

This slot is the fixed-height toolbar strip anchored to the top edge of the main workspace window. It is for global controls, navigation chrome, and window-level actions that must remain reachable while the conversation column changes.

Controls in this slot align leading navigation and workspace controls to the start, reserve flexible space in the middle, and align window-level actions to the end. The toolbar strip stretches horizontally with the OS window and does not automatically resize vertically.

The toolbar strip is controls-only and does not reserve persistent static workspace-name text, thread-count text, visible graph-hotkey labels, or non-interactive status chips.

#### Slot: main-window.thread-strip

This slot is the fixed-height row directly under the toolbar strip. It is for selected-thread controls, thread navigation chrome, and the active thread title selector.

The thread strip stretches horizontally with the OS window and keeps long thread labels from causing outer window-content scrolling. It does not contain static runtime-context labels before the active thread title selector.

#### Slot: main-window.transcript-region

This slot is the stretchable region in the conversation column below the thread strip and above any visible activity panel and user input panel. It is for the selected conversation transcript presentation.

The transcript region receives height left after fixed and visible bounded panels are laid out. Its owning feature decides transcript scrolling, activation, row presentation, selection, media, and context-menu behavior.

#### Slot: main-window.activity-panel

This slot is the optional bounded panel between the transcript region and the user input panel. It is for live or recent selected-thread activity that should take height from the transcript region while preserving pinned lower chrome.

When visible, this slot remains within the conversation column and does not displace the user input panel or status line off-window.

#### Slot: main-window.user-input-panel

This slot is the pinned panel near the bottom of the conversation column, above the status line. It is for user-authored draft input and draft-adjacent controls for the selected conversation or pending new-thread draft.

#### Slot: main-window.status-line

This slot is the fixed-height strip anchored to the bottom edge of the main workspace window. It is for compact status readouts and selected-thread control affordances that are not transcript content.

#### Slot: main-window.overlays

This slot is the bounded layer above the main workspace window content. It is for transient feature-owned overlays, popups, notices, menus, and previews that must leave the main workspace shell in place and remain within the OS window bounds.

Content mounted here is clamped to the visible OS window or to the owning region declared by the feature doc.

#### Slot: semantic-graph.overlay-affordances

This slot is inside semantic graph overlay graph-browser rows and graph node context menus. It is for feature-owned graph-node affordances that extend graph rows, indicators, tooltips, or menu commands without replacing graph navigation.

## Settings Window

The settings window is a top-level auxiliary OS window for Beryl-owned application preferences.

It defines its own dedicated chrome and does not inherit the main workspace toolbar strip. Its minimum OS window size is derived from the settings window chrome and currently visible settings body content.

### Slots

#### Slot: settings-window.body

This slot is the root layout area of the settings window. It is for settings navigation, the active settings page or subpage, and settings-window chrome that belongs inside the auxiliary OS window.
