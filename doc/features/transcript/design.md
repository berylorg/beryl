# Goals

Render the selected conversation and its explicitly promoted contextual presentation as a responsive parent transcript that users can read, scroll, select, copy, quote, inspect through context menus, and navigate without confusing authored turns with synthetic context, loading state, or operational activity.

Preserve user-visible Markdown structure, transcript media, exact manual scrolling, stable provenance for actions, and coherent selected-thread activation while keeping implementation details in the transcript presentation and Syndic systems.

## Non-goals

- Defining Syndic canonical history, transcript-view flattening, Markdown projection, storage schema, resource references, or backend provider policy.
- Defining transcript residency, renderer demand, working-set limits, shell host internals, or GPUI render pipeline mechanics.
- Rendering operational activity, tool logs, raw reasoning, diagnostics, hidden developer instructions, or backend protocol records as parent transcript narrative.

# Decisions

## Implementation References

- [`gui.md`](gui.md) is a normative supplemental GUI composition file for the transcript region, transcript-owned embedded widgets, menus, and previews.
- `doc/systems/transcript-presentation/design.md` owns the internal presentation, residency,
  rendering, scrolling, live-text takeover, and repair-publication architecture.
- `doc/systems/bounded-resource-dataflow/design.md` owns internal resource limits.
- `doc/systems/syndic-conversation-history/design.md` owns durable conversation history,
  provenance, transcript projections, resources, replay, and repaired records.
- `doc/systems/cas-live-syndic-transcript/design.md` owns live capture and terminal-repair mechanics
  for CAS-backed turns.
- `doc/features/image-assets/design.md` owns transcript-image Copy and `Save…` eligibility,
  disabled and failure behavior, focus outcomes, and durable asset semantics.

## Transcript Narrative

- The transcript presents the selected parent conversation narrative.
- Transcript narrative includes user-authored input fragments, transcript-visible user media markers, assistant commentary, assistant final answers, assistant text marked transcript-visible by the source, and generated media intended as assistant output.
- Transcript narrative excludes command execution details, tool calls, protocol records, file-change records, raw reasoning, token accounting, lifecycle/status events, subagent internals, hidden instructions, and other operational records unless a later product feature promotes a specific bounded summary.
- A branch discussion may contribute one readonly synthetic context item at its exact branch boundary. The item participates in transcript flow but remains explicitly classified as contextual presentation rather than authored narrative or a Syndic turn.
- Loading, missing-data, pending-range, stale-result, rejected-demand, and budget-fallback states are visible presentation states only. They are not selectable transcript content and must not be copied, quoted, or targeted as assistant-authored turns.

## Live Turn Presentation

- Transcript-visible assistant text follows its arrival cadence. Each newly arrived bounded portion
  becomes visible on the next available GUI frame; Beryl does not replay it through a fixed-rate
  character or token animation.
- Several portions that arrive before one GUI frame may naturally become visible together. Beryl
  introduces no additional pacing, so pauses and apparent throughput reflect when text reaches
  Beryl within ordinary frame scheduling rather than a simulated typewriter rate. Technical batch
  or fragment boundaries have no user-visible meaning.
- As active assistant text becomes exact history, text already shown remains visually continuous
  and in order. The transition must not duplicate, omit, reorder, blank, or visibly restyle matching
  content.
- Until exact historical provenance is available, live text remains readable but cannot authorize
  selection-derived history commands.
- If text shown live cannot be established as the complete historical record, the transcript does
  not silently replace it with conflicting content or a speculative merge. Any later repaired
  content appears only through the atomic whole-turn presentation described below.

## Repair Presentation

- Assistant words enter history through live capture. When live capture cannot establish a complete
  historical turn, Beryl makes exactly one terminal repair request for the affected turn. That
  request either produces the atomic whole-turn repaired presentation below or the turn becomes
  terminally incomplete; Beryl never retries the request, issues a second repair request, or
  splices competing representations together.
- Every transcript-visible assistant record belonging to a turn awaiting repair is labeled
  `Repair pending`. Visible durable content remains readable, but commands requiring complete turn
  provenance remain unavailable.
- Successful repair replaces the affected turn all at once. All repaired records appear together
  and are persistently labeled `Repaired from CAS history`; the user never sees a partial item
  splice, a mixture of pre-repair and repaired records, or a blank intermediate turn.
- The repair label and each affected record's accessible description expose the same provenance.
  Repair-state changes become visible only with the corresponding whole-turn replacement.
- Whole-turn replacement preserves the user's semantic transcript anchor. Any selection, quote
  affordance, or context menu whose source or geometry changed closes instead of targeting the new
  records through stale provenance.
- If whole-turn repair cannot establish complete history, affected records use the explicit
  terminal `Incomplete` label. Any content that remains present is readable, selectable, and
  copyable, but the transcript never presents it as repaired or complete and keeps commands that
  require complete turn provenance unavailable.

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
- `Discuss in new branch` requires an exact proven-terminal historical turn, a selection that still
  matches its current finalized assistant record, exact selected-range provenance, healthy
  Beryl-home storage, and selection size within the branch-discussion limit.
- The action is unavailable for user input, operational records, synthetic discussion-context
  items, loading or fallback text, cross-record selections without one exact source owner, live,
  unknown-terminal, incomplete, or no-longer-current assistant output, or stale geometry.
- Branch-discussion product behavior is defined in `doc/features/branch-discussions/design.md`.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers: none
