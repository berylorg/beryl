# Goals

Let users create, resume, branch, edit, title, navigate between, and select Beryl conversation views from a workspace while preserving exact runtime/member bindings, keeping historical data owned by Syndic, keeping GUI workspace state owned by Beryl workspace storage, and using Codex App Server only as the live execution stream source for approved projections.

## Non-goals

- Copying Codex App Server historical transcript reads into GUI-local durable thread records.
- Querying Codex App Server historical transcript APIs as the selected transcript source after Syndic capture cutover.
- Querying Codex App Server thread-list or metadata-read APIs as the thread selector, workspace restore, branch tree, title, or link-menu authority.
- Storing selected-thread GUI state, navigation history, manual title overrides, workspace membership, or semantic graph refs inside Syndic history storage.
- Emulating app-server fork, rollback, resume, or live execution primitives in GUI-local state.
- Silently rebinding a thread to another runtime, member, or backend process.
- Automatically generating names for externally created conversation threads without an explicit user action.

# Decisions

## Implementation References

- CAS projection bindings, stale/unbound projection behavior, fresh materialization, and Syndic graph-action reflection are defined in `doc/systems/cas-live-syndic-transcript/design.md`.
- Durable captured transcript history and transcript-view provenance are defined in `doc/systems/syndic-conversation-history/design.md`.
- Transcript presentation residency and context-menu provenance surfaces are defined in `doc/systems/transcript-presentation/design.md`.
- Backend runtime capability probing and backend-unavailable behavior are defined in `doc/systems/backend-runtime/design.md` and `doc/features/backend-runtime-recovery/design.md`.

## Thread Ownership And Binding

- A conversation view is a Beryl-visible path through Syndic-owned historical turns with optional CAS execution projection metadata.
- Syndic owns durable rendered transcript history, historical branch/view relationships, source provenance, projection records, and history-derived summary facts for turns captured through the Beryl live ingestion boundary.
- Beryl workspace storage owns workspace membership, active selected view, manual and generated title overrides, runtime/member binding decisions, semantic graph refs, navigation history, and other GUI state.
- Threads whose CAS projection is stale or unbound remain browsable from Syndic-captured history when that history is complete enough for the selected view.
- Stale or unbound CAS projection state does not force immediate backend thread creation. Backend execution is required only when the user starts or resumes work that needs a live model turn.
- New standalone threads use the active workspace's current primary workspace member as their execution root.
- Existing threads may change bound workspace member or runtime only through an explicit rebind decision. Beryl never silently hops an existing thread to another execution context.
- Existing-thread activation selects a workspace-registered Syndic conversation view. It must not enumerate backend threads, guess an alternate backend thread, or fall back to another runtime target.
- Activation validates that the workspace-owned runtime/member binding for the selected view is in current workspace scope. Mismatches produce a rebind-required or activation-failure state.
- If no default runtime exists in a legacy or recovery workspace state, new thread creation and member attachment flows that need a runtime are unavailable until the user selects one.
- Backend-unavailable states disable backend-required thread operations only for the affected runtime target.

## Thread Display Titles

- Thread display title precedence is manual GUI-local title, then Beryl-generated workspace title metadata, then Syndic history-derived title summary, then one temporary untitled label while naming is pending or unavailable.
- Beryl must not use CAS thread names, thread-list rows, metadata-only thread reads, or live thread-name notifications as selector or restore title authority.
- Beryl-created views without a manual or generated title become eligible for automatic title generation once the first submitted user input fragment is durably captured in Syndic.
- Beryl-created branch views may have a provisional workspace title derived from the branch source or decision item so they are navigable immediately after creation. The Beryl-generated bootstrap turn is not a title seed.
- After the first real user-authored turn in a Beryl-created branch view, Beryl schedules automatic title generation from that real user input and may replace the provisional generated workspace title. This branch retitling must not override a GUI-local manual title, an explicit user-initiated title update, or another title worker already targeting the same view.
- Automatic title generation runs asynchronously and must not wait for the first assistant response or terminal turn state.
- Title generation may use a fresh app-server ephemeral maintenance turn as a live model worker, fixed Beryl title-generation instructions, explicit medium reasoning, and no global developer-instructions preference, but accepted title authority is Beryl workspace title metadata.
- Maintenance threads never appear in selectors, catalogs, semantic graph refs, active-thread state, or transcript UI.
- Beryl persists accepted generated titles in workspace storage, updates GUI-local title projection from the worker result, and schedules catalog refresh.
- Failed title generation or failed workspace title persistence leaves the temporary untitled label until another title source exists.
- Title-generation cleanup requests app-server lifecycle cleanup for each maintenance thread after the attempt completes or fails.
- Beryl must not use a prompt-prefix heuristic as an automatic title.
- `Update thread title` is an explicit user-initiated auto-titling action exposed from the transcript turn context menu for the active selected thread.
- On-demand title updates may target Beryl-created or externally registered views when Beryl can prove the clicked resident parent turn belongs to the selected Syndic view and can reconstruct non-empty ordered user input fragments for that turn.
- Transcript-originated title updates consume the context-menu target provided by the transcript feature. The action is unavailable unless that target has stable provenance and enough resident source data to reconstruct the ordered user input seed.
- On-demand title updates use the clicked parent turn's ordered user input fragments as the title seed. If the click lands on assistant narrative inside a parent turn, the seed still comes from that parent turn's user input. V1 does not summarize full-thread history or assistant output for this action.
- On-demand title updates run asynchronously using the same fresh ephemeral maintenance-turn pattern, fixed Beryl title-generation instructions, explicit medium reasoning, developer-instructions isolation, cleanup behavior, generated-title acceptance rules, and workspace title persistence as automatic title generation.
- On-demand title updates bypass automatic title-generation eligibility because they represent explicit user intent. Existing generated workspace title metadata may be replaced by the newly accepted generated title.
- At most one thread-title worker may target a thread at a time. If a title worker is already in flight for the target thread, the on-demand update action is unavailable instead of starting a duplicate request.
- On-demand title updates publish generated workspace title metadata and do not create GUI-local manual titles. If a GUI-local manual title currently controls display precedence for the target view, the action is unavailable rather than publishing a generated title that would remain hidden.

## Workspace Thread Catalog

- Beryl maintains a bounded UI-facing thread catalog snapshot for the active workspace by joining workspace-registered Syndic conversation-view refs with Syndic history-derived summary facts.
- Catalog groups are available explicit members, or the implicit home member when no available explicit member exists and a default runtime is selected.
- Unavailable explicit members remain visible in member management UI but do not contribute catalog groups.
- A workspace-registered conversation-view ref belongs under a member only when its Beryl-owned runtime/member binding exactly matches that member's runtime and canonical path.
- Catalog refresh runs off the `gpui` thread, reads workspace state and bounded Syndic summaries, and atomically publishes complete snapshots to UI state.
- Catalog refresh must not call CAS thread-list, metadata-read, or historical transcript APIs.
- Branch parent metadata does not affect member grouping. A view remains grouped by its own workspace-owned runtime/member binding.
- Thread rows use the current title precedence before publication so manual titles, generated workspace titles, and Syndic history-derived title summaries are reflected consistently.
- Beryl schedules catalog refresh in response to operations that can change workspace refs or Syndic history summaries, including successful thread creation, branch publication, edit replacement, title update, archive or unarchive state change, workspace member changes, and default-runtime changes that alter the implicit member.

## Thread Selector

- The thread selector is a popup opened from the active thread title control. It switches the active Codex thread without requiring semantic graph interaction.
- Opening the selector closes the graph overlay and graph context menus so only one column-selector surface is interactive.
- The selector renders from the latest available workspace thread catalog snapshot and must not call `codex app-server`.
- Opening the selector must not itself mark catalog refresh or start backend enumeration. It uses the latest completed snapshot prepared by background maintenance.
- A selector popup's visible rows, columns, and ordering remain stable while the popup is open. If a newer catalog refresh completes while the selector is open, Beryl defers applying it to that open selector projection until the selector is closed and reopened, unless a later explicit apply-refresh affordance is designed.
- With exactly one available member, including the implicit home case, the selector opens directly to that member's root/orphan thread column.
- With multiple available members, the first column lists members and selecting a member opens that member's root/orphan thread column.
- Root/orphan columns include views whose Syndic branch parent is absent from the same member group, missing, filtered out, or grouped under another member.
- Selecting a thread row with direct children opens recursive branch columns derived only from workspace refs and Syndic branch/view relationships in the same member group.
- Thread row ordering uses newest Syndic history-derived activity time in the row's visible branch subtree so recently active branches keep their root branch near recent work.
- Opening the selector preselects the active thread row when it appears in the latest snapshot.
- Single-click selects rows and may open child columns. Double-clicking a thread row or pressing `Enter` activates that exact thread. `Escape` closes without changing selection.
- After activation is accepted, the selector closes and the active title selector may immediately show the requested target thread title with a stable in-button progress fill. The control keeps the same geometry as the normal title selector and must not use an `Opening ...` or other transient loading label.
- Pending activation progress is chrome state derived from activation-owned work such as accepted intent, selected transcript-view activation, resident data seed preparation, and presentation-window construction. It is not transcript content, does not publish the target as the selected thread, and must not trigger backend reads, Syndic provider requests, or media admission from rendering.
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
- Thread-navigation history is GUI-local in-memory session state scoped to the loaded Beryl workspace. It survives thread switching within that workspace and is discarded on app restart or workspace teardown.
- History entries identify exact workspace-registered Syndic conversation-view refs and the runtime/member execution target known at the time the entry was recorded. They are not historical conversation content and are not persisted as workspace content.
- Successful user-initiated thread switches from the thread selector, transcript `beryl_threadid://` links, and toolbar branch breadcrumbs update the navigation history.
- Failed, rejected, canceled, already-selected, background-only, catalog-refresh, title-update, workspace-selection, pending-new-thread, automatic restore, and backend recovery selections do not push thread-navigation entries.
- When a new user-initiated thread switch succeeds after the user has navigated backward, Beryl truncates the forward stack before recording the new target.
- Backward and forward commands use the same exact activation path and activation gates as thread selector activation, including backend availability, busy selected-thread work, current workspace scope, rebind-required checks, and no-flicker pending activation presentation.
- Activation failure during backward or forward navigation leaves the current thread and navigation stacks unchanged except for any bounded surface notice produced by the normal activation path.
- Navigating to a thread whose recorded target is no longer in current workspace scope, whose registration is missing, or whose registration requires rebind is rejected instead of substituting another thread.
- Thread-navigation rendering must not synchronously call `codex app-server`, enumerate catalogs, refresh thread summaries, or read transcript history.
- Pending thread activation may change the active thread selector label to the target thread title only when paired with the stable in-button progress fill defined by Thread Selector. It must not change the label to an `Opening ...` or other transient loading presentation. Breadcrumbs continue to render from the last selected-thread branch projection until the activation result applies the new selected thread or reports failure.
- Pending thread activation must not blank or replace the transcript region with an `Opening ...` or loading placeholder. The prior transcript projection remains visible until the activation result applies the new selected thread or reports failure.
- Backward, forward, selector, transcript-link, and breadcrumb activation share the same one-shot transcript viewport rule: once the new thread's transcript is visible, activation-owned scroll state must already be final and must not be corrected by a later renderer callback.

## Thread Branching

- Branching creates a new Syndic conversation view from an active source view, preserves history through the clicked parent turn in the new view, removes later turns from the new view, and leaves the source view unchanged.
- If the clicked area belongs to an assistant response inside a parent turn, the branch keeps that assistant response because the target is the whole parent turn.
- Branching is unavailable when Beryl cannot identify a backend turn id and non-empty user input for the clicked turn, when the source thread is not idle, during selected-thread compaction or activation, or when app-server fork/rollback primitives are unavailable.
- Transcript-originated branch actions consume the context-menu target provided by the transcript feature. The action is unavailable unless the target has stable provenance and enough resident source data to prove the source thread, target turn, and ordered user input fragments needed by backend fork or rollback primitives.
- Every Beryl-created branch thread begins with a visible bootstrap user turn in backend history after fork/rollback or child-thread creation succeeds. The bootstrap turn records branch provenance such as `Branched from [<parent thread title>](beryl_threadid://<parent_thread_id>)` and may include branch-specific context. It is ordinary transcript history from the app-server perspective.
- The bootstrap turn is a Beryl-authored provenance turn, not real user exploration. It must not seed automatic title generation, count as child progress for feature workflows, or be hidden from the transcript.
- Bootstrap turns do not consume global developer-instructions or graph-upkeep hidden-context settings. Any branch-specific context needed by future model turns must be present in the visible bootstrap message.
- Beryl publishes a branch into durable GUI-local workspace state only after the branch's Syndic view and bootstrap capture state are durable. Durable branch publication includes graph refs, decision bindings, workspace registration, catalog visibility, and title scheduling. If bootstrap turn creation, terminal execution, or Syndic durability proof fails, Beryl shows an error and leaves none of that durable Beryl-owned branch state behind for the failed branch creation.
- `Branch and switch to` is a foreground branch workflow. After fork/rollback succeeds and the visible bootstrap turn is accepted with an exact turn id, Beryl immediately selects the new branch thread and attaches the shell UI to that bootstrap turn stream. The transcript shows the branched history plus the visible bootstrap user message while the bootstrap turn is running, and the status line represents the bootstrap as a Beryl-owned active turn with normal applicable foreground controls. Durable branch publication still waits for terminal bootstrap success and durability proof.
- If a foregrounded `Branch and switch to` bootstrap later fails or cannot be proven durable, Beryl reports the failure in the selected branch workflow and still leaves no graph ref, decision binding, workspace branch registration, catalog publication, or title-scheduling state for that failed branch. The transient CAS projection is not treated as a durable Beryl branch without successful Syndic and workspace publication.
- `Branch in background` is a background branch workflow. It registers and publishes the branch only after terminal bootstrap success and durability proof, schedules catalog refresh, and keeps the current active transcript selected while the branch is being prepared.
- When the selected thread is a Beryl-known branch, the toolbar renders its parent lineage as breadcrumbs after Workspaces, for example `Parent Thread > Parent Branch`. The thread strip renders `New Thread`, backward/forward controls, and the active thread title selector. Breadcrumb buttons hug their text label under normal toolbar space, while very long labels truncate inside the bounded breadcrumb trail so Graph and Settings remain reachable. The bounded trail must have enough ordinary desktop capacity for two max-width parent breadcrumb buttons plus their separator and gaps without clipping a button edge. Breadcrumbs use workspace refs, Syndic branch/view relationships, and current title precedence; they must not trigger backend reads or catalog refresh during rendering. If another thread activation is pending, the existing selected-thread breadcrumbs stay visible until the new thread is applied or activation fails. Parent breadcrumb segments activate the exact registered parent view when activation is available and become disabled when the parent is missing or requires rebind. During foreground `Branch and switch to`, transient branch state may supply the parent breadcrumb before durable branch publication succeeds, but must be replaced by workspace and Syndic state before catalog publication.
- Branched threads are Beryl-created threads. Their provisional title seed is the clicked turn's ordered user input fragments, not the source title or assistant output. Their first real user-authored branch turn triggers branch retitling as defined in Thread Display Titles.
- Branch orchestration runs away from the `gpui` thread and does not mutate source transcript presentation data.

## Thread Editing

- Thread editing is a user-initiated source-thread rewrite flow over CAS-owned execution history and Syndic-captured transcript state. It is not an in-place mutation of an existing turn id.
- Keeping the original thread intact is a branch workflow from the edited turn's parent, not an in-place edit of the original turn.
- Replacement editing removes the selected turn and its descendants from the current thread view by detaching the selected turn from its selected-path parent, then starts one replacement turn from the edited input at that parent.
- Beryl does not support an edit operation that creates a replacement turn while deleting the original edited turn and reconnecting the original descendants to the edited turn's parent.
- `Edit message` is enabled only when app-server rollback is available, Beryl can identify the target backend turn id, reconstruct non-empty user input, compute an exact trailing user-turn rollback count including the target turn, and prove the selected thread is idle with a current loaded tail.
- Transcript-originated edit actions consume the context-menu target provided by the transcript feature. The action is unavailable unless the target has stable provenance and enough resident source data to reconstruct the editable user input and prove the rollback scope.
- The turn context menu keeps `Edit message` visible as a disabled row when editing is unavailable for a clicked turn row. Its tooltip names the closest internal Beryl gate or target-resolution reason, such as missing rollback capability, current-tail unknown, pending selected-thread work, source-thread mismatch, unreconstructable input, missing image metadata, or unprovable rollback scope.
- `Edit message` is unavailable during context compaction, thread activation, active or queued turn submission, pending branch/edit work, and incomplete or stale history states.
- Starting edit mode requires an empty composer draft, closes the context menu, dims the target turn and later loaded turns, and populates the composer with the target turn's user input.
- Edit mode is presentation-only until commit. It must not mutate backend history, workspace state, transcript persistence, semantic graph state, image assets, or activity records.
- `Escape` cancels edit mode through the composer command path after higher-priority popups have handled the key. Canceling removes dimming without clearing or changing the composer draft.
- Submitting a non-empty edit draft first performs ordinary local draft validation and backend input preparation. If validation fails, edit mode remains active and the draft remains intact.
- After validation succeeds, edit commit rolls back the selected backend thread by the exact trailing user-turn count, records the truncation through the owning Syndic history boundary, resets visible transcript state from that boundary, and starts a replacement backend turn from the current draft.
- If selection changes while edit commit is in flight, rollback and replacement responses remain scoped to the original target thread and must not apply to unrelated visible transcripts.
- If rollback succeeds but replacement start or delivery fails, the detached tail remains absent from the current thread view. Beryl keeps the draft intact and reports the failure.
- Thread editing does not revert filesystem changes, semantic graph/checklist-item mutations, workspace state, thread title metadata, durable image assets, or other non-history side effects produced by discarded turns.

## Backend Requirements

- Thread activation depends on a workspace-registered Syndic conversation-view ref plus a storage-backed Syndic transcript provider for captured history.
- Branching depends on app-server fork, rollback, turn-start, and exact reopen primitives plus resident Syndic provenance for the source turn. When unavailable, Beryl must not emulate branching by copying backend history locally.
- Editing depends on app-server rollback and turn-start primitives plus resident Syndic provenance for the target turn. When unavailable or when exact rollback scope cannot be proven, Beryl must not emulate editing locally.
- On-demand title updates depend on a title-generation worker turn, accepted title parsing, and workspace title persistence. They must not use CAS thread-name metadata as title authority.
- Thread selector and link-thread menus must render from workspace-plus-Syndic catalog snapshots and stay responsive while refresh or activation work is pending.
