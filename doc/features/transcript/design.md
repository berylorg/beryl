# Goals

Render the selected conversation as a responsive parent transcript that users can read, scroll, select, copy, quote, inspect through context menus, and navigate without confusing transcript content with loading state or operational activity.

Preserve user-visible Markdown structure, transcript media, exact manual scrolling, stable provenance for actions, and coherent selected-thread activation while keeping implementation details in the transcript presentation and Syndic systems.

## Non-goals

- Defining Syndic canonical history, transcript-view flattening, Markdown projection, storage schema, resource references, or backend provider policy.
- Defining transcript residency, renderer demand, resource admission, shell host internals, or GPUI render pipeline mechanics.
- Rendering operational activity, tool logs, raw reasoning, diagnostics, hidden developer instructions, or backend protocol records as parent transcript narrative.
- Providing adapters from the transcript feature to obsolete transcript data structures.

# Decisions

## Implementation References

- `gui.md` is a normative supplemental GUI composition file for the transcript region, transcript-owned embedded widgets, menus, and previews.
- `doc/systems/transcript-presentation/design.md` owns the internal transcript host, residency, presentation, renderer, resource admission, scroll, diagnostics, and shell-boundary architecture.
- `doc/systems/syndic-conversation-history/design.md` owns durable conversation history, transcript views, Markdown projections, resources, and replay.
- `doc/systems/cas-live-syndic-transcript/design.md` owns CAS-live capture into Syndic and selected-history read authority for captured CAS-backed turns.

## Transcript Narrative

- The transcript presents the selected parent conversation narrative.
- Transcript narrative includes user-authored input fragments, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text marked transcript-visible by the source, and generated media intended as assistant output.
- Transcript narrative excludes command execution details, tool calls, protocol records, file-change records, raw reasoning, token accounting, lifecycle/status events, subagent internals, hidden instructions, and other operational records unless a later product feature promotes a specific bounded summary.
- Loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback states are visible presentation states only. They are not selectable transcript content and must not be copied, quoted, or targeted as assistant-authored turns.

## Scrolling And Activation

- Manual transcript scrolling is exact pixel displacement from wheel, touchpad, keyboard, or smoothed input deltas.
- Manual scrolling must not snap chunks, rows, turns, prompts, final answers, or transcript boundaries to viewport edges.
- Semantic placement is reserved for selected-thread activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.
- When manual scrolling reaches the edge of coherent resident content, the transcript clamps at that edge until additional coherent content or a stable terminal fallback is available.
- The transcript region owns transcript scrolling without rendering the shared visual scrollbar affordance.
- Activating a selected transcript publishes visible content and the initial viewport state together.
- Markdown parsing, completed-media readiness, and media resource admission are post-publication work with stable row-owned placeholders or terminal fallbacks. They must not gate selected-thread publication or install later scroll correction after the first visible frame.
- Activation must not blank or replace the transcript with a full-region loading placeholder when a previous coherent transcript can remain visible until the new coherent seed is ready.

## Large Content And Media

- Very large transcript records may appear through bounded chunks, nested widgets, or stable fallbacks.
- Code blocks, tables, generated images, attachments, and comparable heavy resources may expose their own bounded interaction affordances such as inner scrolling, selection, copy, preview, or fallback states.
- A nested scrollable code panel does not take vertical pointer-wheel ownership merely because the pointer hovers over it. Clicking the nested code panel selects it for vertical pointer-wheel ownership; while selected, vertical wheel input over that code panel scrolls only the panel and must not co-scroll the transcript. Pressing `Escape` does not clear that wheel ownership.
- Media rendering must fail visibly and locally when content cannot be admitted, decoded, loaded, or shown within Beryl's resource policy.
- Oversized or unsupported content must not make the full transcript unresponsive.

## Selection, Copy, Quote, And Menus

- Selection, Markdown-preserving copy, quote harvesting, and context menus operate only on rendered records whose provenance and geometry are stable.
- In streamed huge-content mode, transcript-level selection does not span through unrendered chunks.
- Nested widgets expose their own copy and selection contracts for their visible resource ranges.
- If virtualization, release, remeasurement, activation, or missing data destroys stable selection geometry, Beryl closes the selection, quote affordance, or menu instead of pinning unbounded offscreen content.
- Context menus target rendered transcript content. They do not open for empty space, operational activity, missing data, stale loading state, or transient non-content paint state.
