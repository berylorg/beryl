# GUI Integration

# Windows

## Main Conversation Window

The main conversation window is Beryl's primary OS window. Beryl may own multiple independent main conversation windows at once.

Its top-level layout is a fixed toolbar strip, an optional fixed thread-lineage strip, a stretchable conversation body, and a fixed status line strip anchored to the OS window bottom edge. The thread-lineage strip appears directly below the toolbar when the selected thread has parent-thread lineage and otherwise consumes no layout space. The conversation body stacks the stretchable transcript region, optional bounded activity panel, optional fixed discussion-status strip, and pinned user input panel.

The minimum OS window size is derived from the minimum sizes of currently visible child widgets so pinned controls remain reachable. The main window content does not own vertical scrolling during normal operation; only explicitly designated child panels or regions own vertical scrolling.

Feature-owned overlays, flyouts, menus, notices, and previews remain bounded within the OS window or the owning feature-declared region.

### Slots

#### Slot: main-window.toolbar

This slot is the fixed-height toolbar strip anchored to the top edge of the main conversation window. It is for global controls, thread-navigation chrome, and window-level actions that must remain reachable while the conversation body changes.

Controls in this slot align leading thread-navigation controls to the start, reserve flexible space in the middle, and align window-level actions to the end. The toolbar strip stretches horizontally with the OS window and does not automatically resize vertically.

The toolbar strip is controls-only and does not reserve persistent static runtime names, root paths, thread counts, visible hotkey labels, or non-interactive status chips.

#### Slot: main-window.thread-lineage

This slot is the conditional fixed-height strip directly below the toolbar. It is for selected-thread parent-lineage navigation.

When visible, the strip stretches horizontally with the OS window and keeps long lineage labels from causing outer window-content scrolling. When no lineage is visible, the slot contributes no persistent empty row.

#### Slot: main-window.transcript-region

This slot is the stretchable region in the conversation body below the toolbar and any visible thread-lineage strip and above any visible activity panel, discussion-status strip, and user input panel. It is for the selected conversation transcript presentation.

The transcript region receives height left after fixed and visible bounded panels are laid out. Its owning feature decides transcript scrolling, activation, row presentation, selection, media, and context-menu behavior.

#### Slot: transcript.context-records

This slot is a routed presentation-record contribution point inside the transcript view's ordered flow. It is for stable feature-owned synthetic context that belongs at an exact transcript boundary without becoming a Syndic turn or peer transcript viewport.

Contributed records participate in transcript realization, scrolling, anchoring, measurement, selection, and accessibility through the transcript view. When no record applies at a resident boundary, the slot contributes no presentation item or reserved space.

#### Slot: transcript.code-panel-actions

This slot is the routed command-contribution region inside an eligible transcript code panel's header. It is for feature-owned commands that apply to a recognized code-panel content type without making the contributing feature another owner of transcript layout or code-panel mechanics.

The slot exists only for a code panel whose content and feature state admit at least one contribution. Multiple feature definitions may target this slot, but only commands applicable to the exact rendered panel are composed. When no command applies, the slot contributes no header control or reserved space.

#### Slot: main-window.activity-panel

This slot is the optional bounded panel between the transcript region and any visible discussion-status strip or user input panel. It is for live or recent selected-thread activity that should take height from the transcript region while preserving pinned lower chrome.

When visible, this slot remains within the conversation body and does not displace the discussion-status strip, user input panel, or status line off-window.

#### Slot: main-window.discussion-status

This slot is the conditional fixed-height strip below any visible activity panel and immediately above the user input panel. It is for selected-thread discussion lifecycle status and compact input-gating actions.

When no branch discussion is selected, the slot contributes no persistent empty row. When visible, state changes retain one fixed height and do not resize or replace the user input panel.

#### Slot: main-window.user-input-panel

This slot is the pinned panel near the bottom of the conversation body, above the status line. It is for user-authored draft input and draft-adjacent controls for the selected conversation thread.

The ordinary conversation composer and an execution-blocking native-lineage recovery prompt are
mutually exclusive contents of this slot. Replacing one with the other does not create an overlay,
modal interaction boundary, second panel row, or different top-level window layout.

#### Slot: main-window.status-line

This slot is the fixed-height strip anchored to the bottom edge of the main conversation window. It is for compact status readouts and selected-thread control affordances that are not transcript content.

#### Slot: main-window.overlays

This slot is the bounded layer above the main conversation window content. It is for feature-owned overlays, flyouts, persistent or transient notices, menus, and previews that must leave the conversation shell in place and remain within the OS window bounds.

Content mounted here is clamped to the visible OS window or to the owning region declared by the feature doc.

## Busy Home Window

The busy home window is the only startup surface shown when another live Beryl process owns the configured Beryl home. It is a compact, non-resizable application window with no main conversation toolbar, transcript, composer, status line, Settings entry, or runtime/root navigation.

Its body remains fully visible at the window's minimum size and does not own scrolling.

### Slots

#### Slot: busy-home-window.body

This slot fills the busy home window. It is for the ownership explanation, automatic-exit status, and sole explicit exit command.

## Home Failure Window

The home failure window is the only startup surface shown when the configured Beryl-home store cannot be opened or validated and no ordinary main conversation shell can be restored safely. It is a compact, non-resizable application window with no main conversation toolbar, transcript, composer, status line, Settings entry, or runtime/root navigation.

Its body remains fully visible at the window's minimum size and does not own scrolling.

### Slots

#### Slot: home-failure-window.body

This slot fills the home failure window. It is for the bounded startup failure explanation and exact same-home Retry and Exit commands.

## Settings Window

The external `settings-window` widget directly owns Beryl's top-level auxiliary OS window for application preferences. Beryl configures that window with its section identities, routed pages, settings content, and feature commands; it does not place the external window inside another Beryl-owned window body.

The external window defines its own dedicated chrome and top-level layout and does not inherit the main conversation toolbar strip. Beryl-provided page content respects the external widget's sizing, focus, navigation, popup, and scrolling contracts.

### Slots

#### Slot: settings-window.page-content

This slot is the routed feature-content region inside the external settings-window widget's selected-page body. It is for feature-owned settings pages and subpages that participate in the Beryl settings navigation model.

Multiple page definitions may target this slot, but only the page selected by the settings route is visible and interactive. Page contributions do not replace the settings sidebar, outer shell, page routing, or settings-window chrome and do not render simultaneously merely because they share the slot.
