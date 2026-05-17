# Product Features

This document defines the user-visible product behavior for Beryl V1.

## Workspace Startup and Selection

- On successful application startup, Beryl automatically opens the previously active persisted Beryl workspace.
- If the previously active workspace is unavailable, deleted, or otherwise cannot be resumed, Beryl creates and opens a fresh untitled workspace instead.
- Missing or unavailable workspace member paths do not make the previously active workspace unavailable; Beryl opens the workspace, keeps those members attached, marks them invalid, and applies the durable primary fallback rules.
- A fresh untitled workspace starts with host-Windows as the default runtime, no explicit workspace members, and the host user's home directory exposed as the implicit primary workspace member.
- Successful workspace startup requires GUI-owned workspace state, not a successful `codex app-server` launch for the current primary runtime target.
- If the current primary runtime target cannot launch or probe a compatible `codex app-server` during startup, Beryl still opens the workspace screen, keeps workspace and member management available, and disables conversation operations for that target.
- Normal successful startup does not require a dedicated startup screen before the workspace window appears.
- Beryl stores persisted workspaces under the configured Beryl home directory, whose default is `~/.beryl`.
- New workspaces begin untitled and there may be any number of untitled workspaces.
- Untitled workspace display labels use a monotonically increasing sequence and are not renumbered after deletions.
- If a workspace is still untitled after its first completed assistant turn, Beryl best-effort auto-titles it asynchronously from that completed turn.
- Workspace titles map to filesystem-friendly workspace id slugs by transliteration and normalization.
- If two proposed workspace titles normalize to the same slug, they are not different enough to coexist as separate workspace names.
- A workspace title change is refused when the derived slug is empty or already belongs to another persisted workspace. Beryl does not automatically add a suffix to make the name unique.
- When a workspace title change is accepted, Beryl updates both the visible title and the workspace id slug.
- Failed or interrupted first turns do not auto-title the workspace.
- Manually renaming a workspace is available only when no workspace-scoped work is in progress or queued, and prevents later automatic overwrite.
- While manual rename is unavailable because workspace work is in progress, the rename control is disabled and its tooltip tells the user to wait until in-progress workspace work is finished.
- The main window supports one active Beryl workspace at a time.
- Concurrent work across multiple Beryl workspaces is handled by running multiple GUI instances.
- V1 requires a workspace picker popup widget rather than a dedicated workspace-picker screen.
- The main workspace toolbar includes a workspace-picker button that opens the workspace picker popup for workspace selection and active-workspace member management.
- The workspace-picker popup contains a left Workspaces column and a right Members column separated by a vertical divider.
- The Workspaces column includes a full-width filter field above a single divided workspace list.
- The Workspaces filter matches workspace names and explicit workspace member paths shown in workspace rows, including unavailable attached member paths.
- The workspace list's first row is `Create new workspace`, followed by existing workspaces ordered by most recently opened first.
- The currently active workspace is visually indicated by the row's left-edge accent marker only, without full-row primary-blue highlighting or redundant active/current label text.
- Each workspace row shows the workspace name as its primary text, followed by the workspace's explicit member paths, one member per line.
- Workspace rows do not render implicit-home member paths or `last updated` metadata.
- Each ordinary workspace row exposes a row-edge action menu containing `Rename` and hold-to-delete actions.
- The `last updated` timestamp changes when durable workspace state such as graph content, thread refs, titles, default-runtime selection, member registration, or primary-member designation changes, not merely when the user switches into that workspace or Beryl observes live member availability.
- Activating a workspace row switches to that workspace and closes the picker.
- Activating the currently active workspace row closes the picker without reloading the workspace.
- Completing the hold-to-delete action for the active workspace opens a fresh untitled workspace with the default host-Windows runtime and implicit home member.
- Completing a workspace hold-to-delete action removes only Beryl-owned local state and does not delete backend-owned Codex data.
- The picker closes on outside click, on `Escape`, and after successful row activation.
- The picker does not support keyboard row traversal or `Enter` row activation in V1.
- If switching to a selected workspace fails, Beryl keeps the current workspace active, closes no existing transcript state, and records the failure through the standard `tracing` log in V1.

## Runtime Environments, Workspace Members, and Conversation Threads

- Each Beryl workspace may have explicit workspace members from host-Windows and from any number of WSL distro runtime environments.
- A newly created workspace uses host-Windows as the default runtime environment.
- A workspace without a default runtime environment is a legacy or recovery state rather than the normal state of a new workspace.
- Default-runtime selection and workspace-member management are exposed in the workspace picker popup's Members column.
- Each explicit workspace member is one attached directory inside that member's own runtime environment.
- Explicit workspace members must not overlap after canonicalization within the same runtime environment.
- If an attached explicit member path cannot currently be resolved as a live directory, Beryl keeps it attached, displays it as invalid, and excludes it from new-thread execution and thread inventory.
- While a workspace has a default runtime environment but no available explicit members, Beryl shows that runtime environment's home directory as an implicit undeletable workspace member.
- The implicit home member acts as the primary member while no available explicit member exists.
- Attaching the first available explicit member removes the implicit home member from the member list and makes that explicit member the primary member.
- The primary workspace member is the concrete execution root used for newly created conversation threads in that workspace.
- Additional non-primary workspace members remain attached so the model can discover them as related workspace context through Beryl-provided app-server dynamic workspace metadata tools.
- Changing the default runtime environment affects future member attachment and implicit home fallback only; it does not move existing members, change existing thread refs, or restrict the workspace to one runtime.
- If no default runtime environment is selected in a legacy or recovery state, Beryl requires default-runtime selection before creating a new thread or attaching the first workspace member.
- Backend availability is tracked per runtime target. Missing `codex`, failed launch, failed probe, or incompatible app-server capabilities for host-Windows or one WSL distro disable only that target's backend-required conversation operations.
- A backend-unavailable runtime target is distinct from a missing default runtime and from an unavailable workspace-member path. It does not detach members, change the default runtime, promote another primary member, or silently rebind existing threads.
- A usable WSL runtime target remains attachable, primary-selectable, and conversation-capable while the host-Windows target is backend-unavailable, and the same isolation applies between WSL distro targets.
- Beryl stores images pasted into the composer as durable workspace-local image assets under the configured Beryl home directory, whose default is `~/.beryl`, so accepted transcript markers can still open previews after app restart.
- When submitting pasted images, Beryl gives Codex a real image path readable by the selected backend runtime. Host-Windows submissions use a host-readable asset path; WSL submissions use the selected distro's readable path to that same asset, such as its mounted view of the host profile directory, and fail visibly if that mapping cannot be validated.
- A conversation thread remains a backend-owned resource rather than a Beryl-owned graph node.
- Beryl uses backend-provided thread names as the preferred non-manual title source for listing and linking conversation threads.
- Beryl consumes backend thread names from thread metadata, member-thread inventory refreshes, and live backend thread-name update notifications.
- Beryl-created threads without a manual GUI-local title or backend-provided thread name become eligible for an automatic model-generated title after the first submitted user input fragment and backend thread id are known, including threads Beryl created before that fragment through graph or checklist start actions.
- Automatic Beryl thread-title generation runs on a background backend client connection for the target thread's runtime target, does not wait for the first assistant response or terminal turn state, and does not block transcript streaming, turn completion, or selector rendering.
- Automatic Beryl thread-title generation is disabled while that runtime target is backend-unavailable; the thread remains untitled unless another title source exists.
- Beryl runs automatic thread-title generation through one internal title-generation maintenance path that creates a fresh app-server ephemeral thread for each title attempt; that maintenance thread never appears in thread selectors, member-thread inventories, semantic graph thread refs, active-thread state, or transcript UI.
- Title-generation maintenance threads use only Beryl's fixed title-generation instructions and do not receive the global developer-instructions setting.
- Beryl requests lifecycle cleanup for each title-generation maintenance thread after its title attempt completes or fails.
- Beryl publishes accepted automatic thread titles to app-server through `thread/name/set` for the target conversation thread, then updates thread-listing UI from the title worker result, backend thread summaries, or backend thread-name update notifications.
- Threads created outside Beryl do not receive automatic Beryl-generated names in V1.
- Manual GUI-local thread titles take precedence over backend-provided thread names, including backend names that Beryl generated and published.
- Thread title generation and backend name propagation are asynchronous; until a title source exists, Beryl may show a single temporary untitled-thread label.
- Failed title generation or backend name propagation leaves the thread with the temporary untitled-thread label until a manual GUI-local title or backend-provided thread name exists; a later failure or interruption of the target conversation turn does not by itself cancel an already eligible title attempt.
- Automatic Beryl thread-title generation must not use a prompt-prefix heuristic and must not mutate the target conversation transcript.
- When no transcript text is selected, right-clicking a rendered area that belongs to a loaded parent conversation turn opens a transcript turn context menu for that turn.
- The transcript turn context menu includes `Edit message`, `Branch and switch to`, and `Branch in background`.
- When the right-click target is a loaded rendered transcript image, the same transcript turn context menu also includes `Copy image` and `Save image as` for that clicked image. The thread branching and edit options remain present and continue to target the owning conversation turn.
- `Copy image` copies the clicked transcript image to the system clipboard as image data.
- `Save image as` opens a native save picker so the user can choose the destination directory and file name for writing the clicked image.
- `Edit message` is enabled only when app-server rollback is available and Beryl can identify a backend turn id, reconstruct non-empty user input for that parent turn, compute an exact trailing user-turn rollback count including the target turn, and prove that the selected thread is idle with a current loaded tail. It is unavailable during selected-thread context compaction, thread activation, active or queued turn submission, pending branch or edit work, and history states where the tail count is incomplete or stale.
- If the composer draft is non-empty, the context menu still shows `Edit message` disabled with the tooltip `Composer must be empty to edit a message`. Composer non-empty detection includes image markers as well as text.
- Starting `Edit message` enters transient thread-edit mode, closes the turn context menu, dims the targeted turn and all later loaded turns, and populates the composer with the targeted turn's user input. Because the action is disabled when the composer is non-empty, starting edit mode never overwrites an existing user draft.
- Thread-edit mode is presentation-only until commit. It does not mutate backend history, Beryl workspace state, or transcript persistence merely by dimming the future discarded tail.
- Pressing `Escape` while thread-edit mode owns the active composer command cancels edit mode and restores ordinary transcript presentation without clearing or otherwise changing the composer draft.
- Submitting a non-empty composer draft in thread-edit mode first performs the same local draft validation and backend input preparation required for ordinary submission. If that pre-rollback validation fails, Beryl keeps edit mode active, keeps the composer draft intact, and reports the rejection.
- After pre-rollback validation succeeds, edit commit rolls back the selected backend thread by the computed trailing user-turn count, including the edited turn, resets visible transcript state from the rollback response, and starts a new backend turn from the current composer draft on that same thread.
- If the active selection changes while an edit commit is in flight, the commit remains targeted to the original backend thread. Rollback responses, replacement-start responses, retry state, and failure presentation must be scoped to that original thread and must not be applied to an unrelated visible transcript.
- If rollback succeeds but replacement turn start or delivery fails, the discarded tail remains deleted. Beryl keeps the composer draft intact and reports the failure so the user can retry or manually decide what to do next.
- Thread editing is backend-history rollback only. It does not revert filesystem changes, Beryl semantic graph or checklist mutations, workspace state, thread-title metadata, durable image assets, in-memory activity records, or other non-history side effects produced by discarded turns.
- Branching creates a new backend-owned Codex conversation thread from the active source thread, preserves history through the clicked parent turn in the new thread, removes later turns from the new thread, and leaves the source thread unchanged.
- If the clicked area belongs to an assistant response inside a parent turn, the branch keeps that assistant response because the branch target is the whole parent turn.
- Branching is unavailable when Beryl cannot identify a backend turn id and non-empty user input for the clicked turn, when the source thread is not idle, when selected-thread context compaction or thread activation is in progress, or when the backend does not expose the required fork and rollback primitives.
- `Branch and switch to` activates the newly created branch only after fork, rollback, branch registration, and initial transcript activation succeed.
- `Branch in background` registers the newly created branch, schedules thread inventory refresh, and keeps the current active transcript selected.
- The branched thread becomes eligible for automatic Beryl thread-title generation using the clicked turn's user input fragments as the title seed rather than the source thread's title or assistant output.
- Beryl maintains a UI-facing member-thread inventory snapshot for the active workspace, grouped by available runtime-bound workspace member.
- Available workspace members for the inventory are the available explicit workspace members, or the implicit home member when a default runtime environment is selected and no available explicit members exist.
- Unavailable explicit workspace members remain visible in member management UI but do not contribute inventory groups.
- A thread is listed under a member only when the backend thread summary's recorded runtime and working directory exactly match that member's runtime and canonical path.
- Member-thread inventory refresh runs in the background and atomically swaps complete snapshots into UI state, so opening a thread-linking menu or thread selector never blocks on `codex app-server`.
- The thread selector uses the same member-thread inventory snapshot as thread-linking UI.
- Opening the thread selector may request a background member-thread inventory refresh for backend-available targets, but it displays the latest available snapshot while refresh is pending, unavailable, or after refresh failure.
- Threads inside each member group are ordered by `last updated` descending.
- If no conversation thread is active and the user submits input from the workspace screen, Beryl creates and activates a new standalone Codex thread for the current workspace using the current primary workspace member.
- Starting a new Codex thread requires a default runtime environment, a resolved primary workspace member, and a backend-available runtime target for that member.
- Composer submission, `New Thread`, graph-started thread creation, checklist-started thread creation, branch, edit, active-turn steering, context compaction, and lifecycle continuation are unavailable while their selected runtime target is backend-unavailable.
- When the global developer-instructions setting is non-empty, Beryl sends the latest applied value as hidden developer-instructions context with each top-level user message, including messages sent to existing threads and the first message that creates a new user-facing persistent Codex thread.
- Automatic lifecycle continuation after `yield(phase_continue)` also sends the latest applied developer-instructions setting with Beryl's generated continuation message.
- Blank or whitespace-only developer-instructions settings are disabled and clear Beryl's custom developer-instructions context for later top-level user messages. Developer instructions are not sent with subagent requests, active-turn steering, title-generation maintenance, inventory refresh, context-compaction requests themselves, or other background/status-only work.
- Developer-instructions settings are applied at send time, so existing threads, retries, and regeneration-style replacement starts use the current setting without copying the injected instructions into user-visible transcript history or user-message text.
- If Beryl cannot determine the effective model required by the backend's hidden developer-instructions mechanism, it omits the hidden developer-instructions request data rather than guessing a model.
- Activating an existing thread from the thread selector or from a graph thread ref reopens that exact backend thread by id and does not fall back to another thread or runtime target if the selected thread or its runtime target is unavailable.
- Existing-thread activation shows an immediate pending state while the backend resume and initial transcript page load are in progress.
- Existing-thread activation does not require enumerating all backend threads before the selected thread can begin reopening.
- Selecting a valid thread-ref item in the graph explorer activates that Codex thread in the transcript.
- Invalid thread-ref items remain visible, show an invalid-link indicator, and report why the linked thread is unavailable instead of activating a transcript.
- Selecting a semantic node by itself does not switch the active transcript thread.
- Double-clicking a topic-capable semantic node creates and activates a new Codex thread attached to that existing node, using the current primary workspace member.
- If no default runtime environment is selected in a legacy or recovery state, topic-capable node thread creation is unavailable and Beryl reports that the action cannot be completed.
- Existing conversation threads may change bound workspace member or runtime environment only through an explicit rebind decision.
- Beryl never silently hops an existing conversation thread to a different workspace member or runtime environment.
- If a thread ref's original bound member, runtime environment, or backend thread id is no longer in workspace scope, Beryl keeps the ref, marks it invalid, and requires an explicit rebind decision before continuing that ref on another workspace member.

## Workspace Members Column

- The workspace picker popup's Members column manages the active workspace's default runtime environment and workspace members without replacing the main workspace screen.
- The Members column copies the Workspaces column visual structure: a compact header, a fixed control row, a single divided list, left-edge accent row-state treatment, row dividers, soft-wrapping text, and row-edge action-menu triggers.
- The Members column fixed control row is the default-runtime and attachment-runtime selector. It replaces a member filter; the Members column does not have its own filter field.
- The runtime-environment selector opens an attached selector dropdown whose outer boundary aligns with the trigger so the selector and dropdown read as one continuous control.
- Runtime selector rows include host-Windows and available WSL distros. WSL distro rows render with a `WSL: ` prefix.
- The runtime-environment selector remains enabled with explicit members attached because it controls future attachment and implicit home fallback rather than constraining existing members.
- If the workspace has no default runtime environment, the Members column requires choosing host-Windows or one WSL distro before allowing member attachment.
- The Members list's first row is `Attach member`, which opens the native OS file picker.
- Beryl validates the chosen member path after the picker returns and rejects selections outside the runtime environment selected for that attachment.
- Host-Windows attachments reject WSL UNC selections.
- WSL attachments accept only UNC selections inside the selected distro and reject host paths or UNC selections from other distros.
- The default and attachment runtime is presented by the runtime selector rather than as a member row.
- When no available explicit members exist, the Members list shows the default runtime environment's implicit home member as the current primary member and does not allow detaching it. Host-Windows uses the host user's home directory; WSL uses the selected distro's home directory.
- Attached explicit members render as divided-list rows using the same text hierarchy as workspace rows: a primary display label derived from the member directory and a secondary full filesystem path. Long labels and paths soft-wrap and grow the row vertically instead of truncating.
- An unavailable explicit member row remains in the list. Its primary line appends `- path not found` to the normal display label, its secondary line remains the persisted full filesystem path, and it is not eligible for `Make primary`.
- The current primary member is visually indicated by the same left-edge accent marker used for the active workspace, without full-row primary-blue highlighting or redundant primary/current label text.
- Each explicit member row exposes one row-edge action menu for member actions. Non-primary explicit member menus include `Make primary`, and explicit member menus include a detach action that asks for confirmation.
- If the current primary explicit member is detached or becomes unavailable while other available explicit members remain, Beryl deterministically and durably promotes the earliest available explicit member by stable attach order to primary.
- If no available explicit members remain, the implicit home member reappears and durably becomes primary.

## Semantic Graph and Workspace Organization

- The canonical Beryl graph is semantic rather than conversational.
- V1 semantic nodes use constrained facet combinations rather than one exclusive type tag.
- V1 semantic facets are `Topic`, `Checklist`, and `ChecklistItem`.
- Hard parent/child links form the primary ordered single-parent forest of semantic nodes within a workspace.
- Root-level semantic nodes have durable graph-owned order.
- Soft typed links such as `depends_on` and `informs` may connect nodes inside one hard-tree component or across different root-level hard-tree components.
- Soft links and thread refs are visually represented in the graph explorer as terminal link rows rather than as additional tree nodes.
- Workspace members and member-thread inventories are not represented as graph nodes.
- Nodes store short titles plus summaries suitable for later reuse when starting new Codex work from that node.
- Checklist-capable nodes own ordered checklist-item nodes.
- Checklist-item nodes are first-class semantic nodes, with visible status such as `todo`, `in_progress`, and `done`, and are topic-capable in V1 so Codex work can start directly from the item without creating an extra child node.
- Beryl may let the model update the semantic graph and checklist state during ordinary conversation through app-server dynamic tools registered by Beryl.
- Semantic graph and checklist updates from either GUI actions or Beryl dynamic tools appear in the graph explorer without closing, blanking, or rebuilding the visible graph scene as the ordinary success path.
- Successful Beryl dynamic graph tool writes are durable before the tool result is returned.
- Failed graph writes show localized error or recovery state while preserving unaffected visible graph selection, scroll, and expansion state.
- Workspace-member inventory, primary-member designation, and runtime-environment metadata may also be exposed through Beryl-provided app-server dynamic tools so the model can discover the workspace's attached filesystem roots without Beryl preloading all contents into prompt context.
- Deleting a hard-tree leaf semantic node deletes only that node. Deleting a semantic node recursively deletes that node and its hard descendants only, whether or not the target is root-level. Deleting one root-level subtree preserves unrelated root-level subtrees. Soft links are not followed to expand the recursively deleted set, but any soft link whose source or target is deleted is removed so the graph does not retain dangling links.
- Thread refs attached to deleted semantic nodes are removed from Beryl's semantic graph state, but deleting a semantic node never deletes backend-owned Codex conversation threads.
- Durable top-level workspace creation, deletion, retitling, default-runtime selection, and member-management actions remain user-visible actions even when proposed or initiated by the model.

## AI Lifecycle Yield

- Beryl may expose a `yield` dynamic tool so the model can request a semantic lifecycle handoff without controlling Beryl's runtime mechanics.
- The `yield` tool accepts one `outcome` value.
- `phase_needs_review` stops after the current turn so the operator can review or live-test the completed phase.
- `blocked_needs_operator` stops after the current turn and requests an operator-attention notification.
- `phase_continue` lets Beryl continue without manual operator typing: after the current turn finishes, Beryl runs selected-thread context compaction and starts the next turn with Beryl's fixed continuation message.
- `plan_complete` stops after the current turn and requests a completion notification.
- The model does not choose whether Beryl compacts, whether Beryl resumes, the resume text, notification sounds, or notification focus policy.
- Automatic lifecycle continuation does not play the ordinary end-turn sound for the turn that requested continuation.

## Workspace Screen

- The workspace screen includes a toolbar strip, one scrollable transcript surface, one optional activity panel, one pinned user input panel, one fixed bottom status line strip, and an optional checklist sidebar on the right edge.
- The toolbar strip is a controls-only row and does not reserve a static leading text area.
- The toolbar strip includes the workspace-picker button for opening the merged workspace and member-management popup.
- The toolbar strip includes an `Activity` mode control for the activity panel.
- The toolbar strip does not include static workspace-name text, a thread-count label, a visible graph-overlay shortcut label, or non-interactive status chips.
- The workspace screen includes a thread strip beneath the toolbar with a `New Thread` button, the active thread title control, and non-host runtime context when needed.
- Activating the active thread title control opens the thread selector without requiring semantic graph interaction.
- The pinned user input panel wraps draft content at the field's visible width, grows and shrinks vertically with the wrapped line count up to half the OS window height, and then scrolls internally when more draft content remains.
- The user input panel is not manually resizable and does not expose a draggable transcript/input separator.
- The user input field does not horizontally scroll.
- The user input panel does not render a persistent `Run Turn` or submit button; focused `Enter` submits the draft, and focused `Shift+Enter` inserts a real newline.
- Pasting image clipboard content into the conversation composer inserts an inline image marker at the current draft caret or replaces the current draft selection, and stores the original image bytes as a Beryl-owned workspace image asset for preview and backend delivery.
- Pasted images are labeled from the selected thread's monotonic image-label sequence as `A`, `B`, `C`, continuing with spreadsheet-style labels such as `AA` if needed. The composer renders each image as a compact visual marker shaped like `[A]` at its inline draft position.
- Image marker labels remain stable while the draft, accepted fragment, queued fragment, or retry state exists. Removing image `[B]` does not rename later image markers or allow `[B]` to be reused for another image in that conversation thread, because the typed draft text may already refer to those labels. Multiple markers may show the same label only when they are references to the same pasted image.
- Existing-thread image paste is unavailable until Beryl has discovered prior image labels from that thread well enough to allocate the next label without colliding with older history.
- The visual `[A]` marker is not submitted as literal user-authored text. On submission, Beryl sends the original image data at the first ordered position for that image and adds generated label text such as `Image A:` immediately before the image part so Codex can connect user text such as `image A` to the corresponding image. Later markers that reference the same image are submitted as generated text references such as `[Image A]`, not as duplicate image data.
- Copying or cutting a composer selection that contains an image marker writes explanatory marker text such as `[Image A]` to the system clipboard so the copied text remains meaningful outside Beryl. Beryl also writes private clipboard metadata that lets another Beryl composer paste restore the marker as an atomic image reference while the transient payload is still live.
- Pasting copied Beryl image-marker metadata back into the same conversation scope creates another marker reference to the same image and keeps the same label. Cutting and pasting a marker therefore moves that image reference within the text, while copying and pasting creates an additional reference without attaching duplicate image data.
- Pasting copied Beryl image-marker metadata into a different conversation or pending-new-thread scope allocates fresh labels from that target scope, subject to the same prior-label discovery readiness as ordinary image clipboard paste.
- Pasting plain text that merely looks like `[Image A]` inserts plain text only and never creates an image attachment.
- Clicking an image marker opens a context menu with `View` and `Remove`.
- `View` opens a Beryl popup panel showing a larger fitted preview of the original durable image data without submitting the draft or opening the OS default image viewer.
- `Remove` deletes that image marker occurrence from the composer draft. If other markers still reference the same image, the image remains available; otherwise its associated image data is removed from the draft. If the image has already been accepted into a queued fragment, removing it from the draft does not mutate the already accepted transcript fragment.
- A draft containing one or more image markers and no non-whitespace text is non-empty for submission purposes.
- If Beryl cannot store an image asset, derive a runtime-readable image path for the selected backend runtime, or serialize the image-containing draft for backend delivery, the submission is rejected, the draft remains intact, and Beryl reports the failure rather than clearing text or image markers.
- When the user input field is focused, `Ctrl+Up` and `Ctrl+Down` scroll the transcript between turn boundaries without changing the draft caret or selection.
- If the transcript viewport is already inside a tall turn, focused `Ctrl+Up` first moves to the top of that current turn before later jumps move to earlier turns.
- If focused `Ctrl+Down` has no later turn boundary to target, it scrolls to the transcript bottom so repeated downward jumps can reach the end of a large final turn.
- When the user input field is focused and thread-edit mode is inactive, `Alt+Up` browses to older accepted composer submissions for the current conversation scope, and `Alt+Down` browses toward newer submissions and then back to the draft that existed before history browsing began.
- Composer history is GUI-local in-memory session state scoped to the selected backend thread or to the pending-new-thread draft. It is not persisted, is not seeded by loading backend transcript history, and does not trigger backend reads, submissions, or transcript mutations while browsing.
- Each conversation scope keeps a bounded composer history list. When that bound is exceeded, Beryl evicts the oldest entries in that scope rather than allowing unbounded growth.
- When a pending-new-thread draft creates a backend conversation thread, the draft's composer history scope follows the newly created thread so the first accepted submission remains browseable after thread creation.
- Only submissions accepted into the transcript enter composer history. Rejected submissions, empty submissions, and whitespace-only text submissions do not enter history, and consecutive duplicate accepted drafts collapse to one history entry.
- History browsing replaces the current composer draft with an editable copy of the selected history entry, including restorable image atoms. The draft that existed when browsing began is captured exactly and restored when browsing forward past the newest history entry.
- Recalled history entries place the caret at the end of the restored draft and clear any selection. Editing a recalled entry changes only the current draft and does not mutate the stored history entry.
- While thread-edit mode is active, `Alt+Up` and `Alt+Down` do not browse composer history or replace the edit-mode draft.
- If there is no history entry in the requested direction, `Alt+Up` and `Alt+Down` leave the draft, caret, selection, and transcript unchanged.
- When a non-empty draft is accepted for submission and added to the transcript, the user input draft clears immediately; rejected submissions keep the draft intact.
- Each accepted composer submission is one distinct user input fragment. If the user submits multiple fragments that belong to the same backend turn, Beryl renders them as separate user blocks in that turn rather than merging them into one prompt.
- During selected-thread context compaction, focused `Enter` still accepts a non-empty composer draft for that same thread, clears the draft, and shows that fragment immediately in the transcript.
- If multiple fragments are accepted while compaction is in progress, they remain separate visible user blocks, and Beryl sends them to Codex as one ordered turn input after compaction finishes.
- During an ordinary active turn, focused `Enter` accepts a non-empty composer draft and sends it as active-turn steering when Codex can accept steering for that turn.
- If a submitted active-turn fragment cannot be steered because the backend reports that the active turn is not steerable, Beryl keeps that accepted fragment as queued input for the next turn rather than discarding it.
- Beryl remembers the latest draft insertion point so transcript quote actions can insert text into the draft even while the transcript is the active reading surface.
- The bottom status line strip sits between the user input panel and the OS window bottom edge and uses the same edge-to-edge separator treatment as the main toolbar.
- The status line strip contains three left-to-right cells: model/reasoning, context space left, and last-turn state.
- The model/reasoning cell displays the selected thread's active or pending model and reasoning effort. After `New Thread` clears the active backend thread, the cell displays the explicit new-thread draft selection if one exists; otherwise it displays the current effective backend defaults that would be used for the first submitted turn. It uses `Unknown` when the backend configuration does not expose an effective value, and it does not infer the effective reasoning value from model-list menu defaults.
- Backend-derived status values display as unavailable or `Unknown` without launching or probing a backend when the selected runtime target is backend-unavailable.
- When a backend conversation thread is selected and idle, or when the workspace is on a pending new-thread draft, activating the model/reasoning cell opens a popup for choosing model and reasoning effort.
- Model/reasoning changes apply only to the selected thread or pending new-thread draft, affect the next submitted turn for that thread and later turns, and do not change global Codex configuration.
- A pending new-thread draft with no explicit model/reasoning selection follows changes to the current effective backend defaults until the first submitted turn or until the user chooses an explicit draft selection.
- If the selected thread has an active turn, the model/reasoning cell is non-clickable.
- Context space left displays `Unknown` until exact token usage is available for the selected thread with a positive model context window; when available, the percentage is derived as `((modelContextWindow - last.inputTokens) / modelContextWindow) * 100`, clamped to `0..100`. Beryl eagerly reads exact account rate-limit status after backend startup, selects the bucket for the active model, appends available short-window and weekly remaining percentages independently, and preserves any bucket not included in later partial update notifications.
- Exact context token usage may come from the selected thread's latest `thread/tokenUsage/updated` notification, from an in-memory same-thread cache populated by a previous notification, from a durable GUI-held last-known snapshot originally populated by a notification, or from read-only app-server status metadata when the protocol exposes it.
- Switching threads does not submit user input, start a backend turn, or mutate backend-owned conversation history just to refresh the context cell.
- When a backend conversation thread is selected and idle, activating the context cell opens a context operations popup with a `Compact` action.
- `Compact` starts backend context compaction for the selected thread; request acceptance does not mean compaction has already completed.
- If no thread is selected, or if the selected thread has an active turn, the context cell is non-clickable.
- Last-turn state displays `compacting` while selected-thread context compaction is active, `working` while a parent turn is active, `ok` after the latest completed turn, `error` after the latest failed or interrupted turn, and `Unknown` before any turn state is known.
- When a selected user-visible parent turn fails with a backend error payload or local turn-delivery failure, Beryl enqueues a surface notice titled `Turn error` with the available error detail instead of relying only on the compact status-line `error` value.
- Interrupted turns that have no actual error payload update the status line but do not enqueue a turn-error notice.
- When the selected thread has an active ordinary turn with a known interruptible backend turn id, activating the last-turn state cell opens a turn operations popup with `Soft stop` and, when hard-stop targets are known, `Hard stop`.
- When selected-thread context compaction is active and Beryl knows an interruptible backend turn id for that compaction operation, activating the last-turn state cell opens the same turn operations popup with `Soft stop`; `Hard stop` is available only for exact backend-exposed execution targets associated with that operation.
- If no selected-thread turn is active, or if the active selected-thread operation has no known interruptible backend turn id, the last-turn state cell is non-clickable.
- `Soft stop` requests backend interruption for the exact selected-thread active turn and then closes or reports request failure through normal popup feedback. Request acceptance does not mean the turn has already reached a terminal interrupted state.
- `Hard stop` performs the same selected-turn interruption as `Soft stop`, then best-effort terminates known running execution associated with that selected turn through exact backend-exposed handles. It may interrupt known active subagent turns, terminate process-backed command execution handles, and request thread-scoped background-terminal cleanup when those targets are known and supported.
- `Hard stop` is activated only by holding its popup row for three seconds. While the pointer or keyboard activation is held, the row background fills from left to right; releasing early, leaving the row, closing the popup, focus loss, or active-turn target change cancels the hold without sending the hard-stop request.
- If Beryl cannot map a running tool or subagent to an exact backend termination handle, `Hard stop` does not guess an OS process id or process tree and reports the unsupported target when relevant.
- User input fragments already accepted before or during a stop request remain visible and ordered. If they cannot be delivered to the interrupted turn, Beryl keeps them queued for the next eligible turn instead of dropping or merging them.
- The toolbar `Activity` control cycles through `Activity Auto`, `Activity On`, and `Activity Off`.
- New workspace UI state defaults to `Activity Auto`.
- In `Activity Auto`, the activity panel appears as soon as a parent turn is accepted on the conversation surface and remains visible until that turn ends; it also appears while selected-thread context compaction is active. Outside those active-work periods, it is hidden and consumes no conversation-column height.
- In `Activity On`, the activity panel appears between the transcript and the user input panel even when it currently has no rows.
- In `Activity Off`, the activity panel is hidden and consumes no conversation-column height.
- Beryl persists the activity panel mode and panel height as workspace-scoped GUI-local state across app restarts.
- The activity panel is vertically resizable by dragging its top border, taking space away from or giving space back to the transcript region.
- Activity history is kept in memory for the current app process, survives thread switching within the loaded workspace, and is discarded on app restart or workspace/backend-session reset.
- Each observed backend activity item renders as one fixed-height single-line row in the form `Agent <agent label> Activity <activity display value>`.
- Subagent activity rows use the backend-provided subagent nickname for `<agent label>` when it has been resolved. While the nickname is unresolved, the label value is empty; backend thread ids are not shown as fallback agent labels.
- Running rows stay in the panel while the backend item is active, and finished rows remain in the in-memory activity history after completion.
- Rows sort with running activity first, finished activity second, and newly started rows before older rows within those groups.
- Running, finished-ok, and finished-error rows show themed status marker discs before the row text.
- In activity rows, `Agent` and `Activity` use muted status-label styling while the values use status-value styling.
- V1 activity rows show protocol-derived activity display values without broad human-friendly mappings. `commandExecution` rows show the first non-empty line of the spawned command, falling back to `commandExecution` when the command is unavailable. Before display, if the first quoted or unquoted command token case-insensitively matches a drive-rooted Windows PowerShell launcher path shaped as `[drive]:\Windows(\.old)?\System32\WindowsPowerShell\v1.0\powershell.exe`, including the activity-log form with doubled backslashes such as `"D:\\Windows.old\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"`, that token is replaced with `powershell.exe` while preserving the rest of the command line. Reasoning rows show `reasoning` and, when backend summary text is exposed, a bounded single-line `reasoning: <summary>` value. `fileChange` rows show `Patching <relative/path>, +A -D` only when explicit backend file-change records identify exactly one unique path and that path is relative or can be proven to be under the selected conversation execution target root; otherwise they show `Patching N file(s), +A -D`. Other activity rows show raw protocol-derived tool names or resource identifiers. Rows do not show output, progress messages, resource contents, file paths other than that single relative `fileChange` path, patch diffs, raw reasoning content, or expanded operational detail.
- The activity panel owns vertical scrolling when rows exceed the current panel height, and its default viewport position is the top of the sorted row list.
- Scroll-owning surfaces render a narrow thumb-only scrollbar overlay rather than a full track.
- That scrollbar thumb appears only after pointer movement or active scrolling within the owning scrollable area and only when the surface currently has overflow.
- After pointer movement and scrolling both stop, the scrollbar thumb fades in and out around a short inactivity delay instead of appearing or disappearing abruptly, with opacity interpolation that tracks successive render frames while the transition is active.
- Users can drag the visible scrollbar thumb by click-and-hold to directly change the owning surface's scroll position.
- Clicking the invisible vertical scrollbar lane outside the current thumb scrolls one full viewport height toward the click: above the thumb scrolls up, and below the thumb scrolls down.
- Keyboard scrolling acts on the currently focused or routed scrollable area rather than on the scrollbar overlay.
- Scrollbar dragging and lane clicks follow the owning scroll surface's bounds. The main transcript keeps its transcript-specific bottom-following and virtual-tail behavior; other scrollable surfaces do not inherit those transcript rules.
- Streaming scroll surfaces may provide a bounded virtual trailing scroll allowance that lets the user scroll slightly past the last content line without losing visual orientation.
- Virtual trailing scroll allowance is not content; it affects scroll reach and scrollbar geometry without adding a visible content row.
- Hovering an overflowed code block or other nested transcript scroll container may reveal that container's scrollbar according to the same fade rules, even while the transcript still owns vertical wheel scrolling.
- Nested transcript scroll containers consume vertical wheel or touchpad scrolling only after the user selects that container by clicking it.
- Vertical wheel or touchpad scrolling over an unselected nested transcript scroll container scrolls the outer transcript instead of the nested container.
- When a nested transcript scroll container is selected, vertical wheel or touchpad scrolling over it scrolls only that container and does not also scroll the outer transcript.
- Clicking another nested transcript scroll container selects that container for wheel ownership, and clicking ordinary transcript space returns wheel ownership to the transcript.
- Pressing `Escape` does not return nested transcript wheel ownership to the transcript.
- The transcript is the stable parent conversation narrative for the active backend thread rather than a complete operational event log.
- The transcript includes ordered user input fragments plus parent assistant narrative items, including parent commentary, final answers, and optional parent-turn reasoning summaries when exposed by the backend.
- When an existing thread is reloaded, historical user-message content that the backend exposes as separate input items remains visually separate in Beryl's transcript.
- User input fragments that contain images render their image positions as compact atomic labels such as `[A]` inside the user block. The transcript must preserve the relative order between text and image markers rather than flattening image records to unlabeled fallback text.
- Clicking a transcript image marker opens the same Beryl preview panel used by composer image markers when durable image bytes are available. Historical markers remain visible as atomic markers even if their image bytes cannot be recovered.
- AI-generated raster images returned by app-server render directly in the transcript. While generation is pending, Beryl may show a stable media placeholder; when the generated bytes or saved generated-image path becomes available, the placeholder is replaced by the image.
- Markdown image syntax with a local file path, such as `![alt text](relative-or-absolute-path.png)`, renders supported raster image files directly in the transcript when Beryl can resolve and read the file for the selected conversation thread. Ordinary Markdown links remain links, including linked images such as `[![alt](path)](target)`.
- Supported transcript image rendering is limited to raster images, with PNG required. SVG and other non-raster image formats are not rendered inline in this phase.
- If a Markdown image target cannot be rendered because the format is unsupported, Beryl shows `<alt text> (render not supported)`. If the file is missing or unreadable, Beryl shows `<alt text> (file unavailable)`. If the path is outside the allowed conversation runtime/member boundary, Beryl shows `<alt text> (path not allowed)`.
- Consecutive transcript images form their own media row. One image occupies the full transcript row, expands up to the available padded transcript content width without exceeding its natural raster size, and is horizontally centered when it renders narrower than that padded width. Two or more consecutive images sit side by side at a shared compact width equal to about 30 `M` glyph advances in the active regular conversation text font, with each image capped at its natural raster size. Multi-image rows use the same transcript side padding as surrounding content and wrap at the right edge. Consecutive Markdown image embeds are treated this way even when the same Markdown paragraph also contains prose, with text before and after the image sequence rendered as normal text around the media row.
- Clicking a loaded image inside a multi-image transcript row temporarily promotes that image into its own full-width row using the same sizing as a single-image transcript row. Other images from the same row remain compact before or after the promoted image in their original order, even when only one other image remains on one side. Clicking the promoted image again returns it to the compact multi-image row.
- The promoted-image state is local transcript presentation state. It does not change backend conversation history, Markdown source, generated-image output, image bytes, clipboard text-copy behavior, or persisted workspace data.
- The transcript omits asynchronous or operational activity that is not itself parent assistant narrative, including command execution records and output, file-change records, subagent transcripts, tool or MCP calls, title-generation maintenance turns, raw backend lifecycle notifications, status updates, and token-usage updates.
- Beryl does not insert transcript rows whose only purpose is to say that commands, tools, subagents, reasoning work, or background work ran; transient activity history belongs only in the separate activity panel when that panel is visible.
- When the selected thread has an active parent turn, the transcript renders a non-interactive block activity caret at the end of the parent conversation narrative.
- The activity caret is presentation-only: it is not selectable, copyable, quoteable, included in Markdown parsing, included in transcript render metrics, or controllable as a text caret.
- The activity caret blinks without changing layout geometry, disappears when the parent turn leaves the `working` state, and follows platform text-caret blink policy when available. If platform text-caret blinking is disabled, or if only a general reduced-motion signal is available and it requests reduced motion, the activity caret renders steadily.
- Transcript text is selectable with ordinary desktop text selection mechanics.
- Copying selected transcript text through standard copy commands writes Markdown-preserving selected text to the system clipboard rather than the lossy rendered-only presentation.
- Copied transcript selections preserve Markdown syntax for selected semantic constructs such as inline code, emphasis, links, lists, block quotes, headings, code blocks, and image markers. Selected transcript image markers copy as explanatory text such as `[Image A]`.
- Selecting across a Markdown code block copies that portion as Markdown code-block source, while the code block's own copy action copies only bare code.
- Selecting non-empty transcript text shows a small quote popup panel near the selection with a `Quote` action; this popup is not part of the app toolbar.
- Activating `Quote` inserts the Markdown-preserving selected transcript text into the current draft as Markdown block quote text by prefixing each selected logical line with `> `.
- Quote insertion uses the latest remembered draft insertion point, or appends to the end of the draft if no insertion point is known.
- After each quote insertion, the remembered insertion point moves after the inserted quote block so multiple quote actions can collect passages into the draft in reading order.
- Beryl separates gathered quote blocks from surrounding draft content with blank-line spacing so the user can later type responses between or beneath them.
- Activating `Quote` preserves the transcript scroll position and does not force keyboard focus into the user input field, allowing the user to keep reading a long response while gathering quotes.
- Activating `Quote` does not mutate the system clipboard.
- Reloading an existing Codex thread reconstructs the same parent conversation narrative from backend-provided historical thread data loaded in bounded turn pages.
- The initial existing-thread load fetches the latest turn page first, renders it at the transcript tail, and fetches older pages as the user scrolls toward earlier unloaded history.
- The transcript remains responsive on large threads by rendering from the visible viewport plus a small buffer rather than rebuilding presentation for every fetched turn on each scroll frame.
- Fetched transcript pages are transient presentation data; Beryl may retain nearby pages for smooth navigation or release offscreen pages to keep memory and scroll work bounded.
- When an existing thread history page is loaded without a submit-time anchor, the transcript viewport opens at the real end of the loaded thread window while still allowing the user to scroll past the last line until the latest loaded user input fragment's last rendered line can reach the top of the transcript area.
- After a user input fragment is accepted, the transcript viewport is positioned so the last rendered line of that fragment is first visible at the top of the transcript area, and the assistant response streams into the remaining space below it when a response is active.
- If the submitted fragment is taller than the transcript area, earlier fragment lines may sit above the viewport; if the response overflows the visible area, it may continue below the viewport without automatic follow-scrolling.
- If the response content below the latest fragment's last rendered line is too short to fill the viewport, Beryl uses shared virtual trailing scroll allowance so the user can scroll until that fragment line reaches the top of the transcript area, including after loading an existing thread history.
- That trailing scroll allowance shrinks as real response content grows and disappears once the real response content makes the fragment-line-at-top position naturally reachable.
- If the user manually scrolls the transcript during or after that turn, Beryl stops forcing the submit-time anchor but keeps the trailing scroll allowance while it is still needed; a later accepted fragment replaces the prior anchor.
- The checklist sidebar is hidden by default.
- When visible, the checklist sidebar remains horizontally resizable through a draggable separator between the transcript column and the sidebar.
- Selecting a checklist-capable node auto-shows the checklist sidebar and populates it with that checklist.
- Checklist sidebar content updates from GUI graph actions or Beryl dynamic graph tools without closing the sidebar or replacing the graph overlay when the selected checklist remains valid.
- If the selected checklist is deleted or loses checklist capability, Beryl clears or hides only the invalidated checklist sidebar state while preserving unrelated workspace and graph overlay state.
- Failed checklist-affecting graph writes show localized error or recovery state without discarding unaffected checklist sidebar scroll or graph selection state.
- The checklist sidebar can be hidden explicitly through a toolbar action.
- The checklist sidebar presents a flat vertically scrollable numbered list of wrapped checklist-item rows and does not support horizontal scrolling.
- Right-clicking a checklist-item row opens a context menu that includes `Start New Codex Thread`.
- `Start New Codex Thread` on a checklist-item row creates and activates a new Codex thread attached to that existing checklist-item node rather than creating a new semantic child node, and it uses the current primary workspace member.
- Code blocks appear in visually distinct bordered or paneled treatments.
- If an agent turn produces a non-image file artifact that exists on the local filesystem, the GUI represents that artifact as a clickable file link and asks the OS to open it with the default associated application.
- Local filesystem image references render inline only through native app-server generated-image items or Markdown image syntax with a supported raster target. File bytes referenced by Markdown remain filesystem state; if they disappear or become unreadable, Beryl shows the unavailable fallback instead of treating the reference as a durable attachment.

## Diagnostic Child Live-Test Control

- Diagnostic child live-test controls are supervisor dynamic tools for testing an isolated child Beryl instance. They are not visible end-user controls in the ordinary workspace screen.
- Diagnostic child startup uses the supervisor's Beryl executable by default, and may use an explicit compatible Beryl executable path when the operator needs to live-test another build against an isolated copied home.
- Diagnostic child thread-listing reports bounded child workspace thread inventory state using the same inventory model that feeds thread selectors and thread-linking UI. It may report stale or refresh-pending inventory state, but it must not synchronously enumerate backend threads on the child UI thread.
- Diagnostic child new-thread control clears the child active-thread selection into the same pending-new-thread draft state as the `New Thread` button. It does not create a backend thread until a later accepted composer submission creates one through ordinary Beryl behavior.
- Diagnostic child turn submission injects bounded text into the child composer submission path. Accepted submissions become ordinary user input fragments, including first-message new-thread creation, active-turn steering, compaction-time queueing, composer history, transcript anchoring, draft clearing, and rejection behavior.
- Diagnostic child turn submission is unavailable when ordinary composer submission would be unavailable, including empty input, unresolved runtime target, backend-unavailable runtime target, disabled new-thread creation, incompatible edit mode, or another disabled submission state.
- Diagnostic child soft stop uses the same exact selected-thread active-turn interruption behavior as the status-line `Soft stop` action. Request acceptance is not terminal turn completion.
- Diagnostic child hard stop uses the same selected-turn interruption and exact backend-exposed hard-stop targets as the status-line `Hard stop` action, but the diagnostic tool request itself supplies the deliberate activation in place of the visible three-second hold affordance.
- Diagnostic child wait-for-state observes bounded child UI and turn-state predicates such as workspace readiness, selected thread identity, active-turn state, idle state, visible transcript count, and inventory availability. Timeout returns the latest bounded child state rather than blocking indefinitely.

## Surface Notices

- Surface notices are shown one at a time from a bounded queue. Dismissing the visible notice advances to the next queued notice when present.
- Turn-error notices may replace or outlive other localized notices through the shared notice queue, but Beryl must not stack multiple notice popups on the workspace at once or merge unrelated error details into one notice body.
- If the notice queue reaches its cap, Beryl may coalesce overflow into a summary notice rather than preserving every individual queued notice.
- If repeated backend events describe the same failed selected turn, Beryl reports that failed turn through at most one queued notice.

## Turn Completion Notifications

- Beryl supports an optional app-wide end-turn sound for completed user-visible parent conversation turns.
- The default end-turn sound setting is empty, and Beryl plays no end-turn sound while the setting is empty.
- When a configured end-turn sound exists, Beryl plays it only after a user-visible parent conversation turn reaches a terminal state while no Beryl window is focused.
- A Beryl window is focused when either the main workspace window or the settings window has OS focus.
- Terminal parent-turn states that may trigger the sound include successful completion, interruption, and failure.
- Beryl does not play the end-turn sound for title-generation maintenance turns, member-thread inventory refresh, lazy metadata resolution, context compaction, automatic lifecycle continuation, startup probes, settings changes, or other background/status-only work.
- V1 end-turn sound files are WAV files selected by full host filesystem path.
- If the configured WAV file is missing, unreadable, unsupported, or cannot be played at turn completion time, Beryl treats the playback failure as non-fatal, leaves turn state unchanged, and records the failure through normal diagnostics.

## Thread Selector

- The workspace screen includes a thread selector for switching the active Codex conversation thread without using the semantic graph.
- The thread selector is a separate widget from the semantic graph overlay and only shares reusable column selector behavior.
- Opening the thread selector closes the graph overlay and graph context menus so only one selector surface is interactive.
- The thread selector renders from the latest member-thread inventory snapshot and does not synchronously call `codex app-server`.
- If the active workspace has exactly one available member, including the implicit home member case, the selector opens directly to that member's thread list.
- If the active workspace has multiple available members, the selector first shows a member column, and selecting a member opens that member's thread column.
- Opening the selector preselects the currently active thread when it appears in the latest inventory snapshot, including the member path needed to show that thread row.
- Member rows show the member label and thread count.
- Thread rows show the thread display title and are sorted by `last updated` descending within their member.
- The currently active thread row is visibly highlighted.
- Single-clicking a member row opens that member's thread column.
- Single-clicking a thread row selects it without switching the active transcript.
- Double-clicking a thread row activates that exact Codex thread in the main transcript.
- Pressing `Enter` while a thread row is selected activates that exact Codex thread in the main transcript.
- Pressing `Escape` closes the selector without changing the active thread.
- Once thread activation is accepted, the selector closes and the transcript region shows the pending activation state for the target thread.
- If the selected thread is no longer available, cannot be reopened, has a recorded working directory that does not match its expected binding, or requires explicit rebinding, Beryl reports the standard thread activation or rebind notice and keeps the current active transcript selection.

## Graph Overlay

- The workspace screen includes a semantic-graph explorer overlay that is hidden by default.
- One hotkey toggles that overlay on and off.
- Opening the graph overlay closes the thread selector so only one selector surface is interactive.
- When visible, the graph overlay appears above the workspace content and occupies the upper half of the main window area not used by the checklist sidebar.
- The overlay renders as horizontally arranged explorer columns through the reusable column selector behavior.
- The first explorer column begins with the workspace's ordered root-level semantic nodes; if the graph has multiple roots, that first column lists multiple root-level nodes.
- Each later column shows the selected semantic node plus a bounded visible subtree beneath it.
- Each explorer column keeps its own vertical scroll position beneath a fixed column header instead of sharing one vertical scroll position for the whole overlay.
- Each column header is a compact single-line strip for the current column scope and does not show summaries or node counters inline.
- Ordinary graph mutations keep the explorer columns mounted. Affected rows or menu actions may show pending state, but the overlay does not switch to a full-scene loading view when graph content is already available.
- Graph columns preserve selection, expansion, and scroll by semantic identity across graph updates, pruning only state that refers to deleted or invalidated graph items.
- V1 columns default to showing two levels of hard semantic nodes, with fold and unfold controls that can hide attached soft-link and thread-ref rows as well as hard-child expansion within the column.
- Semantic-node rows are compact single-line rows. Node summaries are exposed through hover tooltips instead of inline summary text, except while a graph-node context menu is open.
- Node type is conveyed in the explorer through row background treatment rather than inline facet badges, and checklist-item rows show status with a compact inline marker ahead of the title.
- Soft links and thread refs attached to expanded nodes render as compact terminal rows beneath those nodes.
- Selecting a semantic node opens the next explorer column rooted at that node.
- Selecting a soft link opens the next column rooted at that link's target semantic node.
- Selecting a valid thread-ref row activates the referenced Codex thread in the main transcript instead of using that thread as the next graph root.
- Invalid thread-ref rows remain visible with a compact invalid-link indicator and do not open a transcript.
- Activating a valid thread-ref row uses the same direct exact thread activation path and pending transcript state as the thread selector.
- Right-clicking a semantic-node row opens a context menu for that node.
- The node context menu includes `Delete`, which immediately deletes the selected semantic node only when it has no hard children. `Delete` remains visible but disabled with a reason tooltip when the selected semantic node has hard children.
- The node context menu includes `Delete Recursively`, which deletes the selected semantic node and its hard descendants after held activation.
- `Delete Recursively` is activated only by holding its popup row for three seconds. While the pointer or keyboard activation is held, the row background fills from left to right; releasing early, leaving the row, closing the popup, focus loss, or stale graph-node target cancels the hold without deleting graph state.
- The node context menu includes `Link thread`, which creates a thread ref from the selected existing conversation thread to that semantic node without activating the transcript.
- If the workspace has no default runtime environment, `Link thread` is disabled and exposes the reason in a hover tooltip.
- If the active workspace has exactly one available member, including the implicit home member case, `Link thread` opens directly to that member's thread list.
- If the active workspace has multiple available members, `Link thread` first opens a member list and each member opens its own thread list.
- If a member has no linkable threads in the current inventory snapshot, its thread list shows a disabled `No threads` item.
- Thread-list rows in the `Link thread` menu show only the thread display title and are sorted by last-updated time descending.
- If explorer columns exceed the available width, the overlay remains horizontally scrollable so earlier selections stay reachable.

## Connection Failure Recovery

- If the foreground backend connection or managed backend process is lost, the GUI keeps the current Beryl workspace, semantic graph state, checklist selection state, runtime-environment state, workspace-member state, and active transcript selection intact.
- If a background backend connection used for title generation, inventory refresh, or lazy maintenance fails while the managed backend process remains available, Beryl reports or logs only that background operation's failure and keeps the active conversation usable.
- Backend launch, probe, or compatibility failure before a usable connection exists is reported as backend-unavailable state for that runtime target, not as application startup failure.
- On backend disconnect, the GUI presents a blocking recovery path rather than silently switching to another backend process.
- Recovery actions may include relaunching a managed backend for the same workspace runtime environment and resumed thread binding or closing the application instance.
- The GUI must not silently switch the user to a different backend process after a disconnect.

## Transcript Rendering and Appearance

- The transcript renders Markdown with typographic variation and block-level styling rather than plain unformatted text.
- The styled Markdown surface supports emphasis, strong emphasis, ATX headings, unordered lists, ordered lists, inline code spans, fenced code blocks, block quotes, links, local raster image embeds, explicit line breaks, paragraph spacing, and thematic breaks.
- The transcript Markdown model represents image references and math spans or math blocks as distinct semantic structures. Dedicated raster image rendering is supported for local image file references that pass Beryl's path and format checks; math typesetting remains out of scope for V1.
- Unsupported or non-Markdown inline conventions render literally as text unless they are valid Markdown constructs represented by the transcript Markdown model.
- Raw HTML embedded in Markdown is not rendered as HTML; it must render as literal source text or an unsupported-source fallback, never as executable or styled markup.
- Fenced code blocks render through the shared code panel widget, including Beryl-owned parser-backed syntax highlighting when their language label resolves to a registered parser.
- Fenced code blocks whose language label is `beryl-theme` are ordinary transcript content that can expose Beryl-owned `Preview` and `Install Theme` actions after validation. Installing asks the user to confirm the durable theme name before the theme is saved into Beryl.
- The Markdown language is supported by the Beryl-owned syntax highlighter; code blocks with unsupported, unknown, empty, partial, or invalid language labels render as plain text without changing source or copy behavior.
- Typography and colors used by the application UI and conversation transcript must be configurable by the user.
- Configurable appearance roles cover every Beryl-owned visible appearance value, including backgrounds, borders, single-primitive colors, text foregrounds, text backgrounds, font families, font sizes, and font weights for transcript content, Markdown blocks, code panels, UI chrome, graph/checklist surfaces, status values, warnings/errors/info, selections, focus states, disabled states, settings surfaces, popups, overlays, and media placeholders.
- Beryl-owned buttons use shared visual geometry: one outer-height rule, button-label typography sized to fit that height, padding derived from the label height, and one shared rounded-corner shape.
- Markdown emphasis and strong emphasis must be rendered through configurable style roles rather than being hard-wired to literal italic or bold font treatment.
- The styling system must allow emphasis and strong emphasis to vary by font family, weight, size, color, and related presentation attributes.
- Theme roles expose only the properties their GUI render sites can consume. Separator roles expose a single `color` property rather than border, foreground, text background, or font properties. Supported theme role properties can resolve from concrete values, static parent roles, runtime ambient parent styles, or built-in fallback values. Runtime ambient inheritance lets embedded styles such as inline code follow the background of final-answer text, user-input text, settings rows, or popups while retaining their own foreground or typography.
- Beryl may expose CAS theme tools that help the model author themes by reading bounded guidance about compact TOML syntax, role groups, static inheritance, ambient inheritance, transcript/code/settings styling, and troubleshooting. This guidance supplements the structural schema tool rather than replacing it.
- Beryl may expose a non-mutating CAS theme validation tool that checks a candidate compact TOML theme document through the same parser and resolver used by Preview, Install Theme, Update, and Save As paths. Validation rejects unsupported role-property combinations, can return bounded diagnostics, document summaries, and requested role-source explanations, but it does not preview, install, update, or persist a theme.

## Settings Window

- Application settings live in a dedicated top-level settings window rather than an in-place modal or panel.
- The settings window does not include the main workspace window's shared toolbar strip.
- The settings window should be created ahead of first use and hidden when inactive so opening settings feels immediate.
- The settings window uses broad left-side section navigation and one right-pane settings page at a time.
- Settings subpages open in the right pane with back and breadcrumb navigation. The left sidebar does not contain nested rows or a tree.
- V1 settings sections are `General`, `Appearance`, `Themes`, `Agent`, `Notifications`, and `Advanced`.
- Settings rows are schema-backed key/value rows with stable setting ids, modified indicators, reset actions, and context actions such as copying the setting id.
- The `General` section includes workflow preferences such as the context compaction timeout row. The timeout value is a whole number of seconds that controls how long Beryl waits for backend-reported selected-thread context compaction completion after the backend accepts the compaction request.
- The `Themes` section lists only durable installed themes. It does not list unsaved AI-generated theme candidates from Codex threads and does not provide Preview for installed themes.
- The `Themes` section supports installed-theme operations such as activate, rename, delete, and edit when those operations are valid. Switching between installed themes is direct activation.
- The active theme row exposes Save and Save As only when the active theme has staged changes. Save persists those changes to the active installed theme. Save As asks for a new durable theme name and saves the staged active-theme definition as a new installed theme.
- The active theme row's Edit action opens the right-pane theme editor subpage and uses the same right-facing chevron affordance as other step-in settings rows.
- The theme editor exposes only the properties supported by the selected role. Unsupported property entries in installed theme files are ignored when those themes are loaded and are omitted when the theme is later saved.
- AI-generated unsaved theme candidates stay in the originating Codex thread as `beryl-theme` code panels. The bridge from the thread to durable Beryl settings is the code panel's `Install Theme` action or a Beryl theme dynamic tool operation that explicitly installs a durable theme.
- The `Notifications` section includes an end-turn sound row that shows the currently selected full filesystem path, or an empty disabled state when no sound file is selected.
- The end-turn sound row includes a choose action that opens the Windows file picker for selecting a WAV file.
- The end-turn sound row includes a clear action that stages the setting back to the empty disabled state.
- The `Agent` section includes a multiline developer-instructions setting. Its row shows the subtext `Sent as developer instructions with every user message.` Blank or whitespace-only content is treated as disabled.
- Ordinary settings drafts do not live-preview unapplied changes. User-visible theme Preview controls are limited to unsaved `beryl-theme` transcript candidates and are controlled from the originating code panel. CAS theme preview tool calls may also create transient runtime previews, but they do not create settings-window candidates, transcript offers, installed themes, or durable settings.
- The settings window includes an Apply action that applies the current settings immediately without closing the window.
- Color-valued settings use a dedicated color input field that shows the canonical `#rrggbb` value, shows a preview swatch for the current valid color, and can open an in-window color picker from the preview swatch or a field hotkey.
- The settings window UI is provided through reusable `gpui` settings-window mechanics where practical, but Beryl owns the Beryl-specific section model, settings catalog, stable setting ids, staged draft behavior, and persistence.
- Beryl owns settings schemas, validation, staged draft behavior, apply behavior, and persistence. Installed themes persist as compact TOML theme documents in Beryl's theme repository under the configured Beryl home directory; operation preferences, notification preferences, and global developer-instructions preferences persist as app-wide GUI preferences in `preferences.toml` under the configured Beryl home directory, outside backend-owned Codex configuration.
- A legacy flat `theme.toml` file at the Beryl home root is ignored by the installed theme repository and is left untouched.
- The settings window consumes the active Beryl appearance theme through app-neutral style options exposed by the reusable settings-window crate, so settings panels, rows, popups, inputs, and action buttons use the same configured theme roles as the main window where those roles overlap.
- V1 settings do not provide a separate AI theme-candidate inbox.
- V1 settings do not expose backend-owned Codex configuration.
- Beryl may expose bounded CAS dynamic tools for reading and modifying Beryl-owned GUI settings. Readable settings are limited to Beryl-owned app-wide operation preferences, notification preferences, developer-instructions preference metadata, AI-control preference metadata, active theme identity, installed theme metadata, and theme schema or theme document data exposed through theme tools.
- CAS settings tools may update operation preferences, clear or explicitly replace notification settings, and replace or clear developer-instructions text. AI-control preferences that govern model authority are readable but not model-writable unless a later operator-confirmed operation is designed.
- CAS settings validation is non-mutating. Accepted settings update operations commit immediately through the same validation, active-update, persistence, and recovery paths as settings-window Apply; they do not create unapplied settings-window drafts.
- CAS settings read tools return literal values only for non-sensitive scalar settings. Notification sound reads show configured/disabled state and non-identifying file metadata, not the full local path. Developer-instructions reads show enabled state, character count, line count, and a stable content fingerprint, not literal instruction text.
- These tools do not expose or mutate backend-owned Codex configuration, authentication, skills, MCP state, session storage, or transcript history.

## Change Inspection

- V1 does not include file diff views, change review workflows, or other built-in agent edit inspection UI.
- Users are expected to inspect filesystem changes in their preferred editor or IDE outside this application.

## Performance requirements

UI responsiveness, including minimal input lag and minimal rendering or animation lag, is of paramount importance. RAM and CPU efficiency are also important. No handwaving is acceptable in those areas.
