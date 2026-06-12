# Goals

Let users create, resume, branch, edit, title, navigate between, and select backend-owned Codex conversation threads from a Beryl workspace while preserving exact runtime/member bindings and keeping backend history ownership in `codex app-server`.

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
- Beryl-created branch threads may have a provisional backend title derived from the branch source or decision item so they are navigable immediately after creation. The Beryl-generated bootstrap turn is not a title seed.
- After the first real user-authored turn in a Beryl-created branch thread, Beryl schedules automatic title generation from that real user input and may replace the provisional Beryl-generated backend title. This branch retitling must not override a GUI-local manual title, an explicit user-initiated title update, or another title worker already targeting the same thread.
- Automatic title generation runs asynchronously on a background backend client connection for the target runtime and must not wait for the first assistant response or terminal turn state.
- Title generation uses a fresh app-server ephemeral maintenance thread per attempt, fixed Beryl title-generation instructions, explicit medium reasoning, and no global developer-instructions preference.
- Maintenance threads never appear in selectors, inventories, semantic graph refs, active-thread state, or transcript UI.
- Beryl publishes accepted generated titles to the target thread through `thread/name/set`, updates GUI-local title projection from the worker result or backend metadata, and schedules inventory refresh.
- Failed title generation or failed backend title publishing leaves the temporary untitled label until another title source exists.
- Title-generation cleanup requests app-server lifecycle cleanup for each maintenance thread after the attempt completes or fails.
- Beryl must not use a prompt-prefix heuristic as an automatic title.
- `Update thread title` is an explicit user-initiated auto-titling action exposed from the transcript turn context menu for the active selected thread.
- On-demand title updates may target Beryl-created or externally created registered threads when Beryl can prove the clicked resident parent turn belongs to the selected backend thread and can reconstruct non-empty ordered user input fragments for that turn.
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
- Beryl schedules inventory refresh in response to operations that can change the inventory, including successful thread creation, branch bootstrap completion, archive or unarchive observation, backend thread-name update or title publication, workspace member changes, default-runtime changes that alter the implicit member, and backend reconnect or target reopening.

## Thread Selector

- The thread selector is a popup opened from the active thread title control. It switches the active Codex thread without requiring semantic graph interaction.
- Opening the selector closes the graph overlay and graph context menus so only one column-selector surface is interactive.
- The selector renders from the latest available member-thread inventory snapshot and does not synchronously call `codex app-server`.
- Opening the selector must not itself mark inventory for refresh or start backend enumeration. It uses the latest completed snapshot prepared by background maintenance.
- A selector popup's visible rows, columns, and ordering remain stable while the popup is open. If a newer inventory refresh completes while the selector is open, Beryl defers applying it to that open selector projection until the selector is closed and reopened, unless a later explicit apply-refresh affordance is designed.
- With exactly one available member, including the implicit home case, the selector opens directly to that member's root/orphan thread column.
- With multiple available members, the first column lists members and selecting a member opens that member's root/orphan thread column.
- Root/orphan columns include threads whose fork parent is absent from the same member group, missing, stale, malformed, filtered out, or grouped under another member.
- Selecting a thread row with direct forks opens recursive fork columns derived only from backend-provided parent ids in the same member group.
- Thread row ordering uses newest backend update time in the row's visible branch subtree so recently active forks keep their root branch near recent work.
- Opening the selector preselects the active thread row when it appears in the latest snapshot.
- Single-click selects rows and may open child columns. Double-clicking a thread row or pressing `Enter` activates that exact thread. `Escape` closes without changing selection.
- After activation is accepted, the selector closes and the active title selector may immediately show the requested target thread title with a stable in-button progress fill. The control keeps the same geometry as the normal title selector and must not use an `Opening ...` or other transient loading label.
- Pending activation progress is chrome state derived from activation-owned work such as accepted intent, resident-history fetch, presentation-window construction, and prepublication structural preparation. It is not transcript content, does not publish the target as the selected thread, and must not trigger backend reads or renderer-owned media admission from rendering.
- While pending activation progress is visible, breadcrumbs and the transcript region keep rendering the previous coherent selected-thread state until the new selected thread's resident history is prepared and applied.
- Successful selected-thread activation clears pending progress and applies the normal active title selector state, breadcrumbs, transcript rows, and the transcript's initial viewport state together. The activation path must not rely on deferred renderer callbacks to revise transcript scroll position after the newly activated thread first becomes visible.
- Failed, rejected, canceled, or stopped activation clears pending progress, restores the previous title selector state, and surfaces the normal activation failure notice without changing the selected transcript.
- Snapshot reconciliation preserves closed selector state and next-open projections by member and thread identity, pruning invalid fork columns without substituting another selected thread.

## Thread History Navigation

- The main toolbar exposes any selected branch's clickable parent breadcrumbs immediately after the Workspaces control. Breadcrumb buttons are content-sized under normal space and truncate only when long parent titles hit the bounded breadcrumb area.
- The thread strip exposes workspace-local backward and forward thread-navigation controls immediately after New Thread and before the active thread selector.
- Graph and Settings controls remain in the toolbar trailing group after flexible spacing.
- The Workspaces and New Thread controls are normal text-labeled command buttons that use the shared app-wide button horizontal padding and content-sized width. They must not reserve a wider fixed leading chrome slot merely to match each other.
- Square or icon-like thread-navigation controls may use square geometry, but that exception does not apply to normal text-labeled command buttons.
- The thread strip must not render static runtime-context labels before the active thread selector. WSL context belongs in workspace/member management, activation/recovery messages, diagnostics, or other context-owning surfaces, not as a `wsl-linux:<distro>` prefix in front of the selector.
- Thread-navigation controls are icon-like command buttons using compact backward and forward labels. They remain visible and render disabled with a local unavailable reason when no corresponding navigation target exists or when thread activation is currently blocked.
- Thread-navigation history is GUI-local in-memory session state scoped to the loaded Beryl workspace. It survives thread switching within that workspace and is discarded on app restart or workspace/backend-session teardown.
- History entries identify exact backend conversation thread ids and the runtime/member execution target known at the time the entry was recorded. They are not backend conversation history and are not persisted as workspace content.
- Successful user-initiated thread switches from the thread selector, transcript `beryl_threadid://` links, and toolbar branch breadcrumbs update the navigation history.
- Failed, rejected, canceled, already-selected, background-only, inventory-refresh, title-update, workspace-selection, pending-new-thread, automatic restore, and backend recovery selections do not push thread-navigation entries.
- When a new user-initiated thread switch succeeds after the user has navigated backward, Beryl truncates the forward stack before recording the new target.
- Backward and forward commands use the same exact activation path and activation gates as thread selector activation, including backend availability, busy selected-thread work, current workspace scope, rebind-required checks, and no-flicker pending activation presentation.
- Activation failure during backward or forward navigation leaves the current thread and navigation stacks unchanged except for any bounded surface notice produced by the normal activation path.
- Navigating to a thread whose recorded target is no longer in current workspace scope, whose registration is missing, or whose registration requires rebind is rejected instead of substituting another thread.
- Thread-navigation rendering must not synchronously call `codex app-server`, enumerate inventories, refresh thread summaries, or read transcript history.
- Pending thread activation may change the active thread selector label to the target thread title only when paired with the stable in-button progress fill defined by Thread Selector. It must not change the label to an `Opening ...` or other transient loading presentation. Breadcrumbs continue to render from the last selected-thread branch projection until the activation result applies the new selected thread or reports failure.
- Pending thread activation must not blank or replace the transcript region with an `Opening ...` or loading placeholder. The prior transcript projection remains visible until the activation result applies the new selected thread or reports failure.
- Backward, forward, selector, transcript-link, and breadcrumb activation share the same one-shot transcript viewport rule: once the new thread's transcript is visible, activation-owned scroll state must already be final and must not be corrected by a later renderer callback.

## Thread Branching

- Branching creates a new backend-owned Codex thread from an active source thread, preserves history through the clicked parent turn in the new thread, removes later turns from the new thread, and leaves the source thread unchanged.
- If the clicked area belongs to an assistant response inside a parent turn, the branch keeps that assistant response because the target is the whole parent turn.
- Branching is unavailable when Beryl cannot identify a backend turn id and non-empty user input for the clicked turn, when the source thread is not idle, during selected-thread compaction or activation, or when app-server fork/rollback primitives are unavailable.
- Every Beryl-created branch thread begins with a visible bootstrap user turn in backend history after fork/rollback or child-thread creation succeeds. The bootstrap turn records branch provenance such as `Branched from [<parent thread title>](beryl_threadid://<parent_thread_id>)` and may include branch-specific context. It is ordinary transcript history from the app-server perspective.
- The bootstrap turn is a Beryl-authored provenance turn, not real user exploration. It must not seed automatic title generation, count as child progress for feature workflows, or be hidden from the transcript.
- Bootstrap turns do not consume global developer-instructions or graph-upkeep hidden-context settings. Any branch-specific context needed by future model turns must be present in the visible bootstrap message.
- Beryl publishes a branch into durable GUI-local workspace state only after the bootstrap turn reaches terminal success and the branch thread is proven to be a real openable backend thread. Durable branch publication includes graph thread refs, decision bindings, branch registration, branch inventory visibility, and branch title scheduling. If bootstrap turn creation, terminal execution, or durability proof fails, Beryl shows an error and leaves none of that durable Beryl-owned branch state behind for the failed branch creation.
- `Branch and switch to` is a foreground branch workflow. After fork/rollback succeeds and the visible bootstrap turn is accepted with an exact turn id, Beryl immediately selects the new branch thread and attaches the shell UI to that bootstrap turn stream. The transcript shows the branched history plus the visible bootstrap user message while the bootstrap turn is running, and the status line represents the bootstrap as a Beryl-owned active turn with normal applicable foreground controls. Durable branch publication still waits for terminal bootstrap success and durability proof.
- If a foregrounded `Branch and switch to` bootstrap later fails or cannot be proven durable, Beryl reports the failure in the selected branch workflow and still leaves no graph thread ref, decision binding, branch registration, branch inventory publication, or title-scheduling state for that failed branch. The already-selected backend thread is treated only as transient foreground backend state until a later explicit user action or successful publication gives it durable Beryl branch metadata.
- `Branch in background` is a background branch workflow. It registers and publishes the branch only after terminal bootstrap success and durability proof, schedules inventory refresh, and keeps the current active transcript selected while the branch is being prepared.
- When the selected thread is a Beryl-known branch, the toolbar renders its parent lineage as breadcrumbs after Workspaces, for example `Parent Thread > Parent Branch`. The thread strip renders `New Thread`, backward/forward controls, and the active thread title selector. Breadcrumb buttons hug their text label under normal toolbar space, while very long labels truncate inside the bounded breadcrumb trail so Graph and Settings remain reachable. The bounded trail must have enough ordinary desktop capacity for two max-width parent breadcrumb buttons plus their separator and gaps without clipping a button edge. Breadcrumbs use GUI-local branch parent metadata and current title precedence for the selected thread; they must not trigger backend reads or inventory refresh during rendering. If another thread activation is pending, the existing selected-thread breadcrumbs stay visible until the new thread is applied or activation fails. Branch parent metadata comes from successful Beryl branch publication, transient foreground branch state, and already-enriched thread summary or member-inventory `forkedFromId` metadata as those summaries are registered into workspace state. Parent breadcrumb segments activate the exact registered parent thread when activation is available and become disabled when the parent is missing or requires rebind. During foreground `Branch and switch to`, the transient branch state supplies the parent breadcrumb before durable branch publication succeeds.
- Branched threads are Beryl-created threads. Their provisional title seed is the clicked turn's ordered user input fragments, not the source title or assistant output. Their first real user-authored branch turn triggers branch retitling as defined in Thread Display Titles.
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
- Thread editing does not revert filesystem changes, semantic graph/checklist-item mutations, workspace state, thread title metadata, durable image assets, or other non-history side effects produced by discarded turns.

## Backend Requirements

- Thread activation depends on exact resume by thread id and bounded paginated history reads.
- Branching depends on app-server fork, rollback, turn-start, and exact reopen/read primitives. When unavailable, Beryl must not emulate branching by copying backend history locally.
- Editing depends on app-server rollback and turn-start primitives. When unavailable or when exact rollback scope cannot be proven, Beryl must not emulate editing locally.
- On-demand title updates depend on app-server ephemeral-thread creation, turn-start, turn-stream observation, maintenance-thread cleanup, and `thread/name/set`. When those primitives or a backend client for the exact target runtime are unavailable, Beryl must not emulate retitling through GUI-local title metadata.
- Thread selector and link-thread menus must render from inventory snapshots and stay responsive while refresh or activation work is pending.
