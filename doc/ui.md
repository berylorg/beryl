# Shared UI Contract

This document defines the shared window, panel, scrolling, and reusable-widget behavior referenced by `doc/design.md`.

The terms `stretch`, `fixed`, `anchored`, `overlay`, and `scrollable` describe target runtime behavior, not a required implementation technique.

## Global Window Rules

- The main workspace window includes a toolbar strip anchored to the top edge of the OS window.
- A top-level auxiliary window, such as the settings window, may define its own dedicated chrome and does not inherit the main workspace toolbar strip.
- When a window includes a toolbar strip, that toolbar strip stretches horizontally with the OS window and does not automatically resize vertically.
- The main OS window must not rely on outer window-content scrolling to keep primary widgets reachable during normal operation.
- Only explicitly designated child panels may own vertical scrolling.
- The minimum OS window size is derived from the minimum sizes of the currently visible child widgets so that pinned controls do not move off-viewport.

## Shared Terms

- `OS window` is the native top-level application window.
- `Toolbar strip` is the fixed-height top row reserved for global controls such as the Settings button and workspace-switching actions.
- `Thread strip` is the fixed-height row beneath the toolbar that exposes the `New Thread` button, the active thread title control, and runtime context when needed.
- `Conversation column` is the left-side workspace area beneath the thread strip that contains the transcript region, optional activity panel, and user input panel.
- `Transcript region` is the stretchable workspace area that shows the active Codex thread.
- `Activity panel` is the optional vertically resizable strip that shows the selected backend thread's in-memory turn activity history.
- `User input panel` is the pinned bottom panel for composing the next user turn within the conversation column.
- `Status line strip` is the fixed bottom row for compact backend and turn status metadata.
- `Checklist sidebar` is the optional right-edge panel that shows the selected checklist.
- `Sidebar splitter` is the draggable vertical separator between the conversation column and the checklist sidebar when the checklist sidebar is visible.
- `Column selector widget` is the reusable horizontally branching column-selection widget used by domain-specific surfaces such as the graph overlay and thread selector.
- `Column selector container` is the horizontally scrollable area that owns a column selector widget's side-by-side columns.
- `Column selector column` is one fixed-width vertically scrollable explorer viewport inside a column selector widget.
- `Graph overlay` is the toggleable layered graph-explorer surface shown above the workspace content.
- `Graph column` is one explorer viewport within the graph overlay.
- `Thread selector` is the popup surface for selecting and activating a Codex conversation thread from the active workspace's member-thread inventory.
- `Model/reasoning popup` is the status-line popup surface for choosing the selected thread's model and reasoning effort, or the pending new-thread draft's first-turn model and reasoning effort.
- `Context operations popup` is the status-line popup surface for selected-thread context operations such as compaction.
- `Turn operations popup` is the status-line popup surface for actions on the selected thread's interruptible active backend turn.
- `Code panel widget` is the reusable plain-text monospace widget used for commands, outputs, patches, and other code-like blocks.
- `Transcript quote popup` is the bounded transient popup panel shown near selected transcript text with quote-related actions for the current selection.
- `Transcript turn context menu` is the bounded transient command surface opened from a loaded parent transcript turn when no transcript text selection is active.
- `Composer image payload` is the original pasted image data retained for composer preview and backend delivery.
- `Composer image marker` is one compact inline visual marker, such as `[A]`, that represents an occurrence of a pasted image reference inside the conversation composer draft.
- `Transcript image marker` is one compact inline read-only visual marker, such as `[A]`, that represents an image position in an accepted or historical user input fragment.
- `Transcript media item` is one decoded or pending raster image item rendered from native app-server generated-image output or from Markdown image syntax with a supported local file target.
- `Transcript media run` is a consecutive group of transcript media items rendered together with wrapping horizontal layout, except while a UI-local promotion temporarily gives one item its own single-image row.
- `Transcript media context menu` is the transcript turn context menu augmented with image-specific actions when opened from one loaded transcript media item.
- `Composer image context menu` is the bounded transient command surface opened from one composer image marker.
- `Image preview popup` is the bounded transient popup panel that shows a larger fitted preview of an image referenced by a composer or transcript marker.
- `Turn activity caret` is the non-interactive block cursor shown at the end of the transcript while the selected thread's parent turn is working.
- `Popup widget` is a bounded transient surface such as the workspace picker, thread selector, or model/reasoning popup, layered above the main workspace window without replacing it.
- `Workspace picker popup` is the merged two-column popup used for workspace selection and active-workspace member management.
- `Workspace members column` is the right column inside the workspace picker popup. It manages the active workspace's default runtime environment and workspace members.
- `Surface notice` is a bounded top-right transient message surface for localized errors and recovery information that should not replace the active workspace view.
- `Context menu widget` is a bounded transient command surface opened from a specific row or control, with optional submenus layered above the main workspace window.
- `Settings window` is the dedicated preheated top-level OS window for application settings. It is shown and hidden rather than rebuilt for each open request.
- `Settings section` is one broad, stable top-level category shown in the settings window sidebar.
- `Settings page` is the right-pane content surface for one settings section or subpage.
- `Settings subpage` is a nested right-pane settings page reached from a step-in row, breadcrumb link, or back action; it is not represented as a nested sidebar row.
- `Settings row` is one schema-backed settings item or action row with a stable setting id or stable action id, label, optional description, value/control area, modified state, and row context actions.
- `Theme role navigator` is the theme editor's purpose-built column navigator over the actual UI role schema tree. Every navigator row is a real UI role id, and synthetic grouping labels or folder rows are invalid.
- `Theme candidate code panel` is a transcript code panel produced from a fenced code block whose language label is `beryl-theme`; it presents ordinary transcript code plus Beryl-owned theme actions.

## Appearance Theme Roles

- Every Beryl-owned visual element resolves its user-visible appearance values from the active theme model or from a documented derivation of an active theme property. A role exposes only the properties that the GUI surfaces using that role can consume, such as background, border, single-primitive color, text foreground, text background, font family, font size, and font weight.
- Theme roles cover main-window chrome, toolbar and thread-strip surfaces, buttons and action rows, inputs, transcript shell and transcript content, Markdown inline and block structures, code panels, syntax-highlight token roles, user input fragments, activity rows, status line cells and dynamic status values, graph and checklist visuals, selector accents, scrollbars, structural separators, popups, overlays, notices, media placeholders, warning/error/info states, selection/focus states, disabled/unavailable states, and all settings-window surfaces.
- Transient interaction states may change resolved color properties for hover, pressed, active, selected, focused, disabled, warning, error, info, pending, streaming, and unavailable states, but they must not change widget geometry unless a specific widget contract explicitly permits it.
- A theme role may have a static role parent. Each role exposes a hardcoded supported style-property set from the UI role schema. A supported role-property pair's value source resolves to a concrete value from an inline concrete value, the same supported property on the static role-parent chain, the runtime ambient parent, or a built-in fallback after validation. Unsupported properties do not inherit into a role. Roles that render single-color separators expose `color`, not text, font, or border properties.
- Runtime ambient inheritance is used for embedded content whose surrounding visual context changes by render site. For example, an inline code role can define a concrete foreground while inheriting its background from the final-answer, user-input, settings-row, or popup context in which it is rendered.
- The active settings window uses the same theme resolver as the main workspace window. It does not own a separate visual schema.
- A visual constant may remain outside a named role only when it is not user-visible, is derived from a themed property, or is explicitly documented in this file as fixed geometry rather than appearance.

## Button Geometry

- Beryl-owned buttons share one app-wide button geometry contract independent of primary or secondary color roles.
- Button outer height is one shared command-control height that may exceed the standard UI text line height when doing so improves button legibility and still fits inside fixed chrome strips.
- Button labels use the standard UI font family, a shared button-label size, a shared button-label line height, and the active button role's font weight.
- Internal padding between the button label and the visual border is centralized separately for vertical and horizontal axes so the label has enough visual space while the outer height remains fixed.
- Button text labels may determine button width, but text buttons and icon-only buttons share the same outer height and corner shape.
- Buttons whose visible text comes from a known finite cycling or toggle label set must reserve width for the longest known label in that set so state changes do not move neighboring controls.
- Button geometry is invariant across normal, hover, pressed, active, and disabled visual states. Interaction states must not change width, height, padding, border width, font size, line height, font weight, transform, shadow, or flex sizing.
- Action rows that directly execute a command are buttons for this contract even when they appear inside popups or lists.
- Selector rows, data rows, status messages, and the active thread-title selector are controls rather than command buttons. They may use list or title visual treatment, but clickable title-style controls in fixed chrome must align to the shared button height, label typography, and corner shape where applicable.
- Rounded corners for Beryl-owned buttons and other rounded Beryl-owned widgets come from one shared corner-shape value unless a specific widget contract explicitly requires square edges.

## Settings Window

The settings window is a dedicated top-level OS window separate from the main workspace window. It is created ahead of first use and hidden when inactive so opening settings does not pay first-use window construction cost on the click path. It does not include the main workspace toolbar strip.

The English layout contract in this section is authoritative for implementation. Visual mockups are review companions only; behavior, resizing, hierarchy, and state rules are governed by this text.

### Settings Window Layout

- Anchors: fills the settings OS window.
- Default size: the settings window opens at the minimal useful supported width and height that can show the sidebar, page title, and at least one settings group without clipping pinned controls. The default width should be close to the minimum supported width, not a roomy desktop-wide layout. The minimum size must preserve the sidebar, page title, current page content, and any active apply or modified-theme action area without clipping pinned controls.
- Automatic resize: the root settings layout stretches horizontally and vertically with the OS window. The sidebar keeps a fixed logical width within its documented minimum and maximum; the main pane takes the remaining width.
- Sidebar width: the sidebar targets 270 logical pixels, may shrink no smaller than 220 logical pixels, and may grow no wider than 320 logical pixels when text scale or localization requires it.
- Main pane width: the main pane stretches to the remaining width. Settings row groups use a readable maximum content width while keeping page headers and page-level action areas aligned to the same content column.
- Root layout: a left settings sidebar and a right settings page stack. The settings window has no nested left tree, no second persistent navigation column, and no main-workspace toolbar.
- Sidebar layout: the sidebar contains the `Settings` title, a vertically scrollable broad-section list, and optional app/version footer metadata. Current broad sections are `Themes`, `Operations`, `Notifications`, and `Agent`. Sidebar rows may use real monochrome section icons when a Beryl-owned icon set is available; otherwise they are text-only. Generic discs, bullets, and status-dot placeholders must not precede every sidebar item.
- Sidebar selection: selecting a settings section replaces the right-pane page without mutating settings values. Sidebar rows do not expand into child rows. The selected sidebar section is indicated by selected-row background treatment plus a left-edge accent marker, not by a generic leading disc.
- Main pane layout: the main pane contains the current page header above a vertically scrollable page body.
- Page header: a root settings page shows a single title. A subpage shows breadcrumb text shaped as the full path from the root using `>` separators, such as `Themes > Default Dark`. Breadcrumb text is orientation only unless a breadcrumb segment is explicitly clickable.
- Page body: settings content is organized as section headings followed by grouped row lists. Group containers may visually frame a related row list, but settings pages must not nest cards inside cards.
- Settings row layout: each row has a left label stack and a right value/control area separated by a visible horizontal gutter at the supported minimum width. The label stack contains the display label, optional description, and either an always-visible stable setting id in compact secondary text or a row context action that can copy the stable id. The right value/control area uses type-appropriate stable widths and hugs the row's right edge. Extra horizontal space from wider settings windows belongs to the label stack, not to text fields, multiline fields, color inputs, file-picker action clusters, or row actions. Labels and descriptions wrap by words or normal text lines before controls shrink below their useful widths; they must not be horizontally clipped or forced into one-character vertical columns behind controls.
- Row value types: value areas may render switches, checkboxes, radio groups, segmented controls, dropdown selectors, steppers, sliders, text fields, multiline fields, color inputs, file path pickers, action buttons, or step-in navigation affordances. Numeric text fields use a compact fixed width sized for short numeric preferences, such as 6 to 7 digits, rather than the wider general text-field width. Rows that combine a single-line text field with row actions may stack the actions below the field inside one right-aligned control column so action labels do not take width away from the label stack at the supported minimum width.
- Step-in rows: a row that opens a deeper editor shows a right-facing thick triangle glyph `▸` in the value area and navigates the right pane to a settings subpage. The triangle must visually match the down-facing triangle family used by dropdown selectors. The sidebar remains unchanged.
- Modified state: rows whose staged or active value differs from the default render a stable modified indicator and expose reset through row context actions.
- Row context actions: every setting row exposes actions to copy its stable setting id and reset the setting when reset is valid. Rows may expose additional schema-owned actions such as copy serialized value.
- Apply behavior: settings actions such as Apply, Revert, Save, Save As, or Install Theme are part of the settings window's own chrome, page header, row value area, or a page-level action area rather than the main workspace toolbar.
- Overflow behavior: the settings window itself is not an outer scrolling surface. The sidebar owns vertical scrolling for section rows, and the current page body owns vertical scrolling for content rows. The page header and active apply or modified-theme action areas remain visible while the page body scrolls.
- Empty and failure states: unavailable settings sections, invalid staged values, failed saves, and failed theme operations render localized page or row feedback without replacing the settings window shell.

### Settings Themes Page

- The `Themes` page lists only durable installed themes. It does not list unsaved AI-generated theme candidates from Codex threads, and it does not expose Preview for installed themes.
- Installed theme rows show the theme name, stable theme id or copy-id action, active/modified state when applicable, and theme actions such as Activate, Rename, Delete, or Edit when those actions are valid.
- The active theme is visible at the top of the page body. The active theme row owns Save and Save As actions when the active theme has staged changes; those actions are absent or disabled when there are no staged changes. Save persists changes to the active installed theme. Save As asks for a new durable theme name and saves the staged active-theme definition as a new installed theme.
- Installed non-active themes switch the app by direct Activate. Activation is the settings-page equivalent of trying another installed theme; no separate installed-theme Preview action exists.
- Theme editing opens a settings subpage from the active theme row's Edit action rather than expanding nested controls in the sidebar. Edit uses the step-in triangle `▸`. The theme editor subpage uses an internal theme role navigator and property editor inside the right pane.
- AI-generated unsaved theme candidates enter durable Beryl settings only through the theme candidate code panel's Install Theme action or a theme dynamic tool operation that explicitly installs a durable theme.

### Settings Theme Editor Subpage

- The theme editor is a settings subpage reached from the active theme row. The left settings sidebar remains unchanged with `Themes` selected; the editor is not represented as a nested sidebar row.
- The theme editor page header uses the standard subpage breadcrumb pattern with text shaped as `Themes > <theme name>`.
- Save and Save As actions for modified active-theme drafts may appear in the theme editor page header as well as on the active theme row. They are absent or disabled when there are no staged changes. The default and minimum settings-window sizes must keep the page-header Save As action reachable without horizontal clipping.
- The editor body has two vertical regions: a bounded top theme role navigator and a lower selected-role property editor. This layout is local to the editor content area and must not become a second persistent settings navigation column.
- The theme role navigator presents the actual UI role schema tree as horizontally arranged columns. The first column contains the root role entry, and selecting a role opens the next column for that role's schema children. Every navigator row is a real UI role id from the schema tree; synthetic grouping labels, folder rows, or other non-role navigator items are invalid and must be treated as a design violation.
- The theme role navigator selection is stored and reconciled by role id rather than row index. Selecting a role changes only the property editor for the current page.
- The selected-role property editor shows the selected role id and one row per hardcoded style property supported by that role. Unsupported role-property combinations are absent from the editor and do not appear through inheritance. Role static parents are schema metadata displayed through the role navigator rather than free-form editor fields. Property rows expose both value-source selection, such as concrete value, static parent, ambient parent, or fallback, and the concrete value control when the selected source requires one. They do not add a per-row effective-value subtitle; resolved samples belong in separate presentation-only preview surfaces when useful.
- Dropdown source selectors use a down-facing thick triangle glyph visually matched to the step-in triangle family. Step-in navigation continues to use the right-facing thick triangle `▸`.
- The editor may show resolved samples for the selected role when useful, but samples are presentation-only and do not replace explicit property rows.

### Theme Candidate Code Panels

- A fenced transcript code block with language label `beryl-theme` is ordinary Codex thread content and renders through the shared code panel widget.
- In addition to ordinary code-panel copy, soft-wrap, selection, and scrolling behavior, a valid `beryl-theme` panel exposes Beryl-owned actions for `Preview` and `Install Theme`.
- `Preview` validates the candidate and applies it transiently without persisting it as an installed theme. A transient preview is controlled from the originating code panel, whose affordance can change to Stop Preview while that candidate is active. The Themes settings page must not list the unsaved candidate as a previewable theme.
- `Install Theme` validates the candidate and asks for confirmation of the durable theme name before writing it into the Beryl theme repository. Installing from the code panel saves a durable installed theme; activation remains an installed-theme operation after the theme exists.
- Malformed, unsupported, partial, or invalid `beryl-theme` code blocks render bounded panel-local validation feedback and must not mutate active theme state, settings drafts, or the theme repository.
- Theme candidate code-panel actions do not create synthetic transcript rows. The durable proposal record is the original Codex transcript code block, and the durable installed record is the saved Beryl theme.
- Beryl theme dynamic tools may provide model-facing authoring guidance and non-mutating document validation, but those tools do not add transcript content, do not create settings-window candidate rows, and do not replace the ordinary `beryl-theme` code panel as the operator-review surface.
- Model-facing theme guidance describes existing theme syntax, source keywords, static role-tree organization, value-source resolution, transcript/code/settings roles, candidate workflow, and common troubleshooting. Theme schema reads remain the structural source for exact role and property inventories.

### Settings Color Input

- Color-valued settings use a dedicated single-line color input that displays canonical `#rrggbb` text.
- The color input shows a preview swatch for the latest valid color value associated with that setting.
- Activating the preview swatch opens an in-window color picker for that setting.
- A field hotkey may also open the color picker while the color input is focused.
- The color picker is layered inside the settings window and edits the staged settings value through the same field-change path as text input.
- If a color text draft is temporarily invalid, the preview swatch and picker channel values continue to use the latest valid color for that setting until a new valid color is staged.

### Settings Operations Section

- The `Context compaction timeout` row is a compact key-value text row whose value is a whole number of seconds. Its value field uses the numeric compact width sized for short second counts, and the label/description side keeps a visible gutter from that field at the minimum settings-window width.
- Invalid timeout drafts render field-local validation feedback and are not applied.

### Settings Agent Section

- The `Developer Instructions` row shows smaller secondary subtext under the label: `Sent as developer instructions with every user message.`
- The `Developer Instructions` multiline field keeps a useful fixed right-side width at the default settings-window size, takes width from the label side before shrinking below that width, and leaves the label/subtext area wide enough for normal word wrapping at the minimum settings-window width.

### Settings Notifications Section

- The `End-turn sound` row keeps both `Choose...` and `Clear` actions reachable at the default and minimum settings-window sizes without horizontal clipping. The row keeps its label readable at the minimum width even when the staged sound file name is long, using the shared text-plus-actions control column when needed.

## Main Workspace Window

The main workspace window is a pinned toolbar strip above a workspace body and a fixed status line strip anchored to the OS window bottom edge. The workspace body contains a thread strip above a left conversation column and, when visible, a right checklist sidebar separated by a draggable sidebar splitter. The conversation column is itself a vertically stacked layout with a stretchable transcript region, an optional activity panel, and a pinned user input panel above the status line strip.

- A freshly created workspace renders through the same main workspace window composition as an initialized workspace on a pending new-thread draft.
- Runtime, member, or backend-availability recovery states may disable submission or show localized recovery information, but they do not replace the main workspace window with a separate fresh-startup shell.
- When the current primary runtime target is backend-unavailable, the main workspace window remains mounted, the workspace picker remains usable, and localized recovery text identifies the affected runtime target.

### Toolbar Strip

- Anchors: top, left, and right edges of the OS window.
- Automatic resize: stretches horizontally with the OS window.
- Automatic vertical behavior: fixed height.
- Manual resize: none.
- Overflow behavior: toolbar content must remain within the strip; controls may wrap, clamp, or collapse into simpler presentation, but the strip itself does not become a scrolling region.
- The toolbar strip uses the same fixed height as the thread strip and centers shared-height buttons with the same tight vertical spare space used around the `New Thread` button.
- The toolbar is a controls-only row and does not reserve a static leading text/content area.
- The main workspace window toolbar includes a workspace-picker button that opens the merged workspace picker popup.
- The toolbar includes an `Activity` mode control for the activity panel.
- The toolbar does not render persistent static workspace-name text, a thread-count label, a visible graph-overlay shortcut label, or non-interactive status chips.

### Thread Strip

- Anchors: top edge to the bottom edge of the toolbar strip, left and right edges to the OS window.
- Automatic resize: stretches horizontally with the OS window.
- Automatic vertical behavior: fixed height.
- Manual resize: none.
- Overflow behavior: strip content must remain within the strip; long thread labels truncate rather than causing outer scrolling.
- The thread strip uses the same fixed height as the toolbar strip and centers shared-height buttons with tight vertical spare space.
- The strip includes a `New Thread` button before the active thread title. Activating it clears the active thread selection without creating a backend thread until the next submitted user input fragment.
- The `New Thread` button is disabled when the current primary runtime target is backend-unavailable.
- The strip does not show the default host-Windows runtime as a persistent label; non-host runtime context may be shown when needed for the current execution target.
- The active thread title is a clickable selector control, not a command button. It opens the thread selector and aligns with the shared button geometry without needing the full resting border/background treatment of `New Thread`.
- The active thread title control is disabled and uses disabled control styling when the selected or primary runtime target needed for thread activation is backend-unavailable.

### Conversation Column

- Anchors: top edge to the bottom edge of the thread strip; bottom edge to the top edge of the status line strip; left edge to the OS window; right edge to the sidebar splitter when the checklist sidebar is visible, otherwise to the OS window.
- Automatic resize: stretches horizontally and vertically to occupy the remaining workspace body area not used by the sidebar splitter and checklist sidebar.
- Manual resize: not directly, but its width changes when the visible sidebar splitter is dragged.
- Overflow behavior: the conversation column itself is not a scrolling surface; its child transcript region, activity panel, and user input panel follow their own rules.

### Surface Notice

- Visibility: hidden by default and shown only while a localized notice is active.
- Anchors: top-right inside the main workspace window, below the toolbar and thread strips.
- Automatic resize: keeps a bounded width and constrains long notice text within the notice surface rather than pushing pinned workspace controls off-screen.
- Queue behavior: notices are queued in arrival order in a bounded queue, but the workspace renders at most one active notice popup at a time.
- Visual hierarchy: each notice renders a title line followed by optional detail text. Notice title, detail text, background, border, and warning/error/info variants resolve from active theme notice roles.
- Text interaction: notice text is selectable and standard copy commands copy the selected notice text to the system clipboard.
- Closing behavior: the notice exposes a visible close affordance that dismisses only the current notice, advances to the next queued notice when one exists, and does not mutate transcript, workspace, backend, graph, or persistence state.

### Transcript Region

- Anchors: top edge to the top edge of the conversation column; bottom edge to the top edge of the activity panel when it is visible, otherwise to the top edge of the user input panel; left and right edges to the conversation column.
- Automatic resize: stretches horizontally and vertically to occupy the remaining space between the pinned strips.
- Manual resize: none directly.
- Overflow behavior: owns internal vertical scrolling for transcript content.
- Internal transcript presentation: one chronological parent conversation surface without a separate `Transcript` title strip.
- Transcript content behavior: the transcript shows ordered user input fragments and parent assistant narrative items, including parent commentary, final answers, and optional parent-turn reasoning summaries.
- User-fragment behavior: each accepted composer send-and-clear event renders as its own user block, even when multiple user blocks belong to the same backend turn.
- User-fragment image behavior: accepted and historical user blocks preserve intra-fragment order between text and image markers. Image markers remain compact atomic labels such as `[A]` in the user block rather than inline thumbnails. The first marker for a distinct image carries the submitted image content for backend delivery and local preview; later markers for the same image are references to that same content.
- User-fragment image interaction: activating a transcript image marker opens the image preview popup when Beryl has durable image bytes for that marker. If bytes are unavailable, Beryl reports the unavailable preview state without replacing the marker with plain text.
- Generated-image behavior: native app-server generated-image output renders as transcript media content. Pending generation may show a stable media placeholder, and completed generation renders the raster image once embedded bytes or a readable saved path is available.
- Markdown-image behavior: Markdown image syntax with a supported local raster target renders as transcript media content. Unsupported formats, unavailable files, and paths rejected by runtime/member policy render their textual fallback in transcript order. Images nested inside ordinary Markdown links remain link content instead of media content.
- Media-run layout: one transcript media item occupies a full transcript row. The item uses the available transcript content width inside the same horizontal side padding used by surrounding transcript rows, but its rendered logical width must not exceed its natural raster pixel width after the active window scale factor is applied; when narrower than the padded content width, the item is horizontally centered in that row. Consecutive media items render side by side in one row and wrap at the right edge within the same padded transcript content width. Each item in a multi-item media run uses the same target logical width: the measured horizontal advance of 30 `M` glyphs in the active regular conversation text font, capped per item by that item's scale-adjusted natural raster width so Beryl does not upscale smaller images. Consecutive Markdown image embeds separated only by whitespace or line breaks are extracted to this media row even when their paragraph also contains prose, with surrounding prose rendered as normal text rows before and after the media row.
- Media-run promotion behavior: primary-clicking a loaded image inside a multi-item media run toggles that item into a promoted single-image row at its original transcript position. Items from the same source run before the promoted item continue to render in compact preview layout before it, and items after the promoted item continue to render in compact preview layout after it, including when only one non-promoted item remains on one side. Primary-clicking the promoted item clears the promotion and restores the original compact multi-item run layout. Promotion is UI-local presentation state and is cleared when its stable media target is no longer present in the loaded transcript window.
- Media-item context-menu behavior: when no transcript text selection is active, secondary-clicking a loaded transcript media item opens the transcript turn context menu for the parent turn and adds `Copy image` and `Save image as` for the clicked image. The existing turn actions remain present and keep their ordinary target, availability, disabled reasons, and commit behavior. The image actions target only the clicked media item.
- Media-item clipboard behavior: activating `Copy image` writes the clicked raster image to the system clipboard as image data and does not replace transcript text-selection copy semantics.
- Media-item save behavior: activating `Save image as` opens a native save destination picker for choosing the output directory and file name. After the picker returns a destination, Beryl writes the image bytes off the `gpui` thread and reports write failure without mutating transcript state.
- Media-item unavailable behavior: pending media placeholders, textual unsupported/unavailable/path-rejected fallbacks, and non-image transcript areas do not expose `Copy image` or `Save image as`; their secondary-click behavior follows the ordinary transcript turn context-menu rules.
- Operational-detail behavior: subagent transcripts, command execution records and output, file-change records, tool or MCP calls, maintenance turns, status notifications, token updates, and raw backend lifecycle events are not rendered as transcript rows. Native generated-image media output is an exception because it is assistant-produced transcript content.
- Operational-placeholder behavior: the transcript does not render placeholder rows solely to indicate that commands, tools, subagents, or other background work ran.
- Turn activity caret behavior: while the selected thread has an active parent turn, the transcript renders a block activity caret at the end of the parent conversation narrative.
- Turn activity caret interaction: the caret is not user-controllable, selectable, copyable, quoteable, or treated as part of the draft caret system.
- Turn activity caret layout: the caret has fixed geometry while blinking, does not cause text reflow, and disappears when the parent turn is no longer working.
- Turn activity caret motion: blinking follows the platform text-caret blink policy when available; if platform text-caret blinking is disabled, or if only a general reduced-motion signal is available and it requests reduced motion, the caret renders steadily.
- Text selection behavior: rendered transcript text supports normal text selection for clipboard copying without entering a custom quote-only mode.
- Text selection persistence behavior: ordinary transcript scrolling, live remeasurement, and viewport-window virtualization preserve the logical selected text while the selected transcript content remains in the loaded transcript window.
- Text selection viewport behavior: portions of a logical selection that are outside the current presentation window may have no visible highlight until they are rendered again.
- Text selection highlight behavior: visible highlight rectangles must match the currently rendered portions of the same logical selection range used for clipboard copying and quote harvesting, including selections that start or end inside a soft-wrapped visual line.
- Clipboard behavior: standard copy commands copy Markdown-preserving selected transcript text to the system clipboard without adding Markdown quote prefixes.
- Clipboard image-marker behavior: selected transcript image markers copy as explanatory text such as `[Image A]` while remaining atomic in on-screen selection and highlight behavior.
- Clipboard Markdown behavior: copied transcript text preserves Markdown syntax for selected semantic constructs instead of copying only the rendered presentation text.
- Quote-popup behavior: when the user selects non-empty transcript text and the selection stabilizes, Beryl shows the transcript quote popup near the selection; this popup is separate from the application toolbar and from transcript turn chrome.
- Quote-popup actions: the initial popup contains a `Quote` action that inserts the Markdown-preserving selected transcript text into the user input draft as Markdown block quote text.
- Quote transformation: selected logical text lines are prefixed with `> `, visual soft wrapping does not create additional quoted lines, and the inserted quote block is separated from surrounding draft text by blank-line spacing.
- Quote insertion point: the `Quote` action inserts at the last known draft caret position; if no draft caret position is known, it appends at the end of the draft.
- Quote harvesting behavior: after a quote insertion, the saved draft caret position moves to the end of the inserted quote block so repeated quote actions gather multiple quoted passages in draft order.
- Reading-continuity behavior: quote insertion preserves the transcript viewport and does not require moving focus to the user input field, so the user can keep reading a long response while collecting quotes.
- Quote clipboard behavior: the `Quote` action does not mutate the system clipboard.
- Quote-popup closing behavior: the quote popup closes when the transcript selection clears, when the user clicks outside the popup, when `Escape` is pressed, when transcript scrolling or virtualization leaves no stable current selection geometry, or after an accepted quote action.
- Turn context-menu opening behavior: when no transcript text selection is active, secondary-clicking a rendered area that maps to one loaded parent conversation turn opens the transcript turn context menu near the pointer.
- Turn context-menu actions: the menu contains `Edit message`, `Branch and switch to`, and `Branch in background` when those actions are available for the targeted turn.
- Turn context-menu media augmentation behavior: when the menu is opened from a loaded transcript media item, it is also a transcript media context menu and includes the media-specific `Copy image` and `Save image as` actions for that item.
- Turn context-menu edit-disabled behavior: when the targeted turn is otherwise edit-capable but the composer draft is non-empty, `Edit message` remains visible as a disabled row with the tooltip `Composer must be empty to edit a message`.
- Turn context-menu target behavior: the menu target is the whole parent turn that owns the clicked rendered area, regardless of whether the click lands on that turn's user input or assistant narrative.
- Turn context-menu disabled behavior: the menu is not opened for empty transcript space, released history placeholders, operational activity, the activity caret, or transcript selections.
- Turn context-menu closing behavior: the context menu closes on outside click, `Escape`, edit or branch action acceptance, active-thread change, or loss of a stable target turn in the loaded transcript window.
- Thread-edit preview behavior: while thread-edit mode is active, the targeted turn and all later loaded transcript turns render dimmed. No transcript rows are hidden, removed, reordered, or replaced until edit commit succeeds.
- Thread-edit selection behavior: dimmed turns remain ordinary rendered transcript content for selection, copying, quoting, and scrolling until commit removes them from backend history.
- Thread-edit cancel behavior: `Escape` cancels thread-edit mode after higher-priority popups, menus, and modal surfaces have handled the key. Canceling edit mode removes the dimming and restores ordinary transcript presentation without mutating the composer draft.
- Thread-edit invalidation behavior: active-thread changes, workspace changes, loss of the stable target turn, or transition into a selected-thread active/compacting/activation state cancels edit mode without mutating the composer draft.
- Presentation-window behavior: the transcript renders only the visible turn rows plus a small overscan margin for the current frame, even when additional fetched history pages are retained for navigation.
- Scroll-performance behavior: ordinary transcript scrolling must not rebuild widgets, scan nested transcript state, or clone turn records for every fetched turn in the active thread.
- Turn chrome: transcript turns do not render outer turn cards, turn-number rows, or user or assistant header rows; bordered treatment is reserved for specific content blocks such as user input fragments or Markdown code panels.
- Pending thread activation behavior: while an existing thread is being reopened or its initial transcript page is loading, the transcript region shows a visible pending state with the target thread label rather than leaving the old transcript looking idle.
- Loaded-thread viewport behavior: when an existing thread history window is loaded without a submit-time anchor, the transcript viewport defaults to the real end of the loaded window and still preserves trailing scroll allowance for the latest loaded user input fragment when content below that fragment line is too short to fill the viewport.
- Historical pagination behavior: older unloaded turn pages are requested as the user scrolls toward the top of the loaded transcript window, and the transcript shows a compact loading affordance while an older page request is pending.
- Historical retention behavior: fetched history pages may be retained or released as transient presentation data, but releasing offscreen presentation data must preserve chronological scroll geometry, row identity for retained visible rows, and the ability to fetch missing history again when the user navigates back to it.
- Historical user-fragment behavior: backend historical user-message content boundaries are preserved as separate user blocks instead of flattened into one prompt string.
- Submit-time viewport anchoring: when a user input fragment is accepted and inserted into the transcript, the transcript viewport is positioned so the last rendered line of that fragment is the first visible line at the top of the transcript region.
- Streaming behavior after submit: while the submit-time forced anchor remains active, the assistant response streams below the anchored user-fragment line into the remaining visible transcript space; response content that exceeds that space may overflow below the visible viewport without forcing automatic scrolling.
- Anchor overflow behavior: if the accepted user input fragment is taller than the transcript region, only the last rendered line of that fragment is pinned at the top and earlier fragment lines are above the viewport.
- Anchor slack behavior: after a user input fragment is accepted or an existing thread history is loaded, the transcript uses the shared virtual trailing scroll allowance to let the user scroll until the latest fragment's last rendered line reaches the top when real response content below that line is too short to make that position naturally reachable.
- Anchor slack shrink behavior: as response content below the anchored fragment line grows, virtual trailing allowance shrinks and disappears once real content completely fills the viewport below that line.
- Manual-scroll behavior: if the user manually scrolls the transcript during or after a turn, the transcript honors that scroll position and stops forcing the submit-time anchor, but it preserves the dynamic trailing scroll allowance until real response content no longer needs it or a later accepted draft replaces the anchor.

### Activity Panel

- Visibility: controlled by the `Activity` toolbar mode.
- Mode labels: the toolbar control cycles through `Activity Auto`, `Activity On`, and `Activity Off`.
- Default mode: new workspace UI state uses `Activity Auto`.
- Auto behavior: in `Activity Auto`, the panel is visible from the moment a parent turn is accepted on the conversation surface until that turn ends, and while selected-thread context compaction is active. It is hidden outside those active-work periods.
- On behavior: in `Activity On`, the panel is visible between the transcript region and the user input panel even when it currently has no rows.
- Off behavior: in `Activity Off`, the panel is hidden and consumes no conversation-column height.
- Persistence behavior: the toolbar activity mode and panel height are persisted as workspace-scoped GUI-local state across app restarts.
- History behavior: activity rows are in-memory session history. They survive thread switching within the loaded workspace and are discarded on app restart or workspace/backend-session reset.
- Visible row scope: the panel shows rows for the selected backend conversation thread and observed subagent activity owned by that selected thread. When the workspace is on a pending `New Thread` draft, the panel shows no rows.
- Anchors: left and right edges of the conversation column; bottom edge to the top edge of the user input panel.
- Automatic resize: stretches horizontally with the conversation column.
- Automatic vertical behavior: uses the persisted panel height, clamped to preserve the transcript region and user input panel minimum heights.
- Manual resize: dragging the panel's top border changes the panel height, taking vertical space from or returning vertical space to the transcript region.
- Overflow behavior: if the visible row set exceeds the panel's current height, the panel owns vertical scrolling for its rows; otherwise it does not scroll.
- Row virtualization: row rendering is bounded to the viewport-visible row range plus small overscan while preserving scroll geometry for the full visible row set.
- Initial scroll behavior: the row viewport defaults to the top of the sorted row list, where running and newest activity appears.
- Row layout: each row renders on one line as `Agent <agent label> Activity <activity display value>`.
- Agent label behavior: parent-thread rows may show `Main` without model or reasoning metadata. Resolved subagent rows show backend-provided nicknames after resolution; if exact child-thread model metadata is known from the activity projection, they show `nickname (model)`, and if exact reasoning effort metadata is also known, they show `nickname (model/reasoning)`. Subagent rows stay nickname-only when exact model metadata is unavailable and keep the agent value empty while the nickname is unresolved. Known non-subagent thread display labels may be shown only when they are real user-facing labels rather than generated from backend ids, and they do not receive subagent model/reasoning suffixes. Backend thread ids are never rendered as agent labels, rows update when nickname or exact model/reasoning metadata arrives after the row was first observed, and missing model/reasoning metadata is not inferred from defaults, model-list metadata, thread ids, or nicknames.
- Row status marker: each row starts with a disc that indicates state. Running, finished-ok, and finished-error marker fill and border values resolve from active theme status marker roles.
- Row typography: `Agent` and `Activity` use muted status-label text styling; the agent label and activity display value use status-value text styling.
- Row wrapping: row text does not wrap; long agent labels and activity display values truncate within the available row width.
- Row ordering: running rows sort before finished rows, and rows within each running or finished group sort by start time with the newest started row first.
- Row lifetime: each observed backend turn activity item remains in the in-memory row list after it finishes, with its final ok or error state.
- Activity naming: rows render protocol-derived activity display values without broad human-friendly mapping. For `commandExecution`, the activity display value is the first non-empty line of the spawned command, falling back to `commandExecution` when unavailable. Before display, if the first quoted or unquoted command token case-insensitively matches a drive-rooted Windows PowerShell launcher path shaped as `[drive]:\Windows(\.old)?\System32\WindowsPowerShell\v1.0\powershell.exe`, including the activity-log form with doubled backslashes such as `"D:\\Windows.old\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"`, that token is replaced with `powershell.exe` while preserving the rest of the command line. Reasoning rows render `reasoning` and, when backend summary text is exposed, update to a bounded single-line `reasoning: <summary>` value. `fileChange` rows render `Patching <relative/path>, +A -D` only when explicit backend file-change records identify exactly one unique path and that path is relative or can be proven to be under the selected conversation execution target root; otherwise they render `Patching N file(s), +A -D`. Other activity rows use raw protocol-derived tool names or resource identifiers.
- Row content boundary: rows do not include output, progress messages, resource contents, file paths other than the single relative `fileChange` path described above, patch diffs, raw reasoning content, or expanded operational detail.
- Data behavior: rendering reads the latest selected-thread activity projection from in-memory activity history and does not synchronously query `codex app-server`. Exact subagent model/reasoning metadata may be carried by normalized activity events or later read-only metadata responses; unresolved subagent nicknames may be resolved by background backend maintenance requests and applied to the projection later.

### Code Panel Widget

- All code-like presentation blocks, including transcript Markdown code blocks and diagnostic command, output, or patch panels, must be rendered through the shared code panel widget.
- The shared code panel widget is a reusable Beryl-owned presentation component whose supported chrome, wrapping, scrolling, selection, header-action, and resize modes remain composable for all code-like callers.
- The shared code-panel presentation boundary accepts plain text plus an optional language or syntax label.
- Code-panel callers route supported labels through the shared Beryl-owned off-render syntax lookup before rendering, and the render component consumes precomputed highlight output.
- Syntax highlighting is parser-backed and source-preserving: parser output assigns token roles to source ranges, and code-panel rendering maps those roles through application appearance settings.
- Languages or labels without a registered parser render as plain text using the configured code appearance.
- The widget supports an inline mode for unboxed transcript text fragments and a bordered mode for standalone panels.
- The widget's own copy action copies bare plain text; transcript selection that spans a Markdown code block may copy fenced Markdown source through the transcript selection path.
- The widget supports smart-wrap and no-wrap presentation modes.
- Smart-wrap mode prefers line breaks on spaces, commas, and semicolons before forcing a split at the last possible symbol that fits the current width.
- No-wrap mode enables horizontal scrolling instead of inserting soft line breaks.
- The widget may expose an optional header strip with generic small actions on the left and right; supported actions include `Expand`, `Collapse`, `Soft Wrap`, and `Copy`.
- In bordered mode, the widget may expose a draggable lower edge so the user can vertically resize the panel within the surrounding layout bounds.
- Any scrollable instance of the widget must use the shared app scrollbar affordance rather than an ad hoc local scrollbar.
- When a scrollable code panel widget is nested inside the transcript, pointer movement over the widget still drives the widget's own scrollbar visibility and fade affordance when the widget has overflow.
- A scrollable code panel widget nested inside the transcript does not take vertical pointer-wheel ownership merely because the pointer is hovering over it.
- Clicking a scrollable code panel widget nested inside the transcript selects that widget for vertical pointer-wheel ownership.
- While a nested scrollable code panel widget is selected, vertical pointer-wheel input over that widget scrolls only that widget and must not co-scroll the outer transcript region.
- Pressing `Escape` does not deselect a nested scrollable code panel widget for pointer-wheel ownership.

### User Input Panel

- Anchors: left and right edges of the conversation column; bottom edge to the top edge of the status line strip.
- Automatic resize: stretches horizontally with the conversation column.
- Automatic vertical behavior: automatically grows and shrinks to fit the draft's wrapped visual line count while preserving the panel decoration.
- Manual resize: none; the panel does not expose a draggable transcript/input separator.
- Minimum height: one line of text plus the surrounding panel decoration required to render the input cleanly.
- Maximum height: half the OS window height, further clamped as needed to preserve the transcript region's minimum height.
- Text wrapping: the input field wraps text at the available field width and does not own horizontal scrolling; text segments too long to fit on one line are force-wrapped within the field width.
- Inline image marker layout: pasted images render as compact textual markers such as `[A]` inside the draft at their insertion positions. Markers use the same line box as surrounding text and may wrap with surrounding text, but they do not render image thumbnails inline.
- Width-change behavior: changes to OS window size, conversation-column width, or draft content remeasure the wrapped line count and may grow or shrink the panel within its height bounds.
- Overflow behavior: the panel itself stays pinned above the status line strip; when the wrapped draft content exceeds the panel's maximum height, the text-entry internals own vertical scrolling and keep the caret and active selection endpoint reachable.
- Field sizing: the visible text-entry field uses the panel's automatically computed height while preserving the panel decoration.
- The user input panel uses one composer layout for pending new-thread drafts and selected conversation threads. Submission-unavailable states must not add a separate disabled action button beside the composer.
- Backend-unavailable behavior: when the selected runtime target or pending-new-thread primary runtime target is backend-unavailable, the composer field renders disabled, does not accept submission, and preserves the current draft text, image markers, caret, selection, and undo history.
- Draft caret memory: the user input field preserves a last known insertion position that transcript quote actions can use even while the transcript has focus.
- External draft insertion behavior: transcript quote insertion updates the draft content and saved insertion position without forcing keyboard focus into the user input field.
- Image paste behavior: image clipboard paste inserts an image marker at the caret or replaces the selected draft range when the selected thread's next image label is known. Text-only fields continue to use ordinary text paste behavior.
- Image marker editing behavior: a composer image marker behaves as one atomic draft item for caret movement, selection, deletion, cut, undo, and redo. Backspace or Delete removes the marker when the caret is adjacent in the corresponding direction, and deleting a selected range removes any markers contained by that range.
- Image marker clipboard behavior: copying or cutting a selected image marker writes explanatory fallback text such as `[Image A]`, not the compact GUI-only marker `[A]`. Beryl-owned composer paste may restore image markers only from valid Beryl-private clipboard metadata attached to that fallback text.
- Image marker clipboard fallback behavior: if the clipboard has no valid Beryl metadata, if the metadata no longer resolves to a live transient payload, or if the visible clipboard text differs from the stored fallback text, the composer pastes ordinary text. Text shaped like `[Image A]` is never parsed into an attachment by itself.
- Image marker reference behavior: pasting copied Beryl image-marker metadata in the same label scope inserts another marker that references the same image payload and keeps the same label. Pasting into another thread or pending-new-thread label scope allocates fresh labels from that target scope when label readiness permits.
- Image marker labeling: newly pasted images receive stable labels from the selected thread's monotonic image-label sequence. Removed labels are not reused while the draft, accepted fragment, queued fragment, retry state, or selected thread may still be referenced by surrounding text. Multiple visible markers may share one label only when they reference the same image payload.
- Image-only submission behavior: a draft containing at least one image marker and no non-whitespace text is submit-eligible. If image asset storage, runtime path preparation, or backend serialization fails, submission is rejected and the draft text and markers remain intact.
- Image marker context menu behavior: primary clicking or otherwise activating a marker opens the composer image context menu for that marker.
- Image marker context menu actions: the menu contains `View` and `Remove`.
- Image preview behavior: `View` opens the image preview popup with a larger fitted preview of the original pasted image data. The popup is presentation-only, closes on outside click or `Escape`, and does not mutate the draft.
- Image removal behavior: `Remove` deletes the selected marker occurrence from the mutable draft through the same editing path as keyboard deletion. The associated image data remains while another marker references it and is dropped after the final marker reference is removed.
- Submit controls: the panel does not render a persistent `Run Turn` or submit button.
- Submission behavior: when the user input field is focused, `Enter` submits the current message.
- Edit-mode submission behavior: when thread-edit mode is active, focused `Enter` attempts edit commit instead of ordinary turn start, active-turn steering, or compaction-time queueing.
- Edit-mode cancel behavior: when thread-edit mode is active and no higher-priority popup, menu, or modal surface handles the key first, focused `Escape` cancels edit mode without changing the draft text, image markers, caret, selection, or undo history.
- Transcript turn-jump behavior: when the user input field is focused, `Ctrl+Up` and `Ctrl+Down` scroll the transcript region without moving the draft caret or changing the draft selection.
- Transcript turn-jump alignment: intermediate turn jumps align the target turn at the top of the transcript region when possible.
- Transcript turn-jump within tall turns: when the transcript viewport is scrolled within a tall turn, `Ctrl+Up` first jumps to the top of that current turn; a later `Ctrl+Up` jumps to the previous turn.
- Transcript turn-jump downward terminal step: when no later turn boundary exists, `Ctrl+Down` scrolls to the bottom of the transcript so repeated downward jumps can reach the end of a large final turn.
- Transcript turn-jump bounds: `Ctrl+Up` at the first reachable turn boundary and `Ctrl+Down` at the transcript bottom are no-ops.
- Composer history hotkeys: when the user input field is focused and thread-edit mode is inactive, `Alt+Up` browses older accepted composer submissions for the current conversation scope, and `Alt+Down` browses newer submissions before restoring the pre-browse draft.
- Composer history scope: history browsing uses GUI-local in-memory entries for the selected backend thread or pending-new-thread draft, does not load missing backend history, and does not submit or mutate transcript content.
- Composer history replacement behavior: selecting a history entry replaces the visible draft with an editable copy, remeasures the user input panel, scrolls as needed to keep the caret reachable, clears selection, and places the caret at the end of the restored draft.
- Composer history edit-mode behavior: while thread-edit mode is active, `Alt+Up` and `Alt+Down` leave the edit-mode draft unchanged.
- Composer history bounds: when there is no older or newer entry to browse, the hotkey leaves the draft, caret, selection, user input panel size, and transcript viewport unchanged.
- Accepted-submit behavior: once a non-empty draft is accepted for submission and added to the transcript, the draft field clears immediately.
- Active-submit behavior: if the selected thread has an ordinary active parent turn, an accepted draft is rendered immediately as a distinct user input fragment and delivered through active-turn steering when the active backend turn id is known.
- Compaction-submit behavior: if selected-thread context compaction is active, an accepted draft is rendered immediately as a distinct user input fragment and queued for the next backend turn instead of being sent through active-turn steering.
- Queued-submit behavior: multiple drafts accepted while waiting for a turn id, waiting for compaction completion, or recovering from a non-steerable active turn remain separate visible user blocks and are delivered in accepted order when Beryl can start or steer the appropriate backend turn.
- Rejected-submit behavior: if submission is rejected, such as for an empty draft, a backend-unavailable target, or another disabled submission state, the draft field is not cleared.
- Newline behavior: when the user input field is focused, `Shift+Enter` inserts a newline into the draft instead of submitting it.

### Status Line Strip

- Anchors: left, right, and bottom edges of the OS window.
- Automatic resize: stretches horizontally with the OS window.
- Automatic vertical behavior: fixed height.
- Manual resize: none.
- Separator: the strip uses the same edge-to-edge horizontal separator treatment as the toolbar strip.
- Cell layout: the strip contains three left-to-right cells for model/reasoning, context space left, and last-turn state.
- Model/reasoning: displays the selected thread's active or pending model together with the selected thread's active or pending reasoning effort. When the workspace is on a pending new-thread draft, it displays the explicit draft selection if one exists; otherwise it displays the current effective backend defaults that would be used for the draft's first submitted turn. It uses `Unknown` for values that are unavailable from thread state or backend configuration, and it does not infer effective reasoning from model-list menu defaults.
- Model/reasoning unavailable behavior: when the selected runtime target is backend-unavailable and no exact backend-derived values are already known, the cell displays `Unavailable` or `Unknown` without launching or probing a backend.
- Model/reasoning interaction: activating the cell opens the model/reasoning popup when a backend conversation thread is selected and idle, or when the workspace is on a pending new-thread draft. With an active selected-thread turn or backend-unavailable selected runtime target, the cell is non-clickable.
- Model/reasoning popup behavior: the popup lists backend-supported models and restricts reasoning choices to the selected model's supported reasoning efforts. Choosing a model or reasoning effort updates the selected thread's pending turn defaults, or the pending new-thread draft's first-turn defaults when no backend thread exists yet; the change applies to the next submitted turn for that thread and later turns, not to global Codex configuration.
- Context space left: displays a percentage only when the selected thread has exact token usage with a positive model context window.
- Context account limits: when exact account rate-limit status is available, the same cell appends the active-model short-window and weekly remaining percentages after the context percentage, for example `100% 5h 91% Weekly 98%` or `100% Daily 85% Weekly 45%`.
- Context account-limit typography: rate-limit labels such as `5h`, `Daily`, and `Weekly` use the same status-label color as `Context`, while their percentages use the status-value color.
- Context account-limit fallback: rate-limit segments are omitted independently when the corresponding exact rate-limit window or active-model bucket is unavailable.
- Context usage source: exact token usage may come from the latest selected-thread `thread/tokenUsage/updated` notification, from an in-memory same-thread cache populated by a previous notification, from a durable GUI-held last-known snapshot originally populated by a notification, or from read-only app-server status metadata when the protocol exposes it.
- Context formula: use the latest exact selected-thread token usage available to the GUI and compute `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`; do not use cumulative `tokenUsage.total` for this percentage.
- Context fallback: displays `Unknown` before exact token usage is available, when the model context window is missing or non-positive, or when the selected thread changes to a thread without known exact usage.
- Context refresh behavior: switching threads must not submit user input, start a backend turn, or mutate backend-owned conversation history to fill the context cell.
- Context interaction: activating the cell opens the context operations popup only when a backend conversation thread is selected, the selected thread is idle, and the selected runtime target is backend-available. With no selected thread, an active selected-thread turn, or a backend-unavailable selected runtime target, the cell is non-clickable.
- Context operations popup behavior: the initial popup contains `Compact`, which starts backend context compaction for the selected thread and then closes or reports request failure through normal popup feedback. The request acceptance response is not itself compaction completion.
- Last-turn state: displays `compacting` while selected-thread context compaction is active, `working` while a parent turn is active, `ok` after the latest completed turn, `error` after the latest failed or interrupted turn, and `Unknown` before any turn state is known.
- Last-turn interaction: activating the cell opens the turn operations popup only when the selected runtime target is backend-available and either the selected backend conversation thread has an active ordinary turn with a known interruptible backend turn id or selected-thread context compaction is active with a known interruptible backend turn id. Otherwise the cell is non-clickable.
- Turn operations popup behavior: the popup contains `Soft stop`, which requests backend interruption for the exact selected-thread active turn and then closes or reports request failure through normal popup feedback. Request acceptance is not itself terminal interrupted state.
- Hard-stop row behavior: when hard-stop targets are known and supported, the popup also contains `Hard stop`. Pressing and holding the row for three seconds activates the hard-stop request; while held, the row background fills from left to right to show progress. Releasing early, moving outside the row, closing the popup, focus loss, or selected active-turn target change cancels the hold. Keyboard activation must provide the same three-second hold affordance for the focused row.
- Hard-stop disabled behavior: when the selected active turn is soft-stoppable but no exact backend-exposed hard-stop target is known, the hard-stop row is disabled or omitted rather than pretending to terminate unknown tools.
- Hard-stop progress behavior: after the three-second hold completes, the row triggers exactly once, shows an in-flight state, and suppresses duplicate soft or hard stop submissions until the current stop request finishes or fails.
- Turn stop queue behavior: accepted user input fragments queued before or during a stop request remain visible and ordered. If they cannot be delivered to the interrupted turn, they remain queued for the next eligible turn.

### Diagnostic Child Control

- Diagnostic child-control commands do not add visible controls, persistent labels, panels, popups, buttons, or alternate interaction modes to the ordinary workspace UI.
- Diagnostic child new-thread, turn-submission, soft-stop, hard-stop, thread-listing, thread-activation, popup-closing, transcript-scrolling, and wait commands must drive the same internal application command paths and state transitions as the corresponding visible UI interactions or retained UI projections.
- Diagnostic child UI-state and command results must report backend-unavailable workspace state and backend-unavailable submission or thread-listing rejection distinctly from ready, opening, workspace-idle, and blocked shell states.
- Diagnostic child hard-stop requests are explicit supervisor commands and therefore do not render or require the visible three-second hold progress affordance, but they must still target only the exact selected active turn and supported hard-stop targets that the visible popup would allow.
- Diagnostic child wait commands observe UI state without creating loading UI solely for diagnostics and must return the latest bounded state on timeout.

### Checklist Sidebar

- Visibility: hidden by default and shown when the user requests it or when a checklist-capable node auto-shows it.
- Anchors: top edge to the bottom edge of the thread strip; right edge to the OS window; bottom edge to the top edge of the status line strip; left edge to the sidebar splitter when visible.
- Automatic resize: stretches vertically for the full workspace body height and uses a bounded right-edge width derived from the current splitter position and clamped to layout bounds.
- Manual resize: its left edge is draggable through the sidebar splitter to change the transcript/sidebar width split.
- Overflow behavior: owns vertical scrolling for checklist rows, does not own horizontal scrolling, and wraps checklist-item text within the visible sidebar width.
- Mutation behavior: checklist-affecting GUI or dynamic-tool graph mutations update the sidebar in place when the selected checklist remains valid.
- Reconciliation behavior: checklist row identity, sidebar visibility, and sidebar scroll are preserved by semantic identity across graph mutation commits and optimistic graph projections, pruning only rows or sidebar state invalidated by the graph change.
- Mutation-failure behavior: failed checklist-affecting graph writes report localized error or recovery state without clearing unaffected sidebar content.

### Sidebar Splitter

- Visibility: hidden when the checklist sidebar is hidden and visible only while the checklist sidebar is visible.
- Anchors: top edge to the bottom edge of the thread strip; bottom edge to the top edge of the status line strip; horizontally between the conversation column and the checklist sidebar.
- Automatic resize: stretches vertically for the full workspace body height.
- Manual resize: draggable horizontally to change the transcript/sidebar width split while respecting the minimum sizes of the conversation column, transcript region, checklist sidebar, and pinned user input panel.
- Overflow behavior: not a scrolling surface.

### Column Selector Widget

- Domain ownership: the reusable widget owns column-trail presentation, selected-row state, row expansion state when supplied by the caller, and scroll affordances; callers own the row domain model, row labels, row commands, and activation semantics.
- Column behavior: selecting a branching row truncates any columns to its right and opens the next column from the selected row's target.
- Terminal-row behavior: selecting a terminal row does not open a next column unless the caller defines that row as branching.
- Horizontal overflow behavior: the column selector container owns horizontal scrolling when the number of columns exceeds the visible selector width.
- Vertical overflow behavior: each column selector column owns its own vertical scrolling beneath a fixed column header.
- Header behavior: each column keeps a visually separated fixed-height header strip limited to one visible text line.
- Keyboard behavior: selector surfaces support `Escape` to close, `Up` and `Down` to move within the active column, `Left` and `Right` to move across available columns, and `Enter` to invoke the selected row's caller-defined activation behavior.
- Mouse behavior: single-click selects a row and may open the next column when the row is branching; double-click invokes the selected row's caller-defined primary action when one exists.
- Layering behavior: only one column-selector surface is interactive at a time; opening one closes other column-selector surfaces and their context menus.

### Thread Selector

- Visibility: hidden by default and shown from the active thread title control in the thread strip.
- Layering: appears above the main workspace window as a popup surface and remains clamped within the OS window bounds.
- Opening behavior: opening the thread selector closes the graph overlay and graph node context menu.
- Closing behavior: closes on outside click, on `Escape`, and after an accepted thread activation request.
- Automatic resize: uses the reusable column selector widget with bounded popup dimensions large enough for at least one member column and one thread column when space permits.
- Manual resize: none in V1.
- Overflow behavior: follows the column selector container and column overflow rules.
- Data behavior: renders from the latest member-thread inventory snapshot and does not synchronously query `codex app-server`.
- Refresh behavior: opening the selector may request a background member-thread inventory refresh only for backend-available runtime targets; stale snapshot content remains visible while the refresh is pending, unavailable, or failed.
- Backend-unavailable behavior: when the runtime target needed by the active thread selector is backend-unavailable, the selector trigger is disabled. If a selector is already open when a target becomes backend-unavailable, affected member or thread rows render disabled and cannot activate stale inventory rows.
- Single-member behavior: when the active workspace has exactly one available member, including the implicit home member case, the selector opens directly to that member's root/orphan thread column.
- Multi-member behavior: when the active workspace has more than one available member, the first column lists members and selecting a member opens that member's root/orphan thread column.
- Root/orphan thread behavior: the first thread column for a member lists threads whose backend fork parent is absent from that same member group, including threads with no parent metadata and threads whose parent is missing, filtered out, stale, malformed, or grouped under another member.
- Branch-column behavior: selecting a thread row with direct forks opens the next column to the right with those fork rows, and the same rule applies recursively for deeper fork chains.
- Initial-selection behavior: opening the selector preselects the active thread row when it appears in the latest snapshot, expanding the necessary member, root/orphan, and fork column path first.
- Member-row behavior: member rows show the member label and thread count.
- Thread-row behavior: thread rows show the manual GUI-local title when available, otherwise the backend-provided thread name, otherwise an untitled fallback while automatic naming is pending or unavailable. Rows with direct visible forks show a full-height separator and a rightmost direct-fork count cell; rows without direct visible forks do not reserve that trailing cell.
- Thread-lineage behavior: branch columns are derived only from backend-provided source thread ids. The selector does not show or infer the source turn, fork point, or full transcript lineage.
- Thread-sorting behavior: thread rows sort by newest backend update time in the row's visible branch subtree so a root with a recently updated fork remains near recent work.
- Refresh-reconciliation behavior: when a refreshed inventory snapshot changes available rows while the selector is open, the selected column path is reconciled by durable member and thread identity; invalid fork columns are pruned without activating or selecting a different thread.
- Active-row behavior: the currently active thread row is visibly highlighted.
- Activation behavior: double-clicking a thread row or pressing `Enter` on a selected thread row activates that exact Codex thread in the transcript, including when the row also opens a fork column on single-click. Activation is disabled for rows whose runtime target is backend-unavailable.
- Activation-pending behavior: after an activation request is accepted, the selector closes and the transcript region shows the pending thread activation state until the backend resume and initial transcript page load succeed or fail.
- Exact-selection behavior: if the selected thread is no longer available, cannot be reopened, is outside current workspace scope, or resumes with a recorded working directory that does not match the expected execution target, the workspace reports the standard rebind or activation failure path and does not activate a different thread.

### Graph Overlay

- Visibility: hidden by default and toggled independently of transcript visibility.
- Layering: when visible, the graph overlay floats above the transcript region instead of reflowing the main window layout.
- Opening behavior: opening the graph overlay closes the thread selector.
- Anchors: left edge to the conversation column; right edge to the conversation column; top edge to the bottom edge of the thread strip.
- Automatic resize: stretches horizontally with the conversation column and derives its initial height from the available conversation-column space beneath the thread strip.
- Automatic vertical behavior: opens at a bounded default height near the upper half of the visible conversation-column area beneath the thread strip.
- Manual resize: the bottom edge is draggable vertically to change the overlay height while preserving the visibility of the transcript region, pinned user input panel, and status line strip.
- Overflow behavior: the overlay popup itself does not own shared vertical scrolling; its internal header stays fixed while the explorer columns below it manage their own overflow. Underlying transcript content is not treated as the active interaction surface while the overlay is open.
- Internal layout: the overlay adapts the reusable column selector widget to semantic graph nodes, soft links, and thread refs.
- Root behavior: the first graph column begins with the graph's ordered root-level semantic nodes. Additional columns are opened from user selections rather than from a synthetic root-list domain object.
- Mutation behavior: when graph content is already available, ordinary GUI or dynamic-tool graph mutations keep the overlay body and explorer columns mounted. Full-body loading or recovery surfaces are reserved for startup, empty graph, or explicit authoritative refresh recovery.
- Status behavior: pending graph mutation status appears in compact header, row, or context-menu affordances instead of replacing the graph columns.

### Graph Columns Container

- Anchors: fills the visible graph overlay bounds.
- Reuse behavior: this is the graph overlay's domain-specific instance of the reusable column selector container.
- Automatic resize: stretches horizontally with the graph overlay and fills the remaining height beneath the overlay header.
- Manual resize: none.
- Overflow behavior: owns horizontal scrolling when the number of columns exceeds the available width.
- Column-trail behavior: selecting a node opens that node in the next column to the right, and continued traversal may extend the column trail beyond the visible viewport width. The first column remains the root-level entry column and may list more than one root-level node.
- Reconciliation behavior: column roots, horizontal scroll, and per-column vertical scroll are preserved by semantic identity across graph mutation commits and optimistic graph projections.

### Graph Column

- Visibility: one or more graph columns may be visible at once.
- Reuse behavior: each graph column is a domain-specific instance of the reusable column selector column.
- Automatic resize: columns may clamp or size to a shared preferred width, but all visible columns follow the same layout rules and fill the visible graph-columns height.
- Manual resize: none in V1.
- Header behavior: each graph column keeps a visually separated fixed-height header strip that is limited to one visible text line and does not show a summary or node counters.
- Row behavior: semantic-node rows are compact single-line rows; each row renders either `title` or `status symbol + title`, and node summaries are exposed through hover tooltips instead of consuming vertical row space.
- Tooltip suppression behavior: semantic-node summary tooltips are suppressed while any graph-node context menu is open.
- Context-menu behavior: right-clicking a semantic-node row opens that node's context menu without changing the active transcript thread.
- Thread-ref row behavior: valid thread-ref rows use the ordinary terminal row treatment. Invalid thread-ref rows remain visible, render a compact invalid-link indicator, and expose the invalid reason through a hover tooltip.
- Thread-ref activation behavior: activating a valid thread-ref row uses the same pending thread activation presentation as thread selector activation. Activating an invalid thread-ref row leaves the active transcript unchanged and reports the invalid reason through the standard localized notice path.
- Expand or collapse behavior: any node row with children shows a `+` or `-` control and is collapsible from that row.
- Overflow behavior: each graph column owns vertical scrolling for its own node, soft-link, and thread-ref rows beneath a fixed column header.
- Pending-row behavior: rows affected by pending local graph mutations may show pending, disabled, or dimmed state while unaffected rows remain visible and interactive according to the current graph-action policy.
- Invalidated-state behavior: if a mutation deletes or invalidates a selected row, only that row's selection and dependent columns are pruned; unrelated columns and scroll positions remain intact.

### Graph Node Context Menu

- Visibility: hidden by default and shown on right-click of a semantic-node row.
- Layering: appears above the graph overlay and remains clamped within the OS window bounds.
- Manual resize: none in V1.
- Overflow behavior: the menu and any submenu own vertical scrolling when their row content exceeds their bounded height.
- Internal layout: command rows are compact single-line rows.
- Disabled behavior: disabled command rows remain visible and expose the reason through a hover tooltip.
- Delete behavior: the menu includes a `Delete` command that immediately deletes the target semantic node only when that node has no hard children. The row remains visible but disabled with a reason tooltip when the target node has hard children.
- Delete-recursively behavior: the menu includes a `Delete Recursively` command that deletes the target semantic node and hard descendants only, without following soft links as additional deletion targets.
- Held-delete behavior: pressing and holding the `Delete Recursively` row for three seconds activates the deletion; while held, the row background fills from left to right to show progress.
- Held-delete cancellation behavior: releasing early, moving outside the row, closing the menu, pressing `Escape`, focus loss, or loss of the stable target graph node cancels the hold without deleting graph state. Keyboard activation must provide the same three-second hold affordance for the focused row.
- Delete-recursively completion behavior: after the three-second hold completes, the row triggers exactly once, shows an in-flight state, and suppresses duplicate graph mutation submissions until the current delete request finishes or fails.
- Mutation-failure behavior: if the graph mutation fails, the menu or nearby graph surface reports the failure locally and clears the in-flight state without blanking the overlay.
- Link-thread behavior: the menu includes a `Link thread` command.
- Link-thread single-member behavior: when the active workspace has exactly one available member, including the implicit home member case, `Link thread` opens directly into that member's thread-list submenu.
- Link-thread multi-member behavior: when the active workspace has more than one available member, `Link thread` opens a member-list submenu, and each member row opens that member's thread-list submenu. Unavailable explicit members do not appear in this submenu.
- Thread-list behavior: thread rows show only the thread display title, sorted by last-updated time descending.
- Empty-thread-list behavior: a member with no linkable threads shows a disabled `No threads` row in its thread-list submenu.

### Workspace Picker Popup

- Visibility: hidden by default and shown on demand as a popup widget rather than a full-screen replacement.
- Layering: appears above the main workspace window while leaving the underlying window intact.
- Anchor behavior: opens from the workspace-picker toolbar button and remains clamped within the OS window bounds.
- Automatic resize: preferred width `840 px`, clamped between `620 px` and `94%` of the OS window width, with maximum height `72%` of the OS window.
- Manual resize: none in V1.
- Overflow behavior: the popup itself does not become a general scrolling surface. The Workspaces column owns vertical scrolling for its divided workspace list when needed, and the Members column owns vertical scrolling for its divided member list when needed.
- Internal layout: two side-by-side content columns separated by a vertical divider. The left Workspaces column contains a header, a fixed filter field, and one vertically scrollable divided workspace list. The right Members column contains a header, a fixed default-runtime selector, and one vertically scrollable divided member list.
- Header behavior: column headers identify `Workspaces` and `Members` without item-count labels.
- Filter behavior: the Workspaces filter matches against workspace names and explicit workspace member paths shown in workspace rows, including unavailable attached member paths. Filtering changes which existing workspace rows are visible without moving the `Create new workspace` row out of the first list position.
- Create-row behavior: the `Create new workspace` row is part of the divided list, has no row action-menu trigger, and invokes workspace creation when activated.
- Workspace rows show the workspace name as the primary line and explicit workspace member paths as one secondary line per member, including unavailable attached member paths.
- Workspace-row action behavior: each ordinary workspace row exposes one row-edge action-menu trigger. The row action menu contains `Rename` and a dangerous delete action represented as a hold-for-action trigger.
- Workspace rows do not render implicit-home member paths or `last updated` metadata.
- Long workspace names and member paths soft-wrap and may grow the row vertically instead of truncating to ellipses.
- Rename action behavior: the active workspace row's rename action is disabled while workspace-scoped work is in progress or queued, and the disabled action exposes the reason through a hover tooltip.
- Active-row behavior: the currently open workspace row is indicated by its left-edge accent marker only. The row does not use full-row primary-blue highlighting, does not render redundant active/current label text, and activating it closes the popup without reloading the workspace.
- Members-column target behavior: the Members column targets the currently active workspace. Activating another workspace row switches workspaces and closes the popup rather than editing member registrations for that row in place.
- Closing behavior: the workspace picker closes on outside click, on `Escape`, and after accepted workspace row activation.
- Keyboard behavior: workspace picker rows do not support keyboard traversal or `Enter` activation in V1, and the popup does not render selected-row highlight state for keyboard navigation.

### Workspace Members Column

- Visual behavior: the Members column uses the same divided-list and left-edge accent row-state treatment as the Workspaces column. The member list's first row is the `Attach member` action row, followed by visible workspace members, including unavailable explicit members and any exposed implicit home member.
- Runtime-selector behavior: the default-runtime selector occupies the fixed row above the member list and also chooses the runtime for the next attached member. There is no independent member filter field.
- Runtime-dropdown behavior: activating the runtime selector opens an attached selector dropdown. The opened dropdown and trigger share one continuous outer boundary with aligned left and right walls. WSL distro selector rows are labeled with a `WSL: ` prefix.
- Runtime-enabled behavior: the runtime selector remains enabled when explicit workspace members exist because changing it affects future member attachment and implicit home fallback, not existing runtime-bound members.
- Backend-unavailable behavior: backend-unavailable state for the current primary runtime target does not disable the runtime selector, `Attach member`, detach actions, or `Make primary` actions for backend-available member paths.
- No-runtime behavior: when no default runtime environment is selected, the runtime selector is enabled and the `Attach member` row is disabled until the user chooses host-Windows or one WSL distro.
- Member-row behavior: each member row uses the same text hierarchy as a workspace row. The primary line is a display label derived from the member directory or implicit-home role, and the secondary line is the full filesystem path. Long labels and paths soft-wrap and may grow the row vertically instead of truncating to ellipses.
- Unavailable-member behavior: unavailable explicit member rows remain in the list. Their primary line appends `- path not found` to the normal display label, their secondary line remains the persisted full filesystem path, and they do not expose `Make primary`.
- Primary-member behavior: the current primary member is indicated by the same left-edge accent marker used for the active workspace. The row does not use full-row primary-blue highlighting and does not render redundant primary/current label text.
- Explicit-member action behavior: each explicit member row exposes one row-edge action-menu trigger. Non-primary member action menus include `Make primary`; explicit member action menus include a detach action that asks for confirmation.
- Implicit-home behavior: when no available explicit members exist, the list shows the default runtime environment's implicit home member as the current primary member and does not expose member actions that would detach it. Host-Windows uses the host user's home directory; WSL uses the selected distro's home directory.

## Scroll Ownership

- Beryl uses one shared app-wide scrollbar affordance rather than per-surface custom chrome.
- The shared scrollbar affordance is backed by reusable app-neutral scrollbar primitives that own chrome visibility and direct manipulation, while Beryl surfaces own the viewport routing and scroll-state semantics around it.
- Every Beryl surface that owns scrolling must render that shared scrollbar affordance.
- The shared scrollbar renders only a thumb; its full outline or track remains visually invisible.
- That scrollbar thumb appears only after pointer movement or active scrolling within the owning scrollable area and only when the surface currently has overflow.
- After pointer movement and scrolling both stop, the scrollbar thumb fades in and out around a short inactivity delay instead of appearing or disappearing abruptly, with render-frame-driven opacity interpolation while the transition is active.
- A scrollbar thumb is draggable by pointer press-and-hold on every rendered scrollbar axis. Dragging preserves the pointer's grab offset within the thumb until pointer release or cancellation.
- A vertical scrollbar owns an invisible interaction lane along the thumb's axis even though the track is not painted. Clicking that lane outside the current thumb performs one page scroll by the owning viewport height: clicks above the current thumb page upward, and clicks below the current thumb page downward.
- Direct scrollbar interactions must route through the owning scroll surface's scroll state.
- Shared scrollbar fade and activity timing must come from the reusable scrollbar affordance. Owning scroll surfaces report pointer movement, wheel scrolling, and other app-neutral viewport activity into that affordance instead of implementing their own opacity lifecycle.
- Keyboard scrolling commands act on the scrollable viewport selected by focus or shell routing, not on scrollbar chrome.
- Pointer-wheel and touchpad scrolling act on the routed scrollable viewport, while scrollbar thumb dragging and lane clicks originate from scrollbar chrome and call back into the owning viewport's scroll model.
- The main shell transcript owns transcript-specific top, bottom, bottom-following, and virtual-tail edge behavior. Other scroll surfaces use their own scroll bounds and do not inherit those transcript rules from the shared scrollbar affordance.
- Streaming scroll surfaces may opt into a shared virtual trailing scroll allowance that increases scroll range without adding a fake content child.
- A virtual trailing allowance is capped by the owning viewport and by the caller's visual anchor so the user may scroll into useful empty space only while at least one real content line remains visible for orientation.
- Scrollbar geometry for a virtual trailing allowance reflects the effective scroll range, but content item counts, visible item ranges, and preserved content anchors remain based on real content only.
- Virtual trailing allowance is provided by Beryl-owned scroll/list support layered on `gpui`; it is not implemented by changing `gpui`'s third-party list primitive.
- The transcript region owns normal vertical scrolling for the active conversation thread.
- Pointer movement over an overflowed scrollable surface is an affordance signal and may reveal that surface's scrollbar even when that surface does not currently own pointer-wheel scrolling.
- Scrollable code panel widgets own their own scrolling and use the same shared scrollbar affordance.
- Scrollable widgets nested inside the transcript must be selected by click before they consume vertical pointer-wheel scrolling.
- Vertical pointer-wheel scrolling over an unselected nested transcript widget remains owned by the transcript region.
- Selecting one nested transcript widget replaces any previous nested transcript widget selection, and clicking ordinary transcript space returns vertical pointer-wheel ownership to the transcript.
- Pressing `Escape` does not clear nested transcript widget selection for pointer-wheel ownership.
- Code panel widgets in no-wrap mode may own horizontal scrolling in addition to vertical scrolling when bounded height is smaller than content.
- The activity panel owns vertical scrolling only when the selected-thread activity row set exceeds its current height.
- The user input field does not own horizontal scrolling; it owns vertical scrolling only when wrapped draft content exceeds the capped user input panel height.
- The checklist sidebar owns normal vertical scrolling for its checklist rows and does not own horizontal scrolling.
- The theme editor role navigator owns horizontal scrolling when its role-column trail exceeds the visible navigator width.
- Each theme editor role navigator column owns normal vertical scrolling for role rows that exceed the visible column height.
- The selected-role property editor owns normal vertical scrolling for property rows that exceed its visible height.
- The graph columns container owns horizontal scrolling when explorer depth exceeds the available width.
- Individual graph columns own normal vertical scrolling for graph rows that exceed the visible column height.
- A column selector container owns horizontal scrolling when its column trail exceeds the visible selector width.
- Each column selector column owns normal vertical scrolling for rows that exceed the visible column height.
- The thread selector follows the column selector scroll ownership rules and does not make the main workspace window scroll.
- Popup widgets may own internal scrolling when their content exceeds their bounds.
- Context menu widgets may own internal scrolling when their content exceeds their bounds.
- The toolbar strip, user input panel, activity panel, and status line strip remain pinned rather than becoming general outer scrolling surfaces.

## Small-Window Behavior

- The workspace window must preserve the visibility of the pinned toolbar strip, thread strip, pinned user input panel, visible activity panel, and status line strip within the OS window.
- When the graph overlay is visible, it must remain bounded within the visible conversation column instead of pushing the toolbar strip, thread strip, checklist sidebar, or user input panel off-screen.
- When the thread selector is visible, it must remain bounded within the OS window instead of pushing pinned strips or the active transcript off-screen.
- The minimum OS window size for the main workspace window is derived from the minimum sizes of the toolbar strip, thread strip, conversation column, checklist sidebar when visible, transcript region, visible activity panel, user input panel, and status line strip.
