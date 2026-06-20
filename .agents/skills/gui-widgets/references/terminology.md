# GUI Terminology

Use this catalog as baseline vocabulary for GUI documentation and discussion.

The entries are intentionally short. They establish what a term means, not every implementation rule for that term.

# Term Choice Rules

- Prefer the most specific familiar term over a vague local name.
- Use `command button` for a freestanding button-shaped command control. Treat `action button` and `push button` as synonyms, not separate canonical names.
- Use `command row` for a row-shaped command control inside a list-like surface, including menus. Treat `menu item`, `context menu item`, and `action row` as context-specific descriptions unless a platform API requires those names.
- Use `selector` only when the exact selection control is intentionally unspecified.
- Use `button` for an action invoker, not for a persistent on/off value.
- Use `checkbox` for an independent boolean option.
- Use `radio button` for one choice inside a mutually exclusive set.
- Use `switch` for an immediate on/off setting that is presented as a control, not a confirmation action.
- Use `dropdown` only as a broad visual family; prefer `dropdown menu`, `select`, `combobox`, or `flyout` when the behavior matters.
- Use `modal` to describe interaction blocking, not visual shape.
- Avoid `popup` when a more precise term such as `popover`, `flyout`, `menu`, `tooltip`, or `dialog` applies.

# Contents

- Structure and Surfaces
- Controls and Inputs
- Collections and Navigation
- Content and Indicators
- Selection and Editing
- State and Feedback
- Layout and Spatial Terms
- Interaction Terms

# Structure and Surfaces

- **Application window**: The top-level window owned by an application.
- **Backdrop**: A layer behind an overlay, often used to dim or block interaction with underlying UI.
- **Banner**: A prominent horizontal message region, usually near the top of a page or surface.
- **Canvas**: A drawable or freeform work area for spatial content.
- **Card**: A bounded content container presenting one object, summary, or repeated item.
- **Chrome**: UI around content, such as title bars, toolbars, tabs, and window controls.
- **Callout**: A contextual message or note visually tied to an element or region.
- **Container**: A generic element that owns layout or grouping for child elements.
- **Dialog**: A transient window or overlay used for a focused task, confirmation, or message.
- **Drawer**: A panel that slides from an edge and exposes secondary navigation or controls.
- **Flyout**: A transient panel opened from another control and positioned near that control.
- **Frame**: A bounded structural region around content or another surface.
- **Inspector**: A side or floating surface for viewing or editing properties of the current selection.
- **Modal dialog**: A dialog that blocks interaction with the rest of the relevant UI until dismissed.
- **Modeless dialog**: A dialog that can remain open while the user continues working elsewhere.
- **Overlay**: A UI layer rendered above the normal surface.
- **Page**: A navigable full-view unit of application content.
- **Pane**: A resizable or persistent region within a larger window or view.
- **Panel**: A grouped surface that contains related controls or content.
- **Popover**: A small contextual overlay anchored to an element.
- **Sheet**: A transient surface that enters from an edge or sits over content for a contained task.
- **Sidebar**: A persistent or collapsible side region for navigation, structure, or contextual tools.
- **Split view**: A layout with adjacent panes, often separated by a splitter.
- **Surface**: A visible UI region that can contain content, controls, or overlays.
- **Tooltip**: A brief hover or focus hint that explains an element without accepting interaction.
- **View**: A rendered presentation of a state, document, route, or application area.
- **Viewport**: The visible portion of a scrollable or rendered area.

# Controls and Inputs

- **Action button**: A synonym for command button. Prefer `command button` in widget specs.
- **Affordance**: A visible cue that suggests an available interaction.
- **Button**: A control that invokes an action when activated.
- **Button group**: A visually grouped set of related buttons.
- **Checkbox**: A control that toggles one independent boolean option.
- **Color picker**: A control for choosing a color value.
- **Color swatch**: A small color sample, often selectable or used to preview a chosen color.
- **Combobox**: A text entry or selection control that combines an editable field with a list of options.
- **Command button**: A freestanding button-shaped control that runs a command. Prefer this term over `action button` in widget specs.
- **Command row**: A row-shaped control inside a list-like surface that invokes a command when activated.
- **Control**: An interactive UI element that accepts input or changes state.
- **Control group**: A labeled group of related controls.
- **Date picker**: A control for selecting a calendar date.
- **Disclosure button**: A control that expands or collapses additional content.
- **Dropdown**: A broad term for a control or overlay that opens downward or near its trigger.
- **Dropdown button**: A button that opens a menu, flyout, or other dropdown surface.
- **Dropdown menu**: A menu opened from a control and presented in a dropped-down overlay.
- **Dropdown select**: A selection control that shows the current value and opens an option list.
- **Field**: A form element that accepts, displays, or controls one value.
- **Field label**: The label that names a field.
- **File picker**: A platform or application control for selecting files or folders.
- **Form**: A group of fields and controls used to collect or edit structured input.
- **Icon button**: A button represented primarily by an icon instead of a text label.
- **Input field**: A generic field that accepts user-entered data.
- **Link**: An inline or standalone text control that navigates or opens a referenced target.
- **Menu button**: A button that opens a menu of commands or options.
- **Option**: One selectable value in a selection control or menu.
- **Picker**: A control or flow for choosing a value from a constrained domain.
- **Radio button**: A control that selects one option from a mutually exclusive set.
- **Search field**: A text field specialized for entering search queries.
- **Segmented control**: A compact set of adjacent options, usually used for mode or filter selection.
- **Select**: A control that chooses one value from a known option set.
- **Selector**: A generic control or region used to choose one or more values from a set.
- **Slider**: A control that chooses a numeric value by moving a thumb along a track.
- **Spin box**: A numeric input with increment and decrement controls.
- **Split button**: A button with one region for a default action and another region for alternate actions.
- **Splitter**: A draggable divider that resizes adjacent panes or regions.
- **Stepper**: A control that changes a value in fixed increments.
- **Switch**: A control that toggles an immediate on/off state.
- **Text area**: A multiline text input.
- **Text field**: A single-line text input.
- **Time picker**: A control for selecting a time value.
- **Toggle button**: A button that remains in an on/off or selected/unselected state after activation.
- **Widget**: A reusable UI element with a recognizable structure or behavior.

# Collections and Navigation

- **Accordion**: A stacked set of expandable sections.
- **Action menu**: A menu opened from an explicit primary-action, menu-button, or action-menu trigger rather than from a context-menu gesture.
- **Action row**: A context-specific synonym for a command row outside a formal menu. Prefer `command row` in widget specs.
- **App bar**: A top-level bar for application identity, navigation, or high-level actions.
- **Breadcrumb**: A navigation trail showing the current location in a hierarchy.
- **Carousel**: A horizontally or sequentially browsed collection of items.
- **Cell**: One intersection of a row and column in a table or grid.
- **Command bar**: A toolbar-like region focused on commands for the current context.
- **Command palette**: A searchable command launcher, usually presented as a transient overlay.
- **Column**: A vertical series of cells in a table or grid.
- **Column header**: A header that labels a table or grid column.
- **Context menu**: A menu opened by a context-menu gesture for a specific object, location, or focus context.
- **Data grid**: A grid optimized for tabular data with interactive cells, rows, columns, or sorting.
- **Divider**: A visual separator between regions, groups, or items.
- **Gallery**: A visual collection of items, often image or preview oriented.
- **Grid**: A two-dimensional arrangement of items or cells.
- **Header**: A leading region or row that labels, titles, or controls following content.
- **Header bar**: A bar at the top of a window, page, panel, or region.
- **List**: A one-dimensional collection of items.
- **List item**: One row or entry in a list.
- **Master-detail view**: A layout where selecting an item in one region shows its details in another.
- **Menu**: A command-oriented surface made of rows. Use `context menu` for context-menu gesture invocation and `action menu` for explicit primary-action invocation.
- **Menu bar**: A horizontal set of top-level menus, common in desktop applications.
- **Menu item**: Common platform term for a menu row. Prefer `command row`, `selector row`, `toggle row`, or `submenu row` when documenting reusable row behavior.
- **Navigation bar**: A region containing primary navigation controls.
- **Pagination**: Controls for moving between discrete pages of a result set.
- **Ribbon**: A large command surface organized into tabs and command groups.
- **Row**: A horizontal series of cells or item fields.
- **Row header**: A header that labels a table or grid row.
- **Section**: A named or visually grouped part of a larger surface.
- **Separator**: A line, gap, or element that separates items or groups.
- **Selector row**: A row-shaped control that chooses, opens, or focuses the represented value or object.
- **Status bar**: A bar showing status, mode, progress, or contextual information.
- **Tab**: A labeled selector for one panel in a tabbed interface.
- **Tab list**: The set of tabs that controls visible tab panels.
- **Tab panel**: The content region associated with a selected tab.
- **Table**: A row-and-column presentation of structured data.
- **Toolbar**: A compact collection of frequently used commands.
- **Submenu row**: A menu row that opens a nested menu instead of invoking a command directly.
- **Toggle row**: A row-shaped control that toggles a persistent on/off or selected/unselected state.
- **Tree**: A hierarchical collection with expandable and collapsible nodes.
- **Tree node**: One item in a tree, optionally with child nodes.
- **Tree view**: A UI presentation of a tree.
- **Wizard**: A guided multi-step flow for completing a task.

# Content and Indicators

- **Alert**: A prominent message that communicates important status or requires attention.
- **Avatar**: A visual representation of a person, account, agent, or entity.
- **Badge**: A small label or count attached to another element.
- **Body text**: Primary reading text in a UI surface.
- **Caption**: Supporting text associated with an image, chart, table, or field.
- **Chart**: A visual representation of data.
- **Chip**: A compact token representing an entity, filter, selection, or input.
- **Empty state**: The content shown when a view or collection has no items to display.
- **Glyph**: A symbolic mark, usually an icon-like shape inside a font or icon set.
- **Heading**: Text that titles a section, surface, or group.
- **Helper text**: Supporting text that clarifies a field, setting, or action.
- **Icon**: A small graphical symbol that represents an object, action, status, or concept.
- **Image**: A visual media element.
- **Inline message**: A message shown in place near the relevant content or control.
- **Label**: Text that names a control, field, item, or region.
- **Meter**: A visual indicator of a scalar measurement within a known range.
- **Notification**: A message about an event, often delivered outside the immediate workflow.
- **Pill**: A rounded compact label or control; often similar to chip but more shape-specific.
- **Placeholder**: Temporary text or visual content shown before real user or system content exists.
- **Preview**: A compact representation of content before opening, applying, or committing it.
- **Progress bar**: An indicator showing completion progress over a known or estimated duration.
- **Skeleton**: A placeholder layout shown while content is loading.
- **Spinner**: An indeterminate loading indicator.
- **Tag**: A label representing categorization, metadata, or a selected token.
- **Thumbnail**: A small preview image or visual summary.
- **Toast**: A transient, non-blocking status message.
- **Validation message**: A message explaining whether a value is valid or how to correct it.

# Selection and Editing

- **Active item**: The item currently receiving keyboard navigation or command context.
- **Caret**: The text insertion marker.
- **Checked item**: An item whose checkable state is on.
- **Collapsed**: A state where expandable content is hidden.
- **Current item**: The item that represents current location, current value, or navigation position.
- **Cursor**: The pointer or text-position indicator, depending on context.
- **Drag handle**: A visible affordance used to drag, reorder, resize, or move something.
- **Drag preview**: A visual representation of content while it is being dragged.
- **Drop target**: The region where dragged content can be dropped.
- **Expanded**: A state where previously hidden child content is visible.
- **Focus**: The element currently receiving keyboard input.
- **Focus ring**: A visible outline or effect that indicates keyboard focus.
- **Handle**: A draggable part of a control.
- **Indeterminate**: A mixed or unknown state, often used when child selections differ.
- **Insertion point**: The location where inserted text or content will appear.
- **Multi-selection**: A selection containing more than one item.
- **Range selection**: A continuous selection between two endpoints.
- **Selected item**: An item included in the current selection.
- **Selection**: The current chosen item, items, text, range, or object set.
- **Single selection**: A selection that allows only one selected item.
- **Text selection**: A selected span of text.
- **Thumb**: The movable part of a slider, scrollbar, or similar control.
- **Track**: The rail or path along which a thumb moves.
- **Unselected item**: An item not included in the current selection.

# State and Feedback

- **Active**: Currently engaged, invoked, navigated to, or contextually current.
- **Busy**: Temporarily unable to accept normal interaction because work is in progress.
- **Checked**: On or selected for a checkable control.
- **Dirty**: Modified since the last saved, synced, or committed state.
- **Disabled**: Not available for interaction.
- **Enabled**: Available for interaction.
- **Error**: A state indicating a problem that blocks or invalidates the intended result.
- **Focus-visible**: A focus state that should be visibly indicated, usually for keyboard navigation.
- **Focused**: Receiving keyboard focus.
- **Hover**: Pointer is positioned over an element without pressing it.
- **Invalid**: Fails validation or cannot currently be accepted.
- **Loading**: Content or state is being fetched, computed, or prepared.
- **Open**: A disclosure, menu, flyout, dialog, or expandable element is visible.
- **Closed**: A disclosure, menu, flyout, dialog, or expandable element is not visible.
- **Hidden**: Not currently visible in the UI.
- **Pending**: Requested work has started or is queued but has not completed.
- **Pressed**: Pointer or key activation is held down on an element.
- **Pristine**: Unmodified since initial load, reset, save, or commit.
- **Readonly**: Visible and selectable but not editable.
- **Required**: Must be provided or completed before a task can continue.
- **Saved**: Persisted successfully.
- **Selected**: Included in the current selection or chosen as the current value.
- **Success**: A state indicating that an operation completed as intended.
- **Synced**: Local state matches the authoritative source.
- **Unchecked**: Off or not selected for a checkable control.
- **Valid**: Passes validation and can be accepted.
- **Warning**: A state indicating risk or attention without necessarily blocking progress.
- **Visible**: Currently shown in the UI.

# Layout and Spatial Terms

- **Alignment**: How elements line up relative to each other or a container.
- **Anchor**: The reference element or point used to position another element.
- **Breakpoint**: A layout threshold where responsive behavior changes.
- **Clipping**: Hiding visual content outside a boundary.
- **Density**: How compactly UI elements and content are arranged.
- **Gap**: Space between layout items.
- **Gutter**: Reserved spacing between columns, panes, or content regions.
- **Hit target**: The interactive area that accepts pointer or touch input.
- **Inset**: Space inside an edge, often from a container boundary to its content.
- **Margin**: Space outside an element.
- **Origin**: The reference point from which position or transformation is measured.
- **Overflow**: Content extending beyond its available space.
- **Padding**: Space between an element's boundary and its contents.
- **Responsive layout**: Layout that adapts to viewport, container, device, or input constraints.
- **Scroll container**: A region whose overflowing content can be scrolled.
- **Scrollbar**: A control that shows scroll position and allows scrolling through content.
- **Stacking order**: The front-to-back rendering order of overlapping elements.
- **Truncation**: Shortening visible text or content to fit available space.
- **Wrap**: Moving content onto additional lines or rows when space is constrained.
- **Z-index**: A stacking value used by some UI systems to order overlapping elements.

# Interaction Terms

- **Accelerator**: A keyboard shortcut that invokes a command directly.
- **Activate**: Trigger a control's primary behavior.
- **Blur**: Lose keyboard focus.
- **Cancel**: Abandon an in-progress interaction without applying its result.
- **Click**: Press and release a pointer button on a target.
- **Commit**: Accept and apply an in-progress value, selection, or task.
- **Dismiss**: Close a transient surface without necessarily applying a value.
- **Double-click**: Two rapid clicks on the same target, often opening or editing an item.
- **Drag**: Move a pointer while holding an item, handle, or region.
- **Drop**: Release dragged content onto a target.
- **Gesture**: A touch, pointer, or trackpad movement interpreted as a command.
- **Hover**: Rest the pointer over an element.
- **Invoke**: Run the action associated with a control or command.
- **Keyboard shortcut**: A key combination that invokes a command.
- **Long press**: Press and hold on a target to reveal secondary behavior.
- **Mnemonic**: A keyboard-accessible character associated with a label or command.
- **Pan**: Move the visible portion of spatial content without changing scale.
- **Press**: Hold pointer, touch, or key activation on a target.
- **Reset**: Return values or state to a previous baseline.
- **Resize**: Change the dimensions of a window, pane, region, or object.
- **Scroll**: Move through content larger than its viewport.
- **Submit**: Send form or task input for processing.
- **Tap**: Touch and release a target.
- **Typeahead**: Navigate or filter by typing characters.
- **Undo**: Reverse the most recent applicable change.
- **Redo**: Reapply a change that was undone.
- **Zoom**: Change the scale of visible content.
