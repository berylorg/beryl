# Goals

Render backend-owned Codex conversation history as a responsive parent conversation narrative with Markdown semantics, selection, quoting, images, context menus, and bounded history loading.

## Non-goals

- Rendering operational activity, tool logs, command output, or subagent transcripts as transcript rows.
- Treating backend transcript history as GUI-owned durable state.
- Interpreting raw HTML as live UI content.
- Making Markdown image file bytes durable Beryl attachments.

# Decisions

## Transcript Model

- The transcript is the stable parent conversation narrative for the active backend thread.
- It shows ordered user input fragments plus parent assistant narrative items, including parent commentary, final answers, and optional parent-turn reasoning summaries when exposed by the backend.
- It excludes asynchronous or operational activity that is not parent narrative, including command execution records and output, file-change records, tool or MCP calls, subagent transcripts, title-generation maintenance turns, raw backend lifecycle notifications, status updates, and token-usage updates.
- Native app-server generated-image media output is assistant-produced transcript content and may render inline.
- Multiple user input fragments in one backend turn render as distinct user blocks rather than one merged prompt.
- Historical user-message content boundaries and item order exposed by the backend are preserved.
- User input fragments containing images preserve relative order between text and image markers. Transcript image markers are compact read-only atoms such as `[A]`.
- Backend conversation history remains backend-owned. Loaded transcript pages are transient presentation data.

## History Loading And Rendering Bounds

- Existing backend histories are loaded as bounded pages of turns. Opening an existing thread first renders the latest page at the transcript tail and loads older pages when the user scrolls toward earlier history.
- The loaded transcript window and presentation window are separate. Loading may retain multiple pages for navigation, but each render frame builds UI only from the visible viewport plus small overscan.
- Transcript scroll-frame work must not clone, scan, parse, or retain widget state for the whole loaded history.
- Offscreen pages may remain in a bounded cache, but cache retention must not make ordinary scroll frames process all loaded history.
- Releasing offscreen presentation data must preserve chronological scroll geometry, row identity for retained rows, and the ability to fetch missing history again.
- Pending activation of an existing thread renders a visible pending state with the target label rather than leaving the previous transcript looking idle.

## Markdown And Code

- Transcript Markdown is parsed into Beryl-owned semantic block and inline structures before rendering.
- Supported Markdown semantics include paragraphs, headings, unordered and ordered lists, block quotes, inline code spans, fenced code blocks, links, images, explicit line breaks, thematic breaks, emphasis, strong emphasis, and math spans or blocks.
- Unsupported or non-Markdown conventions render literally unless represented by the transcript Markdown model.
- Raw HTML is represented as literal source or unsupported source and must never be interpreted as executable or styled HTML.
- Fenced code blocks render through the shared code panel widget.
- Syntax highlighting is presentation derived from preserved source text and optional language labels. It must not replace backend-owned transcript text, Markdown copy source, or code-panel copy source.
- Parser output assigns token roles to source ranges; rendering maps token roles through application appearance settings.
- The Markdown language is a registered syntax-highlighting language. Unsupported, unknown, empty, partial, or invalid labels render as plain text while preserving source and selection semantics.
- `beryl-theme` fenced code blocks remain ordinary transcript content, with theme-specific actions owned by `doc/features/theming/design.md`.
- If an agent turn produces a non-image file artifact that exists on the local filesystem, the GUI represents that artifact as a clickable file link and asks the operating system to open it with the default associated application.
- Markdown links using the Beryl-owned `beryl_threadid://<percent-encoded-thread-id>` scheme are internal transcript thread links. Activating one requests exact conversation-thread activation through the conversation-threads feature and must not ask the operating system to open a URL.
- Transcript thread-link activation is user-initiated thread navigation. Successful activation contributes to the workspace-local thread navigation history defined by `doc/features/conversation-threads/design.md`.
- Unknown, malformed, empty, unregistered, out-of-scope, or rebind-required transcript thread links fail with bounded Beryl UI notice behavior and leave the active thread unchanged.

## Transcript Images And Media

- Transcript image markers derived from Beryl-submitted local images remain compact atomic labels in user blocks. Activating a marker opens Beryl's preview popup when durable image bytes are available.
- Historical image markers remain visible even if their image bytes cannot be recovered; unavailable preview state is reported without replacing the marker with plain text.
- Native app-server generated-image output renders as transcript media. Pending generation may show a stable placeholder; completed output renders from embedded bytes or a readable saved path.
- A generated-image saved path is the authoritative generated image artifact while readable. Beryl may retain bounded presentation state but must not create a Beryl-side durable copy solely to preserve generated images when the saved path becomes unreadable.
- Markdown image syntax with a local filesystem target is a transcript media request. Relative paths resolve against the conversation thread's recorded execution target, not the GUI process working directory.
- File bytes referenced by Markdown remain filesystem state and may disappear or change outside Beryl. Beryl renders unavailable or updated filesystem state honestly rather than treating the Markdown reference as an attached artifact.
- Absolute Markdown image targets render only when proven to belong to the selected thread's expected runtime/member boundary.
- Supported raster image targets render in place. SVG and other non-raster formats are unsupported in this phase unless a later design expands support.
- Unsupported targets render `<alt> (render not supported)`. Missing or unreadable targets render `<alt> (file unavailable)`. Path-policy rejections render `<alt> (path not allowed)`. Size-admission rejections render `<alt> (image too large)`.
- Ordinary Markdown links remain links, including linked images shaped as `[![alt](path)](target)`.
- Consecutive transcript media items form media runs. One item occupies a full row, uses padded transcript content width, is capped by natural raster size, and is centered when narrower than the row.
- Multi-item runs use a shared compact width derived from about 30 `M` glyph advances in the active regular conversation text font, capped per item by natural size, and wrap at the right edge.
- Consecutive Markdown image embeds separated only by Markdown whitespace or line breaks are extracted into media rows even when their paragraph has surrounding prose.
- Activating a loaded item in a multi-item run toggles UI-local promotion for that item. Promotion gives it a single-image row at its transcript position while non-promoted siblings remain compact before or after it.
- Media promotion is presentation state only. It must not mutate backend history, Markdown source, generated-image records, media cache ownership, selection semantics, or workspace persistence.
- Secondary-clicking a loaded transcript media item opens the owning turn context menu plus `Copy image` and `Save image as` for that item.
- Media image actions derive full-resolution bytes from the authoritative file-backed or byte-backed source at action time, not from retained preview pixels.
- Pending placeholders, unsupported fallbacks, unavailable files, and path-rejected fallbacks do not expose image-specific actions.

## Selection, Copying, And Quote Harvesting

- Rendered transcript text supports ordinary desktop text selection for clipboard copying.
- Selection, scrolling, live remeasurement, and viewport virtualization preserve logical selected text while selected content remains in the loaded transcript window.
- Visible highlight rectangles must match the rendered portions of the logical selection, including soft-wrapped start and end positions.
- Beryl-owned Markdown semantic structures retain source-span or copy-source information so selection can produce Markdown-preserving copied text without reparsing full transcript history.
- Standard copy commands write Markdown-preserving selected text to the system clipboard, not lossy rendered-only text.
- Copying preserves Markdown syntax for inline code, emphasis, strong emphasis, links, lists, block quotes, headings, code blocks, image markers, math, and unsupported source fallbacks.
- Selected transcript image markers copy as explanatory text such as `[Image A]`.
- Selecting across a Markdown code block copies that selection as Markdown code-block source. A code block's own copy action copies only bare code.
- Selecting non-empty transcript text shows a transcript quote popup near the selection.
- The quote popup initially contains `Quote`, which inserts selected Markdown-preserving text into the current draft as Markdown block quote text by prefixing each logical line with `> `.
- Visual soft wrapping does not create additional quoted lines.
- Quote insertion uses the latest remembered draft insertion point or appends to the draft when no insertion point is known.
- After quote insertion, the remembered insertion point moves after the inserted quote block so repeated quote actions gather passages in reading order.
- Quote insertion preserves transcript scroll position, does not force focus into the composer, and does not mutate the clipboard.
- The quote popup closes when selection clears, outside click, `Escape`, transcript scrolling or virtualization loses stable selection geometry, or after accepted quote action.

## Turn Context Menu And Edit Presentation

- When no transcript selection is active, secondary-clicking rendered content that maps to one loaded parent turn opens that turn's context menu.
- The turn context menu targets the whole parent turn owning the clicked area, whether the click lands on user input or assistant narrative.
- The menu always contains `Edit message` for turn rows, including released history placeholder rows, and disables it with a specific unavailable-reason tooltip when editing cannot currently start.
- The menu contains `Branch and switch to` and `Branch in background` only when branch actions are available. Branch/edit orchestration is owned by the conversation-threads feature.
- The menu contains `Update thread title` for loaded parent turns when the clicked turn can provide a title seed for the selected thread. When title update cannot currently start, the row remains visible for eligible turn rows and is disabled with a specific unavailable-reason tooltip. Title-update orchestration is owned by the conversation-threads feature.
- When opened from loaded transcript media, the same menu is augmented with media-specific image actions for the clicked item.
- The menu is not opened for empty transcript space, operational activity, the activity caret, or active transcript selections.
- While thread-edit mode is active, the targeted turn and all later loaded turns render dimmed. Rows remain selectable, copyable, quoteable, and scrollable until commit removes them from backend history.
- Active-thread changes, workspace changes, loss of stable target turn, selected-thread activation, active turn, or selected-thread compaction cancel edit mode without mutating the composer draft.

## Scrolling And Activity Caret

- When a user input fragment is accepted, the transcript viewport positions the last rendered line of that fragment as the first visible line at the top of the transcript region.
- While the submit-time forced anchor remains active, assistant response content streams below the anchored user-fragment line into remaining visible space.
- If the accepted fragment is taller than the transcript region, earlier fragment lines may sit above the viewport.
- If content below the latest fragment line is too short to make that line naturally reachable at the top, the transcript uses bounded virtual trailing scroll allowance.
- Trailing allowance shrinks as real response content grows and disappears once real content fills the viewport below the anchor.
- If the user manually scrolls during or after a turn, Beryl stops forcing the submit-time anchor but keeps trailing allowance while needed.
- Existing-thread loads without a submit-time anchor open at the real end of the loaded window while preserving trailing allowance for the latest loaded user input fragment when needed.
- While the selected thread has an active parent turn, the transcript renders a non-interactive block activity caret at the end of the parent conversation narrative.
- The activity caret is not transcript content, Markdown source, selectable text, copyable text, quoteable text, or draft-caret state.
- The caret has stable geometry while blinking, does not cause reflow, disappears when the parent turn stops working, and follows platform text-caret blink policy when available. When blinking is disabled or reduced-motion requests apply, it renders steadily.
