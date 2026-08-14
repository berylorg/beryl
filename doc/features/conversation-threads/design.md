# Goals

Let users create, resume, branch, edit, title, and select backend-owned Codex conversation threads from a Beryl workspace while preserving exact runtime/member bindings and keeping backend history ownership in `codex app-server`.

## Non-goals

- Copying backend conversation history into Beryl-owned durable thread records.
- Emulating app-server fork, rollback, resume, or naming primitives in GUI-local state.
- Silently rebinding a thread to another runtime, member, or backend process.
- Automatically generating names for externally created conversation threads without an explicit user action.

# Decisions

## Thread Ownership And Binding

- A conversation thread is backend-owned Codex state with its own message history and execution stream.
- Beryl owns GUI-local metadata for thread display, binding, selection, status, and workspace association.
- New standalone threads use the active workspace's current primary workspace member as their execution root.
- Existing threads may change bound workspace member or runtime only through an explicit rebind decision. Beryl never silently hops an existing thread to another execution context.
- Existing-thread activation resumes the selected backend thread by exact thread id. It must not enumerate all backend threads first, guess an alternate thread, or fall back to another runtime target.
- Activation validates that the expected execution target is in current workspace scope and that the resumed thread's recorded working directory still matches that target. Mismatches produce a rebind-required or activation-failure state.
- If no default runtime exists in a legacy or recovery workspace state, new thread creation and member attachment flows that need a runtime are unavailable until the user selects one.
- Backend-unavailable states disable backend-required thread operations only for the affected runtime target.

## Thread Display Titles

- Thread display title precedence is manual GUI-local title, then backend-provided thread name, then one temporary untitled label while automatic naming is pending or unavailable.
- Beryl consumes backend thread names from thread metadata, member-thread inventory refreshes, and live thread-name update notifications.
- Beryl-created threads without a manual or backend title become eligible for automatic title generation once both the first submitted user input fragment and backend thread id are known.
- Automatic title generation runs asynchronously on a background backend client connection for the target runtime and must not wait for the first assistant response or terminal turn state.
- Title generation uses a fresh app-server ephemeral maintenance thread per attempt, fixed Beryl title-generation instructions, explicit medium reasoning, and no global developer-instructions preference.
- Maintenance threads never appear in selectors, inventories, semantic graph refs, active-thread state, or transcript UI.
- Beryl publishes accepted generated titles to the target thread through `thread/name/set`, updates GUI-local title projection from the worker result or backend metadata, and schedules inventory refresh.
- Failed title generation or failed backend title publishing leaves the temporary untitled label until another title source exists.
- Title-generation cleanup requests app-server lifecycle cleanup for each maintenance thread after the attempt completes or fails.
- Beryl must not use a prompt-prefix heuristic as an automatic title.
- `Update thread title` is an explicit user-initiated auto-titling action exposed from the transcript turn context menu for the active selected thread.
- On-demand title updates may target Beryl-created or externally created registered threads when Beryl can prove the clicked loaded parent turn belongs to the selected backend thread and can reconstruct non-empty ordered user input fragments for that turn.
- On-demand title updates use the clicked parent turn's ordered user input fragments as the title seed. If the click lands on assistant narrative inside a parent turn, the seed still comes from that parent turn's user input. V1 does not summarize full-thread history or assistant output for this action.
- On-demand title updates run asynchronously on a background backend client connection using the same fresh ephemeral maintenance-thread pattern, fixed Beryl title-generation instructions, explicit medium reasoning, developer-instructions isolation, cleanup behavior, generated-title acceptance rules, and `thread/name/set` publication path as automatic title generation.
- On-demand title updates bypass automatic title-generation eligibility and existing backend-title checks because they represent explicit user intent. A backend title that already exists may be replaced by the newly accepted generated title.
- At most one thread-title worker may target a thread at a time. If a title worker is already in flight for the target thread, the on-demand update action is unavailable instead of starting a duplicate request.
- On-demand title updates publish backend thread-name metadata and do not create GUI-local manual titles. If a GUI-local manual title currently controls display precedence for the target thread, the action is unavailable rather than publishing a generated backend name that would remain hidden.

## Member-Thread Inventory

- Beryl maintains a bounded UI-facing member-thread inventory snapshot for the active workspace, grouped by available runtime-bound workspace member.
- Inventory groups are available explicit members, or the implicit home member when no available explicit member exists and a default runtime is selected.
- Unavailable explicit members remain visible in member management UI but do not contribute inventory groups.
- A backend thread summary belongs under a member only when its recorded runtime and working directory exactly match that member's runtime and canonical path.
- Inventory refresh runs off the `gpui` thread, uses backend-side working-directory filtering and updated-time ordering when available, and atomically publishes complete snapshots to UI state.
- Inventory refresh may enrich list summaries with metadata-only thread reads to obtain fork parent ids. That enrichment remains part of the background refresh job and must not move backend calls into selector rendering.
- Fork parent metadata does not affect member grouping. A thread remains grouped by its own recorded working directory.
- Thread rows use the current title precedence before publication so manual titles and backend metadata changes are reflected consistently.

## Thread Selector

- The thread selector is a popup opened from the active thread title control. It switches the active Codex thread without requiring semantic graph interaction.
- Opening the selector closes the graph overlay and graph context menus so only one column-selector surface is interactive.
- The selector renders from the latest available member-thread inventory snapshot and does not synchronously call `codex app-server`.
- Opening the selector may request a background inventory refresh for backend-available targets while stale snapshot content remains usable.
- With exactly one available member, including the implicit home case, the selector opens directly to that member's root/orphan thread column.
- With multiple available members, the first column lists members and selecting a member opens that member's root/orphan thread column.
- Root/orphan columns include threads whose fork parent is absent from the same member group, missing, stale, malformed, filtered out, or grouped under another member.
- Selecting a thread row with direct forks opens recursive fork columns derived only from backend-provided parent ids in the same member group.
- Thread row ordering uses newest backend update time in the row's visible branch subtree so recently active forks keep their root branch near recent work.
- Opening the selector preselects the active thread row when it appears in the latest snapshot.
- Single-click selects rows and may open child columns. Double-clicking a thread row or pressing `Enter` activates that exact thread. `Escape` closes without changing selection.
- After activation is accepted, the selector closes and the transcript region shows pending activation state for the target thread.
- Refresh reconciliation preserves selector path by member and thread identity, pruning invalid fork columns without substituting another selected thread.

## Thread Branching

- Branching creates a new backend-owned Codex thread from an active source thread, preserves history through the clicked parent turn in the new thread, removes later turns from the new thread, and leaves the source thread unchanged.
- If the clicked area belongs to an assistant response inside a parent turn, the branch keeps that assistant response because the target is the whole parent turn.
- Branching is unavailable when Beryl cannot identify a backend turn id and non-empty user input for the clicked turn, when the source thread is not idle, during selected-thread compaction or activation, or when app-server fork/rollback primitives are unavailable.
- `Branch and switch to` activates the new branch only after fork, rollback, local branch registration, title scheduling, and initial transcript activation succeed.
- `Branch in background` registers the branch, schedules inventory refresh, and keeps the current active transcript selected.
- Branched threads are Beryl-created threads. Their automatic title seed is the clicked turn's ordered user input fragments, not the source title or assistant output.
- Branch orchestration runs away from the `gpui` thread and does not mutate source transcript projection.

## Thread Editing

- Thread editing is a user-initiated source-thread rewrite flow over backend history. It is not an in-place mutation of an existing turn id.
- `Edit message` is enabled only when app-server rollback is available, Beryl can identify the target backend turn id, reconstruct non-empty user input, compute an exact trailing user-turn rollback count including the target turn, and prove the selected thread is idle with a current loaded tail.
- The turn context menu keeps `Edit message` visible as a disabled row when editing is unavailable for a clicked turn row. Its tooltip names the closest internal Beryl gate or target-resolution reason, such as missing rollback capability, current-tail unknown, pending selected-thread work, source-thread mismatch, unreconstructable input, missing image metadata, or unprovable rollback scope.
- `Edit message` is unavailable during context compaction, thread activation, active or queued turn submission, pending branch/edit work, and incomplete or stale history states.
- Starting edit mode requires an empty composer draft, closes the context menu, dims the target turn and later loaded turns, and populates the composer with the target turn's user input.
- Edit mode is presentation-only until commit. It must not mutate backend history, workspace state, transcript persistence, semantic graph state, image assets, or activity records.
- `Escape` cancels edit mode through the composer command path after higher-priority popups have handled the key. Canceling removes dimming without clearing or changing the composer draft.
- Submitting a non-empty edit draft first performs ordinary local draft validation and backend input preparation. If validation fails, edit mode remains active and the draft remains intact.
- After validation succeeds, edit commit rolls back the selected backend thread by the exact trailing user-turn count, resets visible transcript state from the rollback response, and starts a replacement backend turn from the current draft.
- If selection changes while edit commit is in flight, rollback and replacement responses remain scoped to the original target thread and must not apply to unrelated visible transcripts.
- If rollback succeeds but replacement start or delivery fails, the discarded tail remains deleted. Beryl keeps the draft intact and reports the failure.
- Thread editing does not revert filesystem changes, semantic graph/checklist mutations, workspace state, thread title metadata, durable image assets, or other non-history side effects produced by discarded turns.

## Backend Requirements

- Thread activation depends on exact resume by thread id and bounded paginated history reads.
- Branching depends on app-server fork and rollback primitives. When unavailable, Beryl must not emulate branching by copying backend history locally.
- Editing depends on app-server rollback and turn-start primitives. When unavailable or when exact rollback scope cannot be proven, Beryl must not emulate editing locally.
- On-demand title updates depend on app-server ephemeral-thread creation, turn-start, turn-stream observation, maintenance-thread cleanup, and `thread/name/set`. When those primitives or a backend client for the exact target runtime are unavailable, Beryl must not emulate retitling through GUI-local title metadata.
- Thread selector and link-thread menus must render from inventory snapshots and stay responsive while refresh or activation work is pending.
