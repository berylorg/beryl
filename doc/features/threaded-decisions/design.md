# Goals

Let users explore nontrivial itemized decisions in dedicated child conversation threads, resolve those explorations back into an exact parent conversation, and keep checklist-item progress, outcome, and provenance coherent across the handoff.

## Non-goals

- Replacing generic semantic-graph thread refs or limiting checklist items to one linked thread.
- Making child branch transcripts, handoff turns, or graph summaries replace authoritative design documents.
- Supporting more than one active decision-making child branch for the same checklist item. Historical closed decision branches may remain recorded.
- Emulating missing backend fork, rollback, turn-start, resume, archive, or unarchive primitives by copying backend conversation history into GUI-local state.
- Guaranteeing that model-authored decision outcomes are correct without user review, later correction, or source-document updates.
- Creating GUI-local synthetic, pinned, or hidden decision-context transcript content for new decision branches. New decision context belongs in the visible backend bootstrap turn.

# Decisions

## Product Workflow

A threaded decision is a workflow binding between one decision checklist-item semantic node, one parent conversation thread, and at most one active decision-making child thread.

Checklist items remain ordinary semantic graph checklist-item nodes, but each item has a kind of `generic` or `decision`. Generic checklist items keep the existing flexible behavior. Decision checklist items are governed by the threaded-decision workflow once they have active or resolved decision state.

Checklist items may keep multiple generic thread refs for navigation and history. Threaded decisions add a separate decision-branch binding so Beryl can distinguish the one child thread currently responsible for resolving the checklist item from other relevant linked threads.

Users may create decision branches from selected checklist items through direct UI actions or through Beryl-owned dynamic tools after explicit user instruction. Creating a decision branch starts a backend child thread, writes a visible bootstrap turn containing branch provenance and decision context, then attaches a generic thread ref to the checklist item for navigation and creates the stronger decision-branch binding for workflow authority after the child thread is proven durable.

Users may also start a decision directly from a topic-capable graph row. That action plans a decision checklist-item child under the topic, starts and bootstraps the decision branch, then commits the checklist item, thread ref, and decision-branch binding after bootstrap success so a failed backend branch start does not leave Beryl-owned graph state behind.

If a decision-branch request is made while the parent thread is still running the turn that identified the checklist items, Beryl records branch-creation jobs and executes them only after that parent turn reaches a backend terminal state and the parent thread is idle. The parent context source for such jobs is the completed parent turn. A branch created from a transcript context menu may use the clicked parent turn as the parent context source.

The child thread is used to explore exactly that decision. The child can use ordinary graph tools and thread refs, but resolution is performed through the threaded-decision workflow so the model does not need to guess which checklist item should be updated.

Resolving a decision branch creates a real user-visible turn in the parent thread containing the handoff, updates the checklist item's decision state, switches the UI to the parent thread, and then closes the child decision branch. The handoff turn is the durable conversation event that records that the branch result re-entered the parent conversation and becomes part of future parent-thread model context.

## State Model

Threaded-decision state is GUI-local workspace state that references semantic graph node ids, backend thread ids, backend turn ids, and backend archive state. It is not backend conversation history and does not copy transcript contents.

Each checklist item may have at most one active decision-branch binding. The binding records the checklist-item node id, parent thread id, child branch thread id, bootstrap turn id when known, parent context source turn id when known, current workflow status, created timestamp, and provenance for the action that created it.

Closed decision records preserve the child branch thread id, parent thread id, handoff parent turn id when known, original resolution outcome, resolution summary, resolved timestamp, archive status, and provenance for the action that resolved it. Historical closed branches may remain attached to the checklist item as exact durable decision history, while only one branch may be active.

Checklist item progress remains separate from decision outcome. Checklist-item status answers whether exploration work is pending, active, or complete. Decision outcome answers what was decided. A rejected option can still mark the checklist item `done`.

The active resolution tool accepts only `accepted` or `rejected`. `Superseded` is a system-applied historical state used when a new decision branch replaces a previously closed decision branch for the same checklist item. Unresolved active branches have no final outcome.

Creating a decision branch and completing its Beryl-authored bootstrap turn does not by itself mark the checklist item `in_progress`. Once the child branch has at least one real user-authored exploratory turn after the bootstrap turn, Beryl marks the checklist item `in_progress`. Closing the decision branch marks the checklist item `done` with outcome `accepted` or `rejected`.

## Branch Creation

Decision-branch creation requires an exact parent thread, an exact checklist-item node id, a backend-supported new-thread path, and a current workspace member/runtime binding that can still open the parent thread. Beryl must not silently rebind the parent, pick another checklist item, or create a local transcript copy when backend thread creation is unavailable.

When a checklist item already has an active decision branch, creating another active branch is unavailable until the current branch is resolved or canceled. Generic thread refs remain unrestricted by this rule.

When a decision checklist item has only closed decision branches, creating a new decision branch is allowed. Beryl marks the previous closed decision branch records as superseded for presentation while preserving each branch's original `accepted` or `rejected` outcome for provenance.

Decision branch creation creates a new backend conversation thread rather than a transcript fork that inherits the parent turns. The source parent remains unchanged. The child branch remains backend-owned conversation history.

Beryl immediately starts a visible bootstrap user turn in the child thread. The bootstrap turn includes a parent-thread link using the Beryl-owned `beryl_threadid://<parent_thread_id>` scheme, the decision checklist-item title and summary, the graph ancestor path and summaries under which the item lives, the parent context source turn when known, and the resolution workflow. This bootstrap turn is the durable decision context for future model turns and ordinary transcript reloads.

Beryl records the parent-child relationship in threaded-decision workspace state only after bootstrap success and durable child-thread proof. The child thread does not copy parent transcript turns into GUI-local state, and Beryl must not use hidden developer/context injection or pinned synthetic transcript blocks as the source of decision context for new decision branches.

Beryl also registers threaded-decision dynamic tools for child threads when the app-server dynamic-tool contract permits registration for that thread.

## Resolution And Handoff

Decision resolution is an explicit user or model-requested action from a bound child decision thread through a dedicated threaded-decision resolution tool or equivalent direct GUI action. The action supplies the resolution outcome, a concise resolution summary, and the handoff message to send to the parent. Beryl supplies the checklist-item id, parent thread id, child thread id, and provenance from the binding. No additional UI confirmation is required after the tool call or GUI action is accepted.

Beryl validates that the active child thread has exactly one active decision-branch binding before accepting a resolution request. If no binding exists, or if multiple inconsistent records are found, resolution is unavailable and the user must repair or choose the target explicitly through UI.

Resolution creates a real parent-thread turn with a normalized handoff preface that identifies the child branch title or checklist item, followed by the model- or user-authored handoff message. The parent thread must be the exact bound parent and must be idle before Beryl starts the turn. The parent model is expected to run because the handoff is an ordinary durable user turn.

The normalized handoff preface must make the automatic origin clear to a human operator returning later. It includes the decision branch title or child thread id, the checklist item title, and the `accepted` or `rejected` resolution before the authored handoff message.

Beryl updates the checklist item only after the parent handoff turn has been accepted or can be identified. The update marks exploration complete, records the decision outcome and summary, links the parent handoff turn as the resolution source, and preserves the child thread as exploration provenance.

After the parent handoff turn and checklist update succeed, Beryl activates the exact parent thread and scrolls or anchors to the handoff turn when possible. The resolved child decision branch is no longer the active UI destination.

If the parent handoff succeeds but the graph or decision-state update fails, Beryl records a recoverable partial-resolution state and retries or exposes a retry action. It must not create a duplicate parent handoff turn as an automatic retry.

If the parent handoff cannot be started, Beryl leaves the child branch active or in a pending-resolution state and does not mark the checklist item resolved.

## Child Cleanup

After the parent handoff and checklist update succeed, Beryl closes the child decision branch.

Beryl uses the exact `codex app-server` archive primitive for child branch closure when the target backend exposes it. Product copy calls the result a closed decision branch. Beryl must not use `thread/unsubscribe` as a substitute for closing a persisted decision branch because unsubscribe only unloads subscription state.

Archive failure does not roll back the handoff or checklist update. Beryl records archive failure on the closed decision record and exposes a retry path.

Archive retry must not assume backend archive is idempotent. If a retry receives an archive error but exact backend evidence shows the thread is already archived, Beryl may treat the child branch as closed and clear the retry warning.

Opening closed decision branches uses app-server exact-id archived-thread reads when the backend supports them. For the observed `codex-cli 0.128.0` contract, `thread/read includeTurns=true` and `thread/turns/list` can read archived threads without unarchiving, so Beryl opens a closed decision branch as a read-only transcript with composer disabled.

If Beryl uses `thread/resume` to inspect or load an archived branch, it must still force closed-branch read-only UI state. The observed backend can return an idle loaded thread from `thread/resume` while the thread remains archived, so returned idle status is not permission to make the closed branch writable or visible through normal thread selection. Beryl must not call `thread/unarchive` for ordinary thread-ref activation of a closed decision branch; explicit unarchive belongs to a later repair or reopening design.

## Provenance And Authority

The parent handoff turn is the preferred navigation target for "where did this resolution enter the main conversation?" The child branch remains the navigation target for the full exploration history. The checklist item stores only concise decision metadata and refs needed to navigate and recover workflow state.

Model-supplied resolution text and outcome are not trusted as identity or provenance. Beryl injects the bound checklist item, parent thread, child thread, source turn, tool-call identity, and timestamps through its own workflow state and graph write path.

Graph summaries and decision summaries are navigational aids. Source documents and feature design documents remain authoritative for durable project decisions when they have been updated.

Resolved decision checklist items are protected from generic AI graph-upkeep mutations to checklist status, item kind, decision metadata, and decision-owned summaries. Changes to resolved decision state go through explicit threaded-decision actions such as resolving, creating a superseding branch, retrying archive, or human repair.

## Dynamic Tools

Beryl may expose operation-specific threaded-decision tools for creating decision branches and resolving active decision branches. These tools must be narrower than a generic graph patch API.

A branch-creation tool accepts explicit checklist-item ids or uses checklist items created earlier in the same parent workflow when Beryl can resolve them exactly. It returns queued, created, or failed branch records per item.

A resolution tool is scoped to the active child thread. It accepts outcome `accepted` or `rejected`, summary, and handoff text. It does not accept checklist-item or parent-thread identity from the model as authority when a binding already exists.

Tool availability follows the app-server dynamic-tool contract. When dynamic tools cannot be registered on an existing resumed thread, Beryl may provide equivalent direct GUI actions, but it must not present unavailable model tools as usable.

## UI

Threaded-decision UI lives in the graph overlay. Beryl does not use a separate checklist sidebar for threaded decisions.

Checklist-item graph rows expose `Start Decision Branch` from the graph node context menu. Starting a decision branch from a generic checklist item converts it to a decision checklist item.

Topic-capable graph rows expose `Start Decision`. The action prompts for or otherwise supplies a concise decision item title, starts and bootstraps a decision child thread for that item, commits the decision checklist-item child under that topic together with the thread ref and decision binding after bootstrap success, and activates the child thread.

Checklist-item graph rows with an active decision branch show a compact active-branch indicator. Activating that indicator opens the child decision thread.

Resolved checklist-item graph rows may show the decision outcome separately from the checklist status. Activating the resolution indicator opens the parent handoff turn when available.

Decision checklist-item graph rows with only closed decision branches expose a command to start a new decision branch. Starting that branch supersedes the earlier closed branch records for presentation.

Generic linked threads remain visible as ordinary graph thread-ref child rows. A decision branch thread ref is still a thread ref and uses the existing thread-ref activation path, but Beryl may annotate it as active, closed, or superseded decision history when threaded-decision binding metadata is available. Generic thread refs do not determine which child thread is responsible for decision resolution.

Decision-branch commands are disabled with specific reasons when backend thread creation is unavailable, the parent thread is active and no queued branch job can be recorded, the checklist item already has an active branch, the bound parent or child thread is unopenable, or workspace/runtime validation fails.

Resolution commands are disabled with specific reasons when the child thread has no active binding, the parent thread cannot be opened exactly, the parent thread is active, the handoff text is empty, the outcome is not `accepted` or `rejected`, or the resolution state is already terminal.

Partial-resolution or archive-failure states show a compact warning on the affected checklist-item graph row and expose retry actions from the graph node context menu.

Decision child threads show a compact clickable parent-thread breadcrumb in the thread strip when horizontal space allows. The breadcrumb uses the parent thread display title and activates the exact bound parent thread. The decision checklist item remains visible through the child thread title, tooltip, graph row, and visible bootstrap turn rather than as the primary thread-strip breadcrumb.

## Failure And Recovery

Threaded-decision operations cross backend-owned conversation history and GUI-owned workspace state, so they are not treated as one atomic storage transaction.

Beryl records pending and partial workflow states with stable operation identities after durable state exists. Branch creation failures before bootstrap success leave no Beryl-owned graph thread ref, decision binding, registered branch thread, or retry row; the user can retry by invoking the start action again. After an uncertain parent turn-start result, Beryl must not blindly replay the handoff without either identifying the original handoff turn or asking the user to confirm a duplicate.

Deleting a semantic graph node removes or invalidates threaded-decision records that reference that node without deleting backend conversation history. Archiving a child branch through the threaded-decision workflow closes the decision branch without deleting checklist history. Externally archiving or unarchiving a referenced backend thread updates threaded-decision navigation and closure state when Beryl observes exact backend metadata or notifications.

Backend-unavailable states disable backend-required threaded-decision actions while preserving local checklist and decision metadata already stored in the workspace.
