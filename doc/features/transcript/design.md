# Goals

Render the selected conversation and its explicitly promoted contextual presentation as a responsive parent transcript that users can read, scroll, select, copy, quote, inspect through context menus, and navigate without confusing authored turns with synthetic context, loading state, or operational activity.

Preserve user-visible Markdown structure, transcript media, exact manual scrolling, stable provenance for actions, and coherent selected-thread activation while keeping implementation details in the transcript presentation and Syndic systems.

## Non-goals

- Defining Syndic canonical history, transcript-view flattening, Markdown projection, storage schema, resource references, or backend provider policy.
- Defining transcript residency, renderer demand, working-set limits, shell host internals, or GPUI render pipeline mechanics.
- Rendering operational activity, tool logs, raw reasoning, diagnostics, hidden developer instructions, or backend protocol records as parent transcript narrative.
- Providing adapters from the transcript feature to obsolete transcript data structures.

# Decisions

## Implementation References

- `gui.md` is a normative supplemental GUI composition file for the transcript region, transcript-owned embedded widgets, menus, and previews.
- `doc/systems/transcript-presentation/design.md` owns the internal transcript host, residency, presentation, renderer, scroll, diagnostics, and shell-boundary architecture.
- `doc/systems/bounded-resource-dataflow/design.md` owns risk-based limits for resident pages,
  layout, snapshots, nested widgets, clipboard/export, decoded media, and GPU working sets.
- `doc/systems/syndic-conversation-history/design.md` owns durable conversation history, transcript views, Markdown projections, resources, and replay.
- `doc/systems/cas-live-syndic-transcript/design.md` owns CAS-live capture into Syndic and selected-history read authority for captured CAS-backed turns.

## Transcript Narrative

- The transcript presents the selected parent conversation narrative.
- Transcript narrative includes user-authored input fragments, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text marked transcript-visible by the source, and generated media intended as assistant output.
- Transcript narrative excludes command execution details, tool calls, protocol records, file-change records, raw reasoning, token accounting, lifecycle/status events, subagent internals, hidden instructions, and other operational records unless a later product feature promotes a specific bounded summary.
- A branch discussion may contribute one readonly synthetic context item at its exact branch boundary. The item participates in transcript flow but remains explicitly classified as contextual presentation rather than authored narrative or a Syndic turn.
- Loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback states are visible presentation states only. They are not selectable transcript content and must not be copied, quoted, or targeted as assistant-authored turns.

## Live Turn Presentation

- Transcript-visible assistant text follows the arrival cadence of normalized CAS text deltas. Beryl
  appends each arrived bounded fragment on the next available GUI frame while preserving its parent
  delta identity and order, and does not replay it through a fixed-rate character or token animation.
- One CAS delta may contain multiple characters or bounded transport fragments, and fragments from
  multiple deltas received between two GUI frames may naturally become visible together. Beryl
  introduces no additional pacing, so pauses and apparent throughput reflect when text reaches Beryl
  within ordinary frame scheduling rather than a simulated typewriter rate. Fragment boundaries do
  not become user-visible or durable event boundaries.
- Durable Syndic coalescing is independent from visible live cadence. The transcript may temporarily present one bounded non-authoritative live suffix beyond its durable Syndic prefix, then replace that suffix only after the corresponding Syndic projection proves exact prefix agreement.
- Durable takeover must not duplicate, omit, reorder, blank, or visibly restyle an already matching live prefix merely because its storage or projection revision changed. Until takeover, the transient suffix has no stable historical provenance and cannot authorize selection-derived history commands.
- If completed-item narrative disagrees with the text received live, the transcript retains the
  exact captured live prefix and presents that record as incomplete. It never swaps in the
  completion payload, hides the record, or presents either representation as repaired history.

## Scrolling And Activation

- Manual transcript scrolling is exact pixel displacement from wheel, touchpad, keyboard, or smoothed input deltas.
- Manual scrolling must not snap chunks, rows, turns, prompts, final answers, or transcript boundaries to viewport edges.
- Semantic placement is reserved for selected-thread activation, live-turn autoscroll policy, explicit navigation, and saved-position restore.
- When manual scrolling reaches the edge of coherent resident content, the transcript clamps at that edge until additional coherent content or a stable terminal fallback is available.
- The transcript region owns transcript scrolling without rendering the shared visual scrollbar affordance.
- Activating a selected transcript publishes visible content and the initial viewport state together.
- Markdown parsing, completed-media readiness, and bounded media loading are post-publication work with stable row-owned placeholders or terminal fallbacks. They must not gate selected-thread publication or install later scroll correction after the first visible frame.
- Activation must not blank or replace the transcript with a full-region loading placeholder when a previous coherent transcript can remain visible until the new coherent seed is ready.

## Large Content And Media

- Very large transcript records may appear through bounded chunks, nested widgets, or stable fallbacks.
- A large synthetic discussion-context item uses the same bounded chunk realization and anchor-preservation rules as other large transcript text and never creates a second fixed-height or independently scrolling viewport.
- Code blocks, tables, generated images, attachments, and comparable heavy resources may expose their own bounded interaction affordances such as inner scrolling, selection, copy, preview, or fallback states.
- A nested scrollable code panel does not take vertical pointer-wheel ownership merely because the pointer hovers over it. Clicking the nested code panel selects it for vertical pointer-wheel ownership; while selected, vertical wheel input over that code panel scrolls only the panel and must not co-scroll the transcript. Pressing `Escape` does not clear that wheel ownership.
- Media rendering must fail visibly and locally when content exceeds an applicable limit or cannot be decoded, loaded, or shown.
- Oversized or unsupported content must not make the full transcript unresponsive.

## Selection, Copy, Quote, And Menus

- Selection, Markdown-preserving copy, quote harvesting, and context menus operate only on rendered records whose provenance and geometry are stable.
- A rendered synthetic discussion-context item permits ordinary text selection and copy but never quote harvesting, replacement edit, branch creation, or an ordinary turn context menu.
- In streamed huge-content mode, transcript-level selection does not span through unrendered chunks.
- Nested widgets expose their own copy and selection contracts for their visible resource ranges.
- Copy reconstructs a contiguous clipboard representation only after the exact selected logical
  range fits the explicit platform clipboard limit. Rejection is explicit and preserves the
  selection. A nested resource that supports arbitrary-size source ranges exposes streaming
  `Save…` as the non-clipboard path; neither action reads renderer text or painted pixels as source.
- If virtualization, release, remeasurement, activation, or missing data destroys stable selection geometry, Beryl closes the selection, quote affordance, or menu instead of pinning unbounded offscreen content.
- Context menus target rendered transcript content. They do not open for empty space, operational activity, missing data, stale loading state, or transient non-content paint state.
- A context menu for an exact historical user-input turn exposes `Edit message`. The row stays visible but disabled when the closest replacement-edit gate can be explained; a row without exact stable Syndic provenance exposes no edit command.
- Entering replacement-edit mode closes the context menu and dims the targeted user-input turn plus its later turns on the selected path without changing their selection, copy, quote, or scrolling behavior.
- Replacement-edit workflow and path semantics are defined in `doc/features/conversation-threads/design.md`; draft interaction is defined in `doc/features/composer/design.md`.
- A non-empty stable selection wholly inside rendered assistant reply text exposes `Discuss in new branch` alongside Quote.
- `Discuss in new branch` requires an exact proven-terminal Syndic turn, a current finalized assistant item/projection revision, selected-range provenance, healthy Beryl-home storage, and selection size within the branch-discussion limit.
- The action is unavailable for user input, operational records, synthetic discussion-context items, loading or fallback text, cross-record selections without one exact source owner, live or unknown-terminal assistant output, stale or incomplete projection work, or stale geometry.
- Branch-discussion product behavior is defined in `doc/features/branch-discussions/design.md`.
