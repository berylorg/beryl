# Next Big Rework

This is a working capture of the decisions and unresolved design questions from the June 2026 architecture discussion.

It is not yet authoritative design. Before implementation, the target state must be promoted into the normal authority chain: feature docs for user-visible behavior, system docs for cross-package architecture, package docs for local boundaries, and a tracked `doc/rework/<name>/REWORK.md` plus `doc/plan.md` entry.

# Checkpoint - 2026-06-29

This checkpoint captures the current conversation state before formal planning.

The 2026-07-01 checkpoint supersedes this checkpoint's open-question list where they conflict.

The 2026-07-02 CAS-optional startup and Syndic thread ownership checkpoint supersedes this checkpoint's startup-runtime and "Beryl thread" wording where they conflict.

## Resolutions

- This is an architectural rework, not an incremental compatibility migration.
- Workspaces should be removed as product UI and architectural authority.
- Semantic graph functionality should be deleted fully, including graph tools, graph upkeep, graph persistence, graph diagnostics, graph-derived workflows, checklist graph bindings, graph-started threads, and graph-specific tests/docs/settings/theme roles.
- Obsolete workspace-era and graph-era live code/docs should be discarded from live authority and kept only as reference material under the rework archive when useful.
- A Beryl home is the physical directory containing all Beryl-owned state.
- Two different Beryl homes do not share one Syndic store.
- One OS process owns a Beryl home at a time; multiple Beryl windows may exist inside that process.
- A busy Beryl home shows a busy-home surface with one exit button, self-exits after about 5 seconds, and exits with code `1`.
- Beryl startup selects only the runtime environment: host Windows or a WSL distro.
- Startup does not select an execution root.
- Execution roots are thread-bound and stored initially as a `(runtime, directory)` tuple on each thread.
- Existing thread activation automatically switches to the runtime and execution root bound to that thread.
- Execution roots define CAS working directory context, including `AGENTS.md` and skill discovery.
- New threads are user-defined durable objects, not turn-derived objects.
- Clicking `New Thread` immediately creates or reuses a durable empty thread.
- If an unlocked empty thread already exists, Beryl should reuse it instead of creating another empty catalog row.
- Threads have stable ids and mutable head turn ids.
- The visible transcript is the path from the thread head back to the root turn.
- Turn parent links are immutable once committed.
- Different Beryl threads may share the same historical head turn and then diverge without conflict.
- Same-thread mutation is guarded by the thread head pointer, not by the historical parent turn.
- Appending should compare-and-update the thread head.
- User input confirms a new turn because user input is the beginning of a turn.
- Beryl creates the durable turn and moves the thread head as soon as it receives any data for that turn.
- Incomplete turns from stop, disconnect, crash recovery, or similar interruption remain durable Syndic turns.
- Replacement edit moves the existing thread head to the replacement path and must not mutate shared historical turns.
- Syndic is durable conversation DAG truth.
- CAS is used only for live turns.
- Browsing existing Beryl threads reads Syndic and does not use CAS.
- Every live Beryl thread must have its own exclusive CAS thread.
- On append, Beryl either reuses the exclusive CAS thread already assigned to that Beryl thread or creates/forks a new exclusive CAS thread for it.
- Creating or forking an exclusive CAS assignment must inherit the prior Syndic turns needed for the append attempt.
- CAS thread lists, metadata, and names must not become catalog or title authority.
- Home-level durable metadata should use one physical Fjall database layout for lower overhead where practical.
- Logical storage ownership still remains separate: Syndic owns conversation DAG/projection records, while Beryl owns GUI metadata in separate Fjall keyspaces behind typed Beryl APIs.
- Thread titles are Beryl-owned, not CAS-owned.
- Branch sibling visualization is deferred.

## Open Questions

The open-question list from this checkpoint is historical.

Use the 2026-07-01 checkpoint and the current `# Non-Trivial Decisions Still Open` section for current open questions.

# Checkpoint - 2026-07-01

This checkpoint supersedes the open-question state from 2026-06-29 where the two conflict.

## New Resolutions

- A single click on the `New Thread` button creates or reuses a thread in the execution root last used by the current Beryl window.
- Click-hold on `New Thread` opens a popup that asks which execution root to use for the new thread.
- The click-hold popup selects both execution runtime and execution directory.
- Empty-thread reuse is scoped by `(runtime, directory)`.
- Empty threads appear in the thread catalog.
- If a preexisting thread's bound runtime or bound directory is unavailable, Beryl still allows reading the thread from Syndic history.
- If a preexisting thread's bound runtime or bound directory is unavailable, Beryl disables or dims mutating widgets such as the composer and mutating context-menu items.
- Beryl allows changing runtime and directory for any thread, not only threads with currently invalid runtime/directory bindings.
- Restart should restore all prior windows.
- Restored windows should keep their positions, sizes, and Windows virtual desktops.
- Multiple simultaneous active turns under the same runtime/root should rely on CAS support rather than adding a Beryl-side limitation.
- Durable database exclusive locks should not be used for thread-head coordination inside Fjall.
- Beryl should prefer revisioned compare-and-swap style metadata updates for thread-head mutation.
- Fjall database integrity is a hard requirement.
- Beryl may lose in-flight data on panic, power loss, or Windows/kernel failure, but the Fjall database must remain reopenable, internally consistent, and fully usable.
- Accepted durable writes should use the strongest practical Fjall persistence mode, currently represented in existing Syndic storage by committing the batch and calling `persist(PersistMode::SyncAll)`.
- Beryl-owned code must not mutate Fjall database files directly or bypass Fjall's journal/recovery machinery.
- Storage implementation must include crash-recovery and restart verification for process panic/abort and simulated interrupted writes.
- CAS is the runtime process, meaning the environment where the `codex` binary runs.
- CAS should not be modeled as separately rejecting a runtime; runtime availability is the availability of the host or WSL environment needed to run `codex`.
- A working directory can still be unavailable or unusable because the path is missing, inaccessible, not a directory, lacks permission, or its WSL distro is unavailable.
- If an assigned CAS thread is lost after backend restart, Beryl should start a new CAS thread and provide the needed Syndic history.
- Packing Syndic history into a new CAS thread input is an implementation design task, not an operator-level product decision for now.
- GUI "edit old input" keeps the old user-facing behavior: the current thread tail disappears and the new user input is sent.
- At the graph level, GUI "edit old input" creates a new turn with the new user input, creating a branch in the DAG rather than mutating an existing Syndic turn.
- The old tail is left alone in the DAG even if no visible thread currently reaches it; reachability and garbage collection are deferred.

## Parked Open Questions

- What exactly counts as an abandoned empty thread, and should Beryl ever auto-clean visible empty catalog threads?
- How does Beryl represent a window's last-used execution root before any thread has been created in that window?
- What is the exact popup content and behavior for click-hold `New Thread` root selection?
- How does Beryl detect runtime or directory availability without making ordinary history browsing fragile?
- What visible affordance changes a thread's runtime/directory binding, and how is that action confirmed?
- What Beryl-side active-turn bookkeeping is needed for multiple simultaneous CAS turns: per-turn ids, stop routing, recovery, and status aggregation?
- What exact storage primitive backs revisioned compare-and-swap: Fjall optimistic transaction, single-writer transaction, short serialized home-store commit, or another typed API?
- How does Beryl create, fork, refresh, and prove the exclusive CAS thread assignment for a thread before append, including history packing when a new CAS thread is needed?
- What is Beryl's release-blocking Fjall crash-consistency verification plan?
- What does thread deletion remove now, and what garbage collection is deferred?
- What is the immediate thread selector/catalog grouping after workspaces and graph are gone?
- Where do Beryl image assets live after workspace directories are removed?
- For WSL maintenance turns, does Beryl use a neutral distro directory, mapped host directory, or host-only maintenance path?
- What are the exact file-lock mechanics for canonical paths, symlinks, junctions, UNC paths, stale locks, recovery, and normal release?

# Checkpoint - 2026-07-02

This checkpoint corrects CAS materialization and GUI edit wording.

## New Resolutions

- CAS does not directly support starting a thread from supplied history as backend-owned prior turns.
- The recovery path for a lost assigned CAS thread is to create a new CAS thread and send a new turn.
- Syndic history needed for that new live turn is packed into hidden model context, not into visible user-authored transcript text.
- The current Beryl backend surface supports `thread/start`, `turn/start`, `thread/fork`, and `thread/rollback`.
- `turn/start` accepts user input and optional developer-instructions collaboration context.
- `turn/start` returns a `TurnStartResponse` containing `TurnInfo.id`.
- Live stream events carry CAS `thread_id` and `turn_id` identities for turn and item events.
- CAS assignment proof can therefore use the CAS thread id Beryl targeted plus the CAS turn id returned by `turn/start` and confirmed by stream events, together with the Beryl/Syndic selected-path revision or digest recorded in metadata.
- GUI edit behavior remains the old user-facing behavior: the current thread tail disappears and the new user input is sent.
- Graph-level behavior for that GUI edit is branch creation: Beryl creates a new Syndic turn with the edited input and moves the selected thread head to that new branch path.
- The old tail remains in the DAG and is not visible on the selected thread path unless another thread or future diagnostic/snapshot surface reaches it.
- A replacement-branch turn that is stopped, disconnected, crashed, or otherwise non-terminal follows ordinary incomplete-turn handling.

## Parked Open Questions

- What exact hidden context channel should carry packed Syndic history for a fresh CAS thread: developer-instructions collaboration context or another CAS-supported hidden input field?
- What is the exact context-pack format, budget, truncation behavior, and provenance marker for Syndic history packed into a fresh CAS turn?
- How should target docs reconcile the current CAS-live design rule that forbids materialization through user prompt or developer instructions with the new target that requires hidden context packing?

# Checkpoint - 2026-07-02 CAS-Optional Startup And Syndic Thread Ownership

This checkpoint supersedes older wording that said startup selects a runtime environment and older wording that called the durable thread data model "Beryl threads."

## New Resolutions

- Beryl should be able to start the main shell without touching CAS.
- Startup must not require choosing a runtime environment.
- Startup must not require CAS to be installed, launchable, authenticated, or otherwise available.
- Beryl startup opens the Beryl home, reads durable window state, and restores previously open windows from Beryl/Syndic-owned state first.
- Each restored window reopens its last selected Syndic thread when that state exists.
- For each unique runtime environment required by restored open windows, Beryl should attempt to start or connect to CAS after the shell can exist.
- If CAS startup fails for a required runtime, Beryl raises an error notification and keeps the shell usable for Syndic history browsing.
- CAS absence or CAS startup failure disables graph-altering functionality and AI conversation starts for the affected runtime or thread.
- Syndic threads and turns remain explorable even when CAS is unavailable.
- Mutating widgets whose actions require CAS should be disabled or dimmed when the relevant CAS runtime is unavailable.
- The durable thread and turn data model should be called Syndic threads and Syndic turns.
- Syndic owns stable thread ids, mutable thread heads, turn parentage, and the thread/turn data model.
- Beryl relies on Syndic for durable thread and turn state.
- "Beryl thread" should be used only when specifically describing the GUI representation of a Syndic thread, such as a selected row, view model, or shell surface.
- Beryl-owned metadata may still attach GUI/application state to Syndic thread ids, but that should not be described as Beryl owning the thread data model.

## Parked Open Questions

The 2026-07-08 CAS-unavailable product-behavior checkpoint resolves these questions.

- Which non-CAS metadata operations remain enabled when CAS is unavailable, such as title edits, pin/archive changes, root rebinding, and empty-thread creation?
- Does creating or reusing an empty Syndic thread require CAS availability, or is it allowed as a Syndic metadata operation with no turn graph mutation?
- When multiple restored windows require the same runtime, what exact notification and retry behavior should represent one failed CAS startup?

# Checkpoint - 2026-07-08 CAS-Unavailable Product Behavior

This checkpoint supersedes the July 2 parked questions about metadata mutations and notification scope when CAS is unavailable.

## New Resolutions

- When CAS is unavailable, Beryl permits only read-only actions that have enough local or Syndic-backed information to operate.
- No mutating operations are allowed without CAS.
- Title edits, pin changes, archive changes, runtime or root rebinding, and empty Syndic thread creation are all disabled when CAS is unavailable.
- CAS startup failures are surfaced per window, because each window should act independently where GUI behavior is concerned.
- A CAS startup failure is shown through an error notification in the affected window.
- The CAS failure notification includes a retry button.
- CAS and the bound working directory do not participate in history browsing.
- History browsing is entirely loaded from Syndic.
- CAS or working-directory unavailability must not be presented as transcript/history load failure.

# Checkpoint - 2026-07-10 Product Decisions And Decision Ownership

This checkpoint supersedes the July 8 blanket prohibition on mutation while CAS is unavailable, supersedes older "restore all prior windows" wording with explicit session rules, and assigns explicit owners to the remaining design decisions.

## New Resolutions

- Checklists are removed entirely with semantic graph functionality. Beryl does not retain a checklist model outside the graph.
- The current threaded-decision workflow is a checklist-bound workflow that creates a dedicated child thread for one decision item, explores there, sends a resolution handoff turn back to the exact parent thread, updates checklist state, and closes the child.
- That checklist-bound threaded-decision workflow cannot survive as-is because checklists are removed.
- Branch, explore, and merge remains a first-class Beryl product workflow. Its graph-independent replacement behavior and GUI require new operator-owned product design.
- Semantic search remains a desired Beryl feature, but its implementation is deferred until after this architectural rework is complete.
- The rework must remove or archive graph-dependent semantic-search implementation rather than preserve it through compatibility paths.
- Semantic search does not keep an authoritative feature doc while its implementation is deferred.
- Root `doc/design.md` gains a `# TODO` section that acts as a local issue/backlog workaround. It may stash future intent and already agreed provisional decisions for semantic search without making them current target-state authority.
- When semantic-search work is eventually scheduled, its stashed `# TODO` material must be reviewed and promoted into a new authoritative feature design before implementation.
- Beryl will not modify Codex App Server as part of this rework.
- Exact Syndic-history packing into a fresh CAS thread is AI-delegated system design constrained to the targeted existing CAS protocol.
- Generated schema for the currently installed `codex-cli 0.144.1` exposes `turn/start.additionalContext` as application- or untrusted-context entries separate from ordinary user input and developer instructions.
- The rework's targeted CAS contract must expose `turn/start.additionalContext`; Beryl uses that existing protocol surface for discussion context and fresh-history context rather than modifying CAS.
- Exact target-version selection and field-semantics verification remain delegated protocol investigation.
- Beryl-owned metadata mutations such as title, pin, archive, and empty-thread creation do not require the thread's currently bound CAS to be available.
- Rebinding away from an unavailable runtime is allowed when the destination runtime and its CAS are available.
- Conversation execution and Syndic turn-DAG mutations still require the relevant live CAS capability.
- Turn garbage collection is deferred. The immediate thread-delete operation removes the named thread reference and Beryl metadata without deleting durable turns.
- A future explicit `Collect Garbage` operation may reclaim unreachable turns and related resources after a separate design.
- One Syndic thread may have at most one active turn.
- Two main windows may not have the same Syndic thread open at the same time.
- Closing a main window with an active turn interrupts that turn.
- The immediate thread selector is recency-first, with runtime and execution-root filtering rather than a mandatory runtime/root hierarchy.
- A single click on `New Thread` uses the current window's last remembered runtime and execution root.
- If a runtime is remembered but no execution root is remembered, single-click `New Thread` uses the user's home directory in that runtime.
- If no runtime is remembered, single-click `New Thread` fails with visible feedback that tells the user to hold the button to add a runtime.
- Holding `New Thread` opens the more capable runtime/root selection flow. That flow must support adding a runtime and adding or selecting a root within a runtime.
- Only main windows that have threads participate in session restore. Auxiliary settings windows and transient windows do not.
- Closing one main window normally removes it from the next restored session.
- A dedicated toolbar `Exit` action exits Beryl while preserving all currently open main windows for the next session.
- Beryl also preserves open main windows for restoration after panic, crash, OS reboot, or other external process termination.
- Virtual-desktop restoration is best effort. A window whose prior virtual desktop still exists returns there; a window whose prior virtual desktop no longer exists is placed deterministically on the first virtual desktop, not the current virtual desktop.

## Decision Ownership Rule

- All unresolved GUI composition, interaction, visual state, discoverability, theme-role hierarchy, and theme-editor behavior is operator-owned.
- Product behavior with broad user-visible or data-lifecycle effects is operator-owned even when the implementation is technically simple.
- Bounded storage, protocol, concurrency-bookkeeping, locking, availability-probing, and test-harness mechanics are AI-delegated after operator-owned constraints are fixed.
- Delegated decisions must remain listed explicitly until resolved; they are not omitted merely because AI may resolve them without further operator input.
- If delegated investigation shows that a chosen product contract cannot technically work with the targeted CAS or platform boundary, implementation stops and returns the issue to the operator rather than inventing a workaround.

# Checkpoint - 2026-07-10 Branch Discussion, Deferred TODO, And Fjall Failure

This checkpoint defines the graph-independent branch-discussion workflow, confirms the temporary root TODO convention for deferred features, and resolves the product behavior for persistent Fjall failure.

## New Resolutions

- A user can select text in a rendered assistant reply and invoke `Discuss in new branch` instead of `Quote`.
- The action creates a new Syndic discussion thread whose path branches from the Syndic turn containing the selected reply text. The new path inherits earlier conversation turns through ordinary immutable Syndic parentage.
- Branch creation immediately creates a durable context-only Syndic turn under the selected source turn as the discussion's committed tail and creates the new discussion thread's separate empty current draft.
- The context-only Syndic turn does not start CAS or run the model because no user input exists yet.
- The user's first ordinary submission freezes the discussion draft as the first user-authored child of that durable context turn and supplies the selected discussion text through CAS `turn/start.additionalContext`.
- The exact selected text and source turn/item/range provenance are durable on the first discussion draft rather than synthetic user-authored transcript text. The new discussion thread separately owns the stable parent Syndic thread id used for eventual handoff.
- Beryl supplies the selected discussion text to the discussion CAS thread through the targeted existing `turn/start.additionalContext` protocol field. Beryl does not modify CAS and does not smuggle the context through developer instructions or ordinary user input.
- Selected transcript text must not gain application-instruction authority merely because Beryl forwards it as context. Exact CAS context-kind and source-key encoding are AI-delegated protocol/security design.
- The discussion GUI displays the selected context as a readonly convenience block so the user can see what is being discussed.
- The readonly context block is not ordinary transcript narrative and does not fabricate a Syndic user or assistant message.
- After branch creation, the user continues with ordinary conversation in the discussion thread.
- Beryl exposes a CAS dynamic tool scoped to the bound discussion thread that archives the discussion and hands a resolution back to its exact parent thread.
- There is no GUI command button that performs archive, resolution, and handoff. The user initiates that workflow by talking to the AI, which invokes the scoped dynamic tool.
- The dynamic tool does not accept model-supplied parent or discussion identity as authority when Beryl already owns an exact binding.
- Discussion archive state is Beryl-owned catalog metadata on the Syndic discussion thread. CAS archive or thread-list metadata is not product authority.
- Because the dynamic tool runs inside the discussion's active CAS turn, successful tool admission first persists the resolution intent and returns a bounded result; final child archival and parent handoff sequencing occur through host-owned lifecycle orchestration after the resolving child turn reaches the required state.
- A resolution handoff becomes a new real turn on the parent Syndic thread and runs through the parent's normal CAS-backed turn path.
- If the parent thread has an active turn, Beryl persists the resolution handoff as a Fjall-backed pending job and appends it only after the active turn reaches a terminal state and the parent becomes eligible.
- Resolution-tool admission atomically checks whether the discussion already has accepted input queued for a later turn.
- If queued turns exist, Beryl does not accept resolution intent, does not disable the composer, and does not change archive state. The tool returns a structured retryable deferred result telling the AI that queued turns must run first and that it may retry resolution later unless subsequent user input or steering cancels that intent.
- Beryl does not automatically retry the deferred tool call. The AI decides whether to call it again in a later turn after considering the intervening user input and steering.
- As soon as resolution intent is durably accepted, the discussion enters a resolution-pending state and its composer is disabled. No new user input may be accepted while handoff is pending.
- The discussion remains unarchived while handoff is pending and is archived only after the parent handoff turn succeeds.
- The pending handoff survives Beryl restart and must not be lost or duplicated. Job identity, compare-and-update admission, retry, recovery, and exact CAS/Syndic correlation are AI-delegated system design.
- Root `doc/design.md` `# TODO` is the temporary project-local substitute for an issue tracker. It may retain non-authoritative future intent and agreed provisional decisions without requiring an unimplemented feature to keep a feature design doc.
- Persistent Fjall failure must not close, remove, reposition, or replace main windows.
- Each existing main window keeps its current thread identity, geometry, virtual-desktop placement, and last coherent visible presentation in memory.
- Beryl disables history loading or navigation, composition and submission, settings, and every other operation that requires Fjall-backed Beryl state while the failure persists.
- Beryl must not fall back to CAS history or another authority merely to keep Fjall-dependent actions operating.
- Fjall is the sole durable owner of window/session restore data. Beryl does not maintain a redundant restore snapshot outside Fjall.
- If Fjall is already corrupted or unreadable at startup, Beryl cannot promise window/session restoration; that condition is outside the restore guarantee and is why crash consistency and Fjall health are hard requirements.
- Exact failure presentation, explanation, retry, and recovery controls remain operator-owned GUI design.

## Operator-Owned Details Still Open

- After resolution, whether the discussion window remains on the archived readonly discussion, switches to the parent when allowed, or follows another navigation rule, including when the parent is already open in another window.
- The placement, composition, selection behavior, truncation/expansion behavior, and visual treatment of the readonly discussion-context block.
- The visible pending, failed, retryable, and completed states for discussion resolution and parent handoff.
- The exact persistent-Fjall-failure surface and recovery interactions while the existing window shells remain intact.

## AI-Delegated Details Still Open

- Exact source-provenance records for selected discussion text and reconstruction of the readonly context block.
- Exact context-only Syndic turn representation and the atomic creation of the new thread, context turn, binding, committed-tail state, and separate empty draft without starting CAS.
- Exact CAS `additionalContext` source key, context kind, budget, escaping, and replay behavior under the targeted protocol.
- CAS fork/reuse versus fresh materialization mechanics for the discussion thread while Syndic parentage remains authoritative.
- Durable pending-handoff job schema, idempotency keys, compare-and-update boundaries, restart recovery, retry ordering, and duplicate prevention.
- Exact resolving-turn completion, Beryl-owned archive transition, CAS projection cleanup, parent eligibility, and handoff-start ordering.
- Exact atomic ordering between queued-input admission and resolution-tool admission, structured deferred-result schema, enforcement of the resolved composer gate, pending-state recovery, and archive-after-handoff transition.
- Failure-safe in-memory shell preservation and gating of every Fjall-dependent command without tearing down windows.

# Checkpoint - 2026-07-10 Progressive Shell Startup

This checkpoint replaces the current visible startup loading screen with a progressively enabled conversation shell, separates readiness of the thread catalog, selected transcript, composer editing, and CAS-backed submission, and supersedes older wording only where it would disable composer editing during CAS warm-up.

## New Resolutions

- The first Beryl surface shown for each restored main window is the ordinary conversation shell, not a loading screen or temporary startup page.
- The minimal Beryl-home and session-restore read needed to know which windows exist may precede window creation, but it must not introduce a visible intermediate loading surface.
- After a main window shell exists, thread-catalog loading, selected-thread history loading, and CAS warm-up proceed independently.
- Each shell region becomes enabled as soon as its own required state is ready; one slow dependency must not keep unrelated ready regions inert.
- The thread selector remains dim and inert until Beryl has loaded a complete compact metadata snapshot for every catalog thread.
- The catalog snapshot contains all metadata needed for immediate row display, recency ordering, filtering, and search, but it does not load thread turns or transcript items.
- Once that metadata snapshot is ready, the thread selector becomes enabled even if the selected thread history or CAS is still loading.
- Thread-selector interaction must remain responsive after activation. All catalog metadata is available up front, while row presentation remains bounded or virtualized rather than eagerly constructing GUI rows for every thread.
- The conversation viewport remains dim and inert until the selected thread's initial Syndic history view is ready.
- Once that initial history view is ready, the viewport becomes browsable without waiting for CAS.
- Beryl begins warming the runtime required by each restored window only after its shell exists. Warm-up for the same runtime is process-wide work that is coalesced rather than duplicated per window.
- Loading a thread into the metadata catalog does not by itself warm that thread's runtime. Beryl does not start every runtime merely because one of its threads appears in the catalog.
- Shared runtime readiness is projected into every affected window independently. Each window owns its own visible loading, ready, and error presentation.
- Window independence applies to selection, navigation, drafts, interaction state, and visible presentation. It does not duplicate home-wide catalog data, Fjall health, runtime processes, CAS connections, or exclusive thread-open reservations.
- Updates to shared home or runtime facts fan out to affected windows, but each window responds through its own shell state and GUI rather than through a process-wide loading or error window.
- Pending CAS warm-up does not disable composer text editing. The user may begin typing while CAS is starting.
- Submission remains unavailable and no user data is sent until CAS is confirmed ready for the selected thread's runtime.
- A CAS startup failure does not discard composer text. The affected window presents its own failure and retry state while Syndic-backed history remains browsable.

## Operator-Owned Details Still Open

The following durable-draft and GUI-clarification checkpoint resolves or reframes this list. The remaining operator decisions are the first-launch state before any runtime/root exists, the final autosave default and disable policy, and final open-elsewhere row presentation.

## AI-Delegated Details Still Open

- Exact startup scheduling and process-wide deduplication of catalog reads, selected-thread reads, and per-runtime CAS warm-up.
- Compact in-memory catalog snapshot and index design that makes sort, filter, and search immediate without retaining turn bodies.
- Bounded or virtualized thread-selector rendering with stable row identity, selection, focus, scroll position, and anchored transient-GUI behavior.
- Invocation of the existing anchor-relative conversation chunk loader and atomic replacement of the dimmed prior viewport when the requested chunk becomes ready, without redesigning transcript retention or rendering.
- Atomic readiness and command-gating rules ensuring that submission cannot race CAS readiness, committed-tail or draft loading, Fjall failure, active-turn state, or resolution-pending state.

# Checkpoint - 2026-07-10 Durable Draft Turns And Progressive GUI Clarifications

This checkpoint preserves the current anchor-relative conversation loader, makes composer drafts durable per-thread Syndic state, removes ordinary no-thread shells once an execution context exists, and resolves the provisional loading and open-in-another-window presentation. It supersedes older mutable-head-only wording where the new current-draft lifecycle requires separate committed conversation-path and draft state, supersedes the blanket rule that every Syndic turn mutation requires CAS, and supersedes older wording that disabled the composer editor merely because thread history, a runtime binding, or CAS was unavailable while Fjall remained healthy.

## New Resolutions

- Conversation rendering and history loading remain architecturally unchanged by this startup rework. Beryl continues loading bounded conversation chunks relative to an anchor using the current mechanism.
- Progressive startup invokes that existing conversation loader for the selected thread; it does not introduce a new transcript loading, retention, pagination, or virtualization design.
- The initial Fjall read that discovers the restorable window set occurs invisibly while no Beryl windows exist. Its session/window records and read path must therefore remain small, direct, and efficient.
- The only progressively loaded shell regions are the thread selector for its metadata catalog, the conversation viewport for its anchor-relative Syndic chunk, and the composer region for CAS-backed submission readiness.
- While one of those regions is loading, Beryl dims that region. No separate loading screen, skeleton page, or process-wide loading surface is introduced.
- The composer editor remains writable before selected-thread history and CAS are ready. Pending readiness disables submission, not typing.
- Explicit semantic gates still override ordinary editability: persistent Fjall failure disables composition, and accepted branch-resolution intent disables the discussion composer while handoff is pending.
- Loading and startup failures continue through Beryl's established per-window error alerts rather than gaining a new startup-specific error surface.
- A failed catalog, conversation, or CAS load does not disable unrelated regions that already have their own required state. The failed region remains unavailable until retry or recovery, except that CAS failure still permits editing the durable composer draft.
- Each Syndic thread owns exactly one current durable draft turn, and composer text is the content of that draft turn.
- A draft turn is explicitly typed mutable pre-submission state. It is not ordinary committed transcript narrative, is not supplied to CAS before submission, and is not shown as a conversation item in the viewport.
- The immutable branch-discussion context turn is not itself a draft. A newly created discussion thread owns that context turn and a separate current draft turn for the user's first input.
- Editing a draft updates only draft-owned mutable content and revision metadata. It does not mutate committed historical turns or immutable parent links.
- Submitting a draft must atomically freeze the submitted input and establish a replacement current draft so the thread never enters an ordinary state without a draft.
- Draft content is stored in Fjall and restored with its Syndic thread across app restarts, thread switches, runtime/root rebinds, CAS failures and retries, and ordinary window close/reopen flows.
- Draft autosave writes only when content changed. The interval is a Beryl setting; approximately 30 seconds is the current default candidate rather than a final value.
- Normal lifecycle boundaries such as thread switch, runtime/root rebind, ordinary window close, app `Exit`, and submission flush dirty draft state rather than waiting for the periodic autosave timer.
- Once a runtime/root execution context exists, a main window may not remain without a Syndic thread. If that context has no suitable thread, Beryl creates and selects a new thread with its current draft turn.
- A thread whose only conversation state is an empty current draft is the replacement meaning of an empty thread and may participate in the existing scoped reuse policy.
- A thread already open in another main window remains visible in the selector but is unavailable for selection.
- That selector row has a visible open-elsewhere indicator and a hover/focus tooltip explaining that another window already has the thread open. Wrapping the title in `[` and `]` is a candidate indicator, not yet a fixed theme contract.
- Clicking an open-elsewhere thread to focus its owning window is deferred as a possible later enhancement.

## Operator-Owned Details Still Open

- The first-ever-launch behavior when no runtime/root execution context exists. The earlier no-remembered-runtime flow requires a main window, while the new no-thread rule requires a runtime/root before that window can own a thread.
- Creation of an additional main window under the same invariant: Beryl must claim or create its selected thread before the main window exists, or use a separate pre-window selection surface.
- The final default draft-autosave interval and whether Settings may disable autosave entirely or only tune a required interval.
- The final open-elsewhere indicator and later decision on whether activating that row should focus its owning window.
- Draft activation timing: whether restored-window drafts load in the invisible pre-window bootstrap and whether a thread switch remains pending until the target draft is loaded. This avoids merging early edits with unseen persisted content and is the recommended behavior.

## AI-Delegated Details Still Open

- Exact Syndic draft-turn kind, record shape, ownership pointers, revisioning, compare-and-update rules, and separation from committed conversation-head semantics.
- Atomic draft submission, submitted-input freezing, replacement-draft creation, active-turn and queued-turn correlation, recovery, and duplicate prevention.
- Dirty detection, timer coalescing, lifecycle flush ordering, Fjall persistence mode, autosave retry, and bounded write amplification across many open windows.
- Atomic create-or-reuse of a thread and its draft when a runtime/root context would otherwise have no selected thread.
- Efficient pre-window loading of restored current drafts and lazy single-thread draft loading, caching, and activation after the operator fixes the visible switch contract.
- Disabled selector-row hit testing and keyboard behavior that preserve the tooltip anchor without allowing pointer, keyboard, touch, or programmatic selection.

# Checkpoint - 2026-07-10 Branch Context In The First Durable Draft

This checkpoint supersedes the separate branch-context-turn model. Once every Syndic thread has a durable draft turn, that first draft can durably anchor the branch and own its immutable discussion context without adding an otherwise empty context-only turn.

## New Resolutions

- `Discuss in new branch` creates a new Syndic discussion thread whose current draft has the selected source turn as its immutable parent.
- That first draft owns the selected reply text, exact source turn/item/range provenance, and any other durable context derived from the selection.
- The discussion thread record, not any turn, owns the stable parent Syndic thread id used as the handoff target.
- The draft's immutable parent turn identifies the historical branch point. The discussion thread's parent thread id identifies the mutable named thread to which resolution is eventually appended; these are distinct relationships.
- Only the composer-input portion of the draft is mutable. Its parent and branch-context/provenance fields are immutable after branch creation.
- Syndic does not need to mirror CAS's wire schema literally. It owns durable typed branch context and provenance; the CAS projection layer maps the relevant entries to `turn/start.additionalContext`.
- Creating the branch and its context-bearing draft does not start CAS or run the model.
- On first submission, Beryl freezes that same draft into the first committed discussion turn, retaining its immutable branch context, and atomically creates the thread's replacement current draft.
- The selected context is supplied through CAS `turn/start.additionalContext` when that first draft is submitted.
- Replacement drafts do not need to copy the branch context on every user turn. The first committed discussion turn retains it durably, while the discussion thread's metadata keeps both the context owner and parent handoff target discoverable for the readonly block, recovery, fresh CAS materialization, and resolution.
- The readonly discussion-context block is reconstructed from that durable branch context rather than from synthetic transcript narrative.

## AI-Delegated Details Still Open

- Exact Syndic branch-context envelope, immutable-field enforcement, provenance representation, thread-owned parent-thread binding, and efficient context-owner lookup from the discussion thread.
- Exact projection of the durable context envelope into CAS `additionalContext`, including source keys, context kind, escaping, budget, replay, and fresh-CAS rematerialization.
- Atomic branch-thread plus first-draft creation and atomic first-draft freeze plus replacement-draft creation without a transient headless or draftless thread state.

# Checkpoint - 2026-07-10 Split New Thread And Window-Scoped Empty Threads

This checkpoint supersedes hold-to-open and no-runtime click-failure behavior for `New Thread`, resolves first-runtime onboarding through the same control, and defines new-window thread acquisition without a runtime-global default thread.

## New Resolutions

- A new Beryl home has no configured runtimes by default.
- Zero configured runtimes is the one allowed state in which the initial main window has no Syndic thread. Once any runtime exists, every main window must own a selected Syndic thread.
- `New Thread` is a split button: one visually unified control with a primary `New Thread` segment and a smaller secondary `...` segment.
- Holding or long-pressing the primary segment is not the runtime/root-selection interaction.
- When the current window has a runtime/root context, the primary segment creates or reuses an eligible empty thread for that same runtime/root, claims it for the window, and switches the window to it.
- The secondary `...` segment opens the runtime/root management and selection flyout.
- When no runtime exists, the primary `New Thread` segment is disabled. Its disabled-state tooltip explains that a runtime must first be added through the `...` segment.
- In that zero-runtime state, the secondary `...` segment uses the theme's attention-drawing `secondary` style so the available entry point is visually prominent.
- Once a runtime exists, the secondary segment returns to its ordinary split-button presentation.
- The secondary `...` segment is the single product entry point into runtime and root configuration.
- Successfully adding a runtime automatically adds that runtime's user home directory as its non-removable default root.
- Runtime addition is not product-visible as successful without that default home root. Exact probing, persistence, rollback, and recovery mechanics are AI-delegated.
- Adding the first runtime automatically creates or claims the current window's default empty thread under that runtime and default root, closes the setup flow, and switches the conversation shell to that thread.
- CAS warm-up may continue after that switch. The draft is immediately editable while submission remains gated on CAS readiness.
- A default new thread is window-scoped, not a singleton owned by a runtime. Different windows cannot share it.
- Empty-thread reuse is home-wide within the existing `(runtime, root)` scope but may claim only an eligible thread not occupied by another window.
- An eligible reusable empty thread has never had any draft submitted into its historical conversation path. The always-present durable current draft does not itself violate this condition.
- A submission makes the thread permanently ineligible for automatic empty-thread reuse even when CAS produces no response or the submitted turn later stops, fails, disconnects, or remains incomplete.
- The current draft must contain no user-authored composer payload or attached resources at claim time.
- The thread must be an ordinary unarchived, unpinned thread with no manual title, branch-discussion context or parent-thread binding, active turn, queued input, resolution or handoff job, or occupying/restoring window claim.
- If no eligible unoccupied empty thread exists, Beryl creates a new durable thread with its durable current draft.
- `Ctrl+Shift+N` creates an additional main window using both the runtime and root of the invoking window's selected thread. Beryl claims or creates that new window's thread before showing the window, so the new main window never appears threadless when a runtime exists.
- `Ctrl+Shift+N` is disabled while zero runtimes exist; runtime setup remains owned by the existing initial window's secondary `...` segment.
- If the invoking window is already on an eligible pristine empty thread, primary `New Thread` is a no-op and remains on that thread rather than creating or claiming another empty thread.
- The new window may reuse an eligible unoccupied empty thread; it never reuses the invoking window's occupied thread.
- A window's remembered new-thread runtime/root changes only when a runtime/root change or thread activation is successfully committed. Hovering, focusing, or cancelling inside the flyout does not change it.
- Exact flyout composition and the add/select runtime/root interactions remain intentionally deferred for the next GUI discussion.

## Operator-Owned Details Still Open

- Final secondary-segment iconography, accessible name, tooltip, and focus/keyboard presentation; `...` is the current visual direction.
- Exact disabled-command and keyboard-shortcut explanation when zero runtimes exist, while consistently directing runtime setup to the `...` segment.

## AI-Delegated Details Still Open

- Atomic claim-or-create across simultaneous windows so two windows cannot acquire the same empty thread.
- Deterministic selection among multiple eligible empty threads, claim release, crash recovery, and stale window-occupancy cleanup.
- Atomic or recoverable first-runtime, non-removable-home-root, first-thread, and window-activation orchestration.
- Split-button implementation mechanics after the operator fixes its final GUI contract.

# Confirmed Direction

The next large rework should remove Beryl workspaces as both product UI and architectural authority.

Beryl should also fully remove semantic graph functionality. This means deleting the graph feature, graph tools, graph upkeep, checklists, checklist-bound threaded decisions, graph thread refs, graph overlay, graph persistence, graph diagnostics, graph dynamic tools, and graph-derived workflows. This is intended as a clean deletion with no compatibility path and no "bring it back later" preservation layer.

Branch, explore, and merge remains a first-class product workflow, but it must be redesigned without semantic graph or checklist authority. Semantic search also remains a desired product feature, but its graph-independent implementation is deferred until after this rework rather than being kept live through compatibility code.

The theme system also requires operator-owned redesign. Themes must continue to support configuration of every intended GUI piece and style/role inheritance, while the style-role hierarchy and the theme editor's presentation of that hierarchy are replaced deliberately.

Beryl should move toward a model where a Beryl home contains all Beryl-owned state, including Syndic storage, Beryl GUI metadata, settings, image assets, runtime/root metadata, themes, and any app-local UI state that is intended to survive restart.

Home-level durable metadata should use one physical Fjall database layout for lower overhead where practical, but logical ownership must stay separated. Syndic owns the thread and turn data model, including stable thread ids, committed conversation-path state, current durable draft turns, turn parentage, the conversation DAG, and projection records. Beryl-owned GUI and application metadata such as thread presentation state, titles, root bindings, archive or pin state, and window/app records should live in separate Fjall keyspaces behind Beryl-owned typed APIs.

# Resolved Decisions

## Beryl Home

- A Beryl home is the physical directory containing all Beryl state.
- Two separate Beryl homes cannot point to the same Syndic storage inside one another.
- The architecture does not need to support two different Beryl homes sharing one Syndic store.
- One OS process may own a Beryl home at a time.
- Multiple Beryl windows are allowed inside the same OS process.
- A Beryl home needs a durable file lock so a second OS process cannot open the same home concurrently.
- If startup finds the Beryl home busy, Beryl should show a busy-home surface with a single exit button.
- The busy-home surface should self-exit after about 5 seconds.
- The process exit code for this case should be `1`.
- Shared in-process state includes storage handles, settings, theme repository, backend managers, Syndic thread access, Beryl GUI metadata, and root/runtime registries.
- Window-local state includes selected thread, last-used execution root for new threads, selected or pending new-thread root choice, in-memory editing state for the selected thread's durable draft, scroll position, popups, activity panel presentation, pending activation chrome, and navigation history.
- Durable composer content is owned by the selected Syndic thread's current draft turn in Fjall rather than by window/session metadata.

## Beryl Home Metadata Store

- Home-level durable metadata should use one physical Fjall database layout for lower overhead where practical.
- Logical ownership remains separated by typed APIs and Fjall keyspaces.
- Syndic keyspaces own the thread and turn data model, including stable Syndic thread ids, committed conversation tails, current durable draft turns, the conversation DAG, and projection records.
- Beryl GUI metadata keyspaces own GUI and application records keyed by Syndic thread id where needed, including execution-root tuples, titles, archive or pin state, window/app state, and GUI asset references.
- Raw Fjall handles, key encodings, and transaction details should not leak across logical storage domains.
- Manual and generated thread titles are Beryl-owned metadata keyed by stable Syndic thread id.
- CAS thread names, CAS thread lists, and CAS metadata reads are not title or catalog authority.

## Fjall Integrity

- Fjall database integrity is a hard requirement.
- Beryl may lose in-flight data on process panic, abort, power loss, or Windows/kernel failure.
- Beryl must not leave the Fjall database structurally corrupted, unreopenable, or partially unusable after those failures.
- Accepted durable writes should use the strongest practical Fjall persistence mode for the Beryl home store.
- Existing Syndic storage currently commits the Fjall batch and calls `persist(PersistMode::SyncAll)` when sync-after-commit is enabled.
- The home metadata store should preserve that level of durability unless a target doc explicitly accepts weaker durability for a specific non-critical metadata class.
- Beryl-owned code must not mutate Fjall database files directly or bypass Fjall journal/recovery machinery.
- Crash consistency must be verified with tests or harnesses that interrupt Beryl during writes and then reopen and validate the database.
- The target is database wholeness and full reopenability; the latest in-flight operation may be missing or represented as an incomplete durable record according to the relevant record lifecycle.
- If persistent Fjall failure occurs while Beryl is running, Beryl keeps every existing main window, its open thread identity, geometry, virtual-desktop placement, and last coherent visible presentation intact in memory.
- Persistent Fjall failure disables history loading and navigation, composition and submission, settings, and all other operations requiring Fjall-backed Beryl state.
- Beryl must not close or replace the windows with a startup-like surface merely because the durable store becomes unavailable.
- Beryl must not fall back to CAS history or another authority for Fjall-dependent behavior.
- Fjall remains the sole durable owner of window/session restore data; Beryl does not write a redundant restore snapshot outside Fjall.
- If Fjall is already corrupted or unreadable at startup, window/session restoration is not guaranteed.
- Exact failure, retry, and recovery presentation is operator-owned GUI design.

## Window Restore

- Beryl should restore main windows with threads that were marked open for session restoration.
- Auxiliary settings windows and transient windows are not restored as session windows.
- Closing one main window normally removes it from the next restored session.
- A dedicated toolbar `Exit` action exits Beryl while keeping all currently open main windows marked for restoration.
- Panic, crash, OS reboot, and other external process termination preserve the last durable open-window set for restoration.
- Restored windows should keep the same positions.
- Restored windows should keep the same sizes.
- Restored windows should return to the same Windows virtual desktops when those desktops still exist.
- A window whose former virtual desktop no longer exists is restored on the first virtual desktop, not whichever desktop happens to be current.

## CAS-Optional Startup

- Beryl startup should not select a runtime environment.
- Beryl startup should not touch CAS before the main shell can open.
- Startup must not require CAS to be present, launchable, authenticated, or healthy.
- Startup opens the Beryl home and restores durable window state from Beryl/Syndic-owned storage.
- Each restored window reopens its last selected Syndic thread when that state exists.
- A fresh Beryl home with no configured runtime opens one initial main conversation shell in the sole permitted threadless state.
- That initial shell does not invent a runtime, root, or thread. The split button's primary `New Thread` segment is disabled, while its attention-drawing secondary `...` segment opens the flyout for adding the first runtime and thereby creating the first root and thread.
- Runtime environment means the host Windows environment or one WSL distro where the CAS `codex` binary would run.
- For each unique runtime environment required by restored open windows, Beryl attempts to start or connect to CAS after the shell can exist.
- CAS startup failure is non-fatal for the app shell.
- If CAS startup fails for a required runtime, Beryl raises a per-window error notification and marks CAS-backed mutating actions unavailable for affected windows or threads.
- The CAS startup failure notification includes a retry button.
- If CAS is absent from the environment, Beryl still starts and leaves Syndic history browsable.
- Any read-only action with enough local or Syndic-backed information may operate while CAS is unavailable.
- Beryl-owned metadata mutations such as title, pin, archive, and empty-thread creation may operate without the thread's currently bound CAS.
- Starting a conversation with AI requires CAS availability for the relevant runtime.
- Submitting a draft for AI execution and applying CAS-produced turn state require CAS availability. Creating or autosaving an ordinary or branch-context-bearing current draft does not require CAS.
- Rebinding away from an unavailable runtime is allowed when the destination runtime and its CAS are available.
- CAS and the bound working directory do not participate in history browsing; history browsing is entirely Syndic-backed.
- CAS or working-directory unavailability must not be presented as transcript/history load failure.
- Startup does not select an execution root.
- Execution roots are not replacements for workspace selection at startup.

## Progressive Shell Readiness

- The ordinary conversation shell is the first visible surface of every restored main window; Beryl does not show a startup loading screen.
- The minimal home/session bootstrap needed to discover the durable open-window set happens invisibly before those shells can exist and must use a small, direct, efficient Fjall read path.
- Thread-catalog loading, selected-thread history loading, and required-runtime CAS warm-up proceed independently after shell creation.
- The thread selector is dim and inert until a complete compact metadata-only catalog snapshot is ready, then becomes responsive without waiting for transcript or CAS readiness.
- Catalog readiness includes all metadata needed for row presentation, recency ordering, filters, and search, but excludes Syndic turns and transcript items.
- The catalog model is complete up front, while the selector's repeated GUI rows use bounded or virtualized rendering so total thread count does not determine render-tree size.
- The conversation viewport is independently dim and inert until the selected thread's ordinary anchor-relative Syndic chunk is ready, then becomes browsable without CAS. Existing conversation rendering and chunk loading remain unchanged.
- CAS warm-up begins after shell creation and is coalesced once per unique required runtime across windows.
- Catalog membership alone does not require runtime warm-up; only the runtime needed by an open window's current execution context is proactively prepared.
- Each affected window independently presents the shared runtime's readiness or failure state.
- Pending thread-history or CAS readiness does not gate composer text editing, but it gates submission. Beryl sends no user data until the selected thread state and CAS are confirmed ready.
- Draft text persists through the thread's Fjall-backed current draft turn rather than window/session metadata.
- Loading dims only the affected thread-selector, conversation-viewport, or composer region. Failures use the established per-window error alerts.
- CAS startup failure preserves the durable draft and leaves readable Syndic history available.

## Execution Roots

- Execution roots are directly tied to threads.
- A Syndic thread has an associated execution root used when Beryl needs CAS-backed execution for that thread.
- The execution root should initially be stored as a `(runtime, directory)` tuple associated with the Syndic thread.
- Runtime means either host Windows or a specific WSL distro.
- Roots do not need to be first-class durable objects in the initial target.
- Existing-thread activation automatically switches to the runtime and execution root bound to that thread.
- New-thread creation needs a way to choose or infer an execution root before the first backend turn can run.
- Execution roots define the CAS working directory context, including `AGENTS.md` and skill discovery behavior, because those remain CAS-owned behaviors.
- Existing threads must not silently rebind to another execution root.
- Explicit rebind is a separate operation and must never happen silently.
- A thread may be rebound away from an unavailable runtime when the destination runtime and its CAS are available.
- Missing or unavailable execution roots should not erase threads or history.
- Missing or unavailable CAS should not erase threads or history.
- If a preexisting thread's bound runtime or bound directory is unavailable, Beryl still allows reading that thread from Syndic history.
- If a preexisting thread's bound runtime or bound directory is unavailable, Beryl disables submission and other CAS-backed mutating commands. The composer editor remains writable and continues updating the durable draft while Fjall is healthy and no explicit composer gate such as resolution-pending state applies.
- If CAS is unavailable for a thread's runtime, Beryl still allows reading that thread from Syndic history. CAS-backed mutations are disabled, while locally valid Beryl-owned metadata actions remain available.
- Beryl allows changing runtime and directory for any thread, not only threads with currently invalid runtime/directory bindings.

## Maintenance Turns

- Maintenance turns should avoid project-specific `AGENTS.md` and skill discovery when that context is not intended.
- Beryl may reserve an empty directory under the Beryl home for maintenance turns.
- Runtime-specific details remain open, especially for WSL-backed maintenance turns.

## Syndic Threads And Turns

- Threads are normally created through `New Thread`, and may also be created automatically to satisfy the one-selected-thread invariant when a window activates a runtime/root execution context that has no suitable thread.
- A turn existing in the shared DAG does not by itself define or create a Syndic thread.
- `New Thread` should immediately create or reuse a durable Syndic thread whose current draft is empty.
- `New Thread` is a split button. Its primary segment creates or reuses an eligible empty thread in the current window's runtime/root, while its secondary `...` segment opens runtime/root configuration and selection.
- When zero runtimes exist, the primary segment is disabled with an explanatory tooltip, and the attention-drawing secondary `...` segment remains enabled as the runtime-configuration entry point.
- Adding a runtime automatically adds that runtime's user home directory as its non-removable default root.
- Adding the first runtime automatically creates or claims the current window's thread under that default root and switches the shell to it.
- Hold or long-press is not used to open the runtime/root flyout.
- Empty-thread reuse is scoped by `(runtime, directory)`.
- Empty-thread acquisition may reuse only a qualifying thread not occupied by another main window; otherwise Beryl creates a new durable thread and draft.
- If the current thread is already a qualifying pristine empty thread, primary `New Thread` remains on it as a no-op.
- Threads whose only conversation state is an empty current draft appear in the thread catalog.
- Syndic threads have stable thread ids.
- A Syndic thread may be open in at most one main window at a time.
- A Syndic thread may have at most one active turn at a time.
- Closing the main window that owns an active thread interrupts that thread's active turn.
- A Syndic thread is a stable named reference that owns committed conversation-path state and exactly one current durable draft turn.
- Turns do not define Syndic threads.
- The current draft is mutable pre-submission state with a stable identity and revision. It is excluded from transcript narrative and CAS projection until submitted.
- In this draft, a `submitted conversation turn` is a former draft frozen into the historical conversation path. Submission does not imply that CAS responded or that the turn completed successfully; avoid using `committed` alone where it could be confused with Fjall durability.
- A draft may also own immutable parentage and a typed immutable context/provenance envelope. Autosave may change composer-owned mutable content but never those immutable branch fields.
- Submitted, active, incomplete, and other non-draft turns remain durable conversation-DAG state according to their explicit type and lifecycle.
- An ordinary new-thread draft begins without branch context. A branch-discussion draft has the selected source turn as its immutable parent and owns durable selected-context provenance without starting CAS.
- The visible transcript for a Syndic thread is the path from its committed conversation tail back to the root; the current draft is not part of that visible path.
- Each committed turn has at most one parent turn, so walking from the committed tail to the root gives one flattened transcript path.
- Turn parent links are historical graph structure and are immutable once their turn leaves draft state.
- Different threads may point to the same historical conversation tail.
- If two threads share tail `T1`, submitting thread `A`'s draft establishes `T2(parent = T1)` only on `A`'s path.
- If thread `B` still points at `T1` and submits later, Beryl establishes `T3(parent = T1)` only on `B`'s path.
- The result is a natural branch in the Syndic turn graph without a conflict between the two threads.
- Use "Beryl thread" only for GUI representation of a Syndic thread, such as a row, selected surface, or view model.

## Syndic Thread And Draft Mutation

- Same-thread mutation needs a Beryl-orchestrated atomic guard in Syndic-owned thread storage.
- The guarded state includes the thread's committed conversation tail plus its current draft identity and revision, not the immutable historical parent turn itself.
- Draft autosave uses revisioned compare-and-update and may change only draft-owned mutable content and metadata.
- Submitting user input atomically freezes the current draft as ordinary conversational input, retains any immutable branch-context envelope, advances the committed path, creates the replacement current draft, and preserves exact parentage.
- Thread and draft coordination should avoid durable exclusive locks in Fjall.
- The Syndic thread storage API should expose revisioned compare-and-swap operations for committed-tail and draft mutation.
- Sending user input confirms an ordinary conversational turn because the frozen draft input is the beginning of that turn.
- An explicitly requested branch-discussion operation atomically creates the new discussion thread plus its context-bearing current draft without CAS execution.
- Incomplete turns from user stop, backend disconnect, crash recovery, or similar interruption remain durable committed conversation turns while the thread retains its separate current draft.
- If another in-process operation already advanced the same thread or draft revision, the operation must reject or replan instead of creating a competing child for the same named thread.
- Different threads appending from the same historical turn do not conflict.
- Replacement edit moves the existing Syndic thread's committed tail and current draft binding to the replacement path.
- Replacement edit must not rewrite or detach shared historical turns in a way that damages another thread's path.

## CAS Projection

- Syndic is the durable truth for conversation history.
- CAS is the runtime process, meaning the environment where the `codex` binary runs.
- CAS remains the live execution authority and policy boundary for new live turns.
- Beryl uses CAS only for live turns.
- Browsing existing Syndic threads in Beryl reads Syndic history and does not use CAS.
- Every live Syndic thread being executed through Beryl must have its own exclusive CAS thread.
- A CAS thread must not be shared between two Syndic threads, even when those Syndic threads currently point at the same Syndic turn.
- When Beryl attempts to append to a Syndic thread, it either reuses the exclusive CAS thread already assigned to that Syndic thread or creates/forks a new CAS thread and assigns it exclusively to that Syndic thread.
- Creating or forking that exclusive CAS assignment must inherit the prior Syndic turns needed for the append attempt.
- If an assigned CAS thread is lost after backend restart, Beryl starts a new CAS thread and provides the needed Syndic history.
- CAS does not directly support starting a new thread from supplied history as backend-owned prior turns.
- The practical recovery path is to create a CAS thread and send a new live turn with the relevant Syndic history packed into hidden model context.
- Current Beryl backend code supports `thread/start`, `turn/start`, `thread/fork`, and `thread/rollback`.
- The currently installed `codex-cli 0.144.1` schema exposes ordinary user input, developer-instructions collaboration context, and a separate `additionalContext` map with application or untrusted context entries on `turn/start`.
- The target CAS version selected for this rework must expose `turn/start.additionalContext`.
- `turn/start` returns a CAS turn id through `TurnStartResponse.turn.id`.
- Live turn stream events carry CAS `thread_id` and `turn_id` identities for turn and item events.
- CAS assignment proof should record the CAS thread id Beryl targeted, the CAS turn id returned and confirmed by stream events, and the Beryl/Syndic selected-path revision or digest used to assemble the live turn.
- Packing Syndic history into hidden context for a new CAS thread is an AI-delegated implementation and system-design task constrained to the targeted existing CAS protocol. Beryl will not modify CAS.
- CAS adaptation is an internal system behavior and should not force branch structure into the UI.

## Branch UI

- The UI does not need to expose graph-sibling or branch-sibling structure for now.
- The immediate thread selector is recency-first and may expose runtime, execution root, archive state, pin state, and search as filters.
- Branch visualization is an interesting future UI exploration, but not part of the immediate target.

## Branch Discussion And Resolution Handoff

- Selecting text in a rendered assistant reply exposes `Discuss in new branch` alongside the existing quote interaction.
- The new Syndic discussion thread branches from the turn containing the selected text and inherits prior turns through ordinary immutable Syndic parentage.
- Branch creation atomically creates the discussion thread with a current draft whose parent is the selected source turn and whose immutable context envelope owns the selected text and provenance, without starting CAS or running the model.
- The discussion thread itself owns the stable parent Syndic thread id used for resolution handoff. No shared or committed turn owns thread-to-thread membership or handoff binding.
- The first user-authored submission freezes that same draft as the first committed discussion turn, retains its context envelope, creates the replacement current draft, and supplies the selected text through CAS `turn/start.additionalContext`.
- Selected discussion text and exact source provenance are durable context metadata, not synthetic transcript narrative.
- Beryl passes that selected context through the targeted existing CAS `turn/start.additionalContext` field without modifying CAS or using developer instructions as a history channel.
- The discussion GUI shows the selected context in a readonly convenience block that is separate from transcript narrative.
- The user continues with ordinary conversation in the discussion thread.
- Resolution and handoff are initiated through conversation and a Beryl-scoped CAS dynamic tool, not a GUI resolve/archive button.
- When its admission gates pass, the tool durably admits resolution intent for the bound discussion. Beryl-owned lifecycle orchestration schedules a real resolution handoff turn on the exact parent thread and archives the Syndic discussion in Beryl catalog state only after that handoff succeeds.
- CAS archive or thread-list metadata does not become archive authority.
- If the parent is active, the handoff is durably queued in Fjall until the parent becomes eligible after the active turn reaches terminal state.
- If the discussion has accepted future-turn input queued when the tool is called, resolution is not admitted. The tool returns a retryable deferred result, queued turns run normally, and the AI may retry in a later turn unless intervening user input or steering cancels the requested resolution.
- Deferred resolution does not disable the composer or change archive state and is not automatically retried by Beryl.
- Durable acceptance of resolution intent immediately puts the discussion into a resolution-pending state with its composer disabled.
- The discussion remains unarchived while resolution handoff is pending and archives only after the parent handoff turn succeeds.
- Pending handoff state survives restart and uses idempotent, exact-identity orchestration so retries cannot duplicate the parent turn.
- Readonly-block GUI composition, post-archive window navigation, and visible pending/failure behavior remain operator-owned decisions.

# Non-Trivial Decisions Still Open

Every unresolved area remains listed even when its resolution is delegated. `Operator-owned` means the operator must approve the product or GUI decision. `AI-delegated` means the target-design or implementation agent should resolve it within the recorded constraints without escalating ordinary technical choices. `Split ownership` identifies both parts explicitly.

## New Thread Split Button And Runtime/Root Flyout

Decision owner: **Operator-owned GUI design, with AI-delegated atomic claim and runtime-setup mechanics.**

The primary interaction is resolved as a split button. Its `New Thread` segment creates or reuses an eligible unoccupied empty thread for the current runtime/root, except that it is a no-op when the current thread is already pristine-empty. Its smaller `...` segment opens the runtime/root flyout.

With zero runtimes, the primary segment is disabled with an explanatory tooltip and the secondary segment receives the theme's attention-drawing `secondary` style. Adding a runtime through that segment automatically adds its non-removable user-home root. Adding the first runtime also creates or claims and activates the initiating window's first thread.

`Ctrl+Shift+N` inherits both the selected thread's runtime and root, claims or creates the new window's own thread before showing it, and is disabled while zero runtimes exist. Reusable-empty eligibility is resolved: no draft has ever been submitted, the current draft is empty of all composer payload, and the ordinary thread has no archive, pin, manual title, branch binding, active/queued/pending work, or window claim. Remaining operator decisions are the flyout composition; add/select runtime and root interactions; final secondary-segment iconography, accessible name, tooltip, focus, and keyboard behavior; and exact zero-runtime disabled-command explanation. A successful committed runtime/root or thread change updates the window's remembered target; hover, focus, and cancellation do not.

## Progressive Shell Startup GUI

Decision owner: **Split ownership.** Three-region dimming, established error alerts, always-editable ordinary drafts, and send gating are resolved product behavior. Exact later visual tuning remains operator-owned; scheduling, compact loading, virtualization, readiness bookkeeping, and race-free command gating are AI-delegated.

The first visible surface is the ordinary conversation shell. The thread selector, selected-thread viewport, composer editor, and send command have independent readiness gates rather than waiting behind one startup loading screen.

The selector activates only after the complete metadata-only thread catalog is loaded. The viewport activates when the selected thread's existing anchor-relative Syndic chunk is ready. Composer text editing remains available while thread state or CAS loads, but submission sends nothing until all submission gates are ready.

Loading presentation is resolved provisionally: dim only the corresponding selector, viewport, or composer region, and use established per-window error alerts for failures. While switching threads, the prior viewport stays dim and inert until the requested thread's anchor-relative chunk is ready, then Beryl replaces it atomically. Exact styling may be tuned later.

First-launch acquisition is resolved: the zero-runtime initial shell may be threadless; primary `New Thread` is disabled; and the attention-drawing secondary `...` segment opens runtime setup. Adding the first runtime creates its home root and the window's first thread. `Ctrl+Shift+N` is unavailable until then and later inherits both runtime and root. Remaining operator decisions are the final draft-autosave default and disable policy, final open-elsewhere row treatment, and whether restored and newly selected thread activation waits invisibly or visibly for its durable draft. AI-delegated mechanics include startup scheduling, process-wide read and runtime-warm-up deduplication, compact catalog indexing, bounded or virtualized selector rows, invocation of the unchanged conversation chunk loader, viewport replacement, and atomic gating against CAS readiness, committed-tail and draft readiness, Fjall failure, active turns, and resolution-pending state.

## Durable Draft Turn And Autosave Details

Decision owner: **Split ownership.** The per-thread durable-draft lifecycle, persistence expectation, dirty-only saving, tunable setting, and no-thread invariant are operator-owned and resolved except for the listed policy choices. Record layout, concurrency, atomic submission, autosave scheduling, and recovery are AI-delegated.

Every Syndic thread owns exactly one current mutable draft turn distinct from committed transcript history. Submission freezes the current draft into the committed conversation path and atomically establishes its replacement. Each thread therefore retains a draft while an active turn runs or future input is being composed.

Dirty drafts persist in Fjall across thread switches, rebinds, CAS failures, window close/reopen, and app restart. Periodic autosave runs only for changed content, and dirty state is also flushed at normal lifecycle boundaries. Approximately 30 seconds is the current default candidate and Settings may tune the interval.

Remaining operator decisions are the final default interval, whether autosave can be disabled rather than merely retimed, and target-draft loading during startup and thread activation. The recommended activation contract is to load restored current drafts during the invisible pre-window bootstrap and keep a later thread switch pending on the old selection until the target draft is loaded, rather than accepting edits that would need to merge with unseen content. AI-delegated mechanics include draft revisions, compare-and-update, dirty detection, timer coalescing, lifecycle flush ordering, failure retry, submission sealing, replacement-draft creation, queued-turn interaction, caching, and crash recovery.

## Empty Thread Cleanup Details

Decision owner: **Operator-owned product and GUI policy.**

The durable-empty-thread decision now means a thread whose only conversation state is its empty current draft.

Reuse scope, eligibility, and catalog visibility are resolved. Reuse is scoped by `(runtime, directory)`. An eligible thread has never had a draft submitted, currently has no composer payload, and has no user-distinguishing metadata, branch binding, work, or window claim. A stopped, failed, disconnected, or incomplete submitted turn disqualifies the thread even if no CAS response exists. These draft-only threads appear in the catalog, and once a runtime/root context exists Beryl creates or reuses one rather than leaving a main window without a selected thread.

Turn garbage collection is deferred, but visible empty-thread policy remains open: what counts as abandoned, whether an empty catalog row is ever automatically removed, and what user action removes or archives an unwanted empty thread.

## Fjall Crash Verification Details

Decision owner: **Split ownership.** Crash harnesses, durability implementation, invariant verification, and command gating are AI-delegated. GUI-visible storage-failure and recovery behavior is operator-owned.

The integrity requirement is resolved: the Fjall database must remain whole and reopenable after Beryl panic, abort, power loss, or Windows/kernel failure, although the latest in-flight operation may be lost or incomplete.

AI-delegated details are the crash-test harness, which write classes require `SyncAll` under the accepted durability contract, and how recovered database invariants are validated after interrupted writes.

Real-world cases include a full disk rejecting a commit, a removable or network-backed home disappearing during a write, an I/O or permission error that persists across retries, or a reopened database that Fjall reports as damaged. The technical layer must fail the affected mutation rather than report success.

The running-app behavior is resolved: preserve every existing window shell, open thread identity, geometry, virtual-desktop placement, and last coherent visible presentation; disable history loading/navigation, composition, settings, and every other Fjall-dependent action; and do not fall back to CAS history.

The exact error, retry, and recovery presentation remains operator-owned. Fjall is the sole durable source for window/session restoration. Beryl does not keep a redundant snapshot outside Fjall, so restoration is not guaranteed when Fjall is already corrupted or unreadable at startup.

## Runtime, CAS Availability, And Rebind

Decision owner: **Split ownership.** Availability probes and state bookkeeping are AI-delegated. All visible affordances and disabled/error presentation are operator-owned.

The main unavailable-root UX is resolved.

Threads bound to unavailable runtimes or directories remain visible and readable from Syndic history. Beryl disables submission and other operations that require the unavailable binding, while the composer editor stays writable against the thread's durable draft unless Fjall failure or another explicit semantic gate disables it.

Threads whose required runtime cannot start CAS also remain visible and readable from Syndic history. Beryl reports the CAS failure through the established per-window error alert with a retry action and disables CAS-backed submission while preserving draft editing.

Beryl allows changing runtime and directory for any thread, not only threads with invalid bindings.

Beryl-owned metadata operations do not require the currently bound CAS. Rebinding away from an unavailable runtime is allowed when the destination runtime and its CAS are available.

Unavailable or unusable cases include missing paths, inaccessible paths, non-directory paths, permission failures, WSL distro unavailability for WSL-bound roots, and CAS absence or launch failure for the relevant runtime.

History browsing is independent of CAS and the bound working directory. AI-delegated details include availability checks and state transitions. Operator-owned details include how unavailable mutation state is shown without implying transcript/history load failure and what visible affordance performs runtime/directory changes.

## Multiple Active Turns

Decision owner: **Split ownership.** Concurrency bookkeeping is AI-delegated. Any multi-window activity/status presentation is operator-owned.

Different threads in the same execution root may run simultaneously.

Beryl should rely on CAS support for simultaneous active turns rather than adding a Beryl-side limit against them.

One Syndic thread may have at most one active turn. Two main windows may not have the same Syndic thread open. Closing the owning main window interrupts its active turn.

AI-delegated implementation decisions remain:

- What per-turn Beryl records are needed to route stop, stream, recovery, and status updates correctly.
- How per-thread gates prevent concurrent append, edit, rollback, stop, and compaction operations.
- How process-wide backend/account facts are correlated without conflating independent active turns.

Operator-owned GUI decisions remain for how independent active turns are displayed and stopped across their owning windows and how process-level account or rate-limit state appears in each window.

## CAS Exclusive Thread Materialization Mechanics

Decision owner: **AI-delegated system and protocol design.** Any resulting user-visible unavailable, oversized-history, or degraded-continuation state is operator-owned GUI/product behavior.

Define the exact mechanics for creating or refreshing the exclusive CAS thread assignment before Beryl submits a frozen draft from a Syndic thread's committed conversation tail.

The ownership rule is resolved: each live Syndic thread being executed through Beryl gets its own exclusive CAS thread, and browsing uses Syndic rather than CAS.

The Beryl-owned CAS assignment record should use revisioned compare-and-swap semantics rather than a durable exclusive Fjall lock.

The remaining mechanics are how Beryl creates or forks that exclusive CAS thread and how it packs previous Syndic turns into hidden model context when a fresh CAS thread is required.

The backend protocol provides enough live identity for the first proof layer: Beryl knows the CAS thread id it targeted, `turn/start` returns a CAS turn id, and live stream events carry CAS `thread_id` and `turn_id` identities.

The Beryl metadata proof should also record the selected Syndic path revision or digest used to assemble the live turn.

If an assigned CAS thread is lost after backend restart, Beryl starts a new CAS thread and provides the needed Syndic history.

CAS does not directly support starting a thread from supplied history as backend-owned prior turns. The target recovery path is a new CAS thread plus hidden context packing for the relevant Syndic history.

The exact history-pack format, budget, provenance, and use of the targeted existing CAS protocol remain to design. Beryl will not modify CAS. Generated schema for the currently installed `codex-cli 0.144.1` exposes `turn/start.additionalContext` as a map of application or untrusted context entries separate from ordinary user input and developer instructions. Target-version selection must require that field and delegated protocol investigation must verify its exact semantics.

This should remain a system-level contract, not thread-selector UI behavior.

## Branch Discussion GUI And Handoff Timing

Decision owner: **Operator-owned product and GUI design.** Durable binding, context, and queue mechanics are AI-delegated.

The thread-native workflow is resolved: select assistant reply text, invoke `Discuss in new branch`, atomically create a discussion thread whose first durable draft branches from the selected source turn and owns immutable selected-context provenance, show that context as a readonly non-transcript block, start CAS only when that draft is submitted through `additionalContext`, and resolve through a scoped CAS dynamic tool that creates or queues a real parent handoff turn. A tool call made while future turns are queued is deferred without state change and may be retried by the AI later; successful resolution admission disables the composer immediately, and the discussion archives only after successful parent handoff.

Remaining operator decisions are:

- What the owning discussion window displays or activates after archival, especially when the parent is already open in another window.
- Exact context-block placement, composition, selection, expansion, and visual behavior.
- Visible pending, failure, retry, and completion behavior for queued resolution handoff.
- Product behavior if the parent Syndic thread is deleted before resolution is handed off: prevent that deletion, leave resolution failed/pending, or provide another explicit recovery path.

AI-delegated mechanics include exact draft-owned immutable context envelope, selected-text provenance, `additionalContext` encoding and budget, first-draft submission and CAS projection creation, thread-owned parent-thread binding identity, atomic resolution-versus-queue admission, structured deferred-tool results, enforcement of the resolved composer gate, Fjall queue schema, restart recovery, idempotency, retry ordering, and duplicate prevention.

## Replacement Edit Semantics

Decision owner: **AI-delegated implementation under a resolved product contract.** Any later change to the visible edit interaction remains operator-owned.

GUI editing of old user input keeps the existing user-facing behavior.

In the GUI, the current thread tail disappears and the new user input is sent.

At the graph level, Beryl does not edit an existing Syndic turn.

Instead, Beryl creates a new turn containing the new user input and thereby creates a branch in the Syndic DAG.

The selected thread's committed tail and replacement current-draft binding move to the new branch path according to the ordinary atomic mutation rule.

The old tail remains in the DAG, even if no visible thread currently reaches it.

Reachability, old-tail discovery, and garbage collection are deferred.

If the new branch turn is stopped, disconnected, interrupted by crash, or otherwise lacks terminal successful completion, it follows ordinary incomplete-turn handling.

A failed write to Fjall is a storage failure, not an ordinary replacement-edit semantic branch.

## Thread Deletion And Garbage Collection

Decision owner: **Split ownership.** Immediate reference deletion mechanics are AI-delegated. The future `Collect Garbage` product and GUI design is operator-owned.

Deleting a thread should delete the named thread ref and its Beryl metadata, not the durable turns themselves.

Garbage collection for unreachable turns, resources, sidecars, CAS projections, and title metadata is explicitly deferred. Until a future `Collect Garbage` design exists, deletion preserves durable turns and related records that may still be reachable by another thread, diagnostic record, projection binding, or resource reference.

## Catalog Shape

Decision owner: **Split ownership.** Row composition and visible interaction are operator-owned. Compact snapshot, indexing, and bounded-rendering mechanics are AI-delegated after that GUI contract is fixed.

The immediate selector is recency-first, with runtime and execution root available as filters rather than mandatory hierarchy levels.

At startup the selector stays dim and inert until all thread metadata required for row display, ordering, filtering, and search is loaded into a compact snapshot. It then activates without waiting for the selected transcript or CAS. Thread turns and transcript items are not part of this catalog snapshot.

Threads already open in another main window remain visible but unavailable for selection. Their rows carry a visible open-elsewhere indicator and a hover/focus tooltip explaining the blocking window ownership. Wrapping the title in `[` and `]` is a provisional indicator candidate; the final themed treatment remains open. Click-to-focus of the owning window is deferred.

Other remaining GUI decisions include exact row composition, ordering rules within equal recency, filter presentation, archive and pinned-state presentation, search interaction, empty-state behavior, and activation feedback. Branch/sibling visualization is intentionally deferred.

The repeated row surface must use bounded or virtualized rendering with stable identities even though the compact metadata model covers the complete catalog. Snapshot and index design must keep filtering and search responsive and memory-efficient. The catalog must not use CAS thread-list, CAS metadata reads, or backend thread names as authority.

## Image Assets

Decision owner: **Operator-owned data-lifecycle and GUI decisions, followed by AI-delegated storage mechanics.**

Decide where Beryl image assets live after workspace directories are removed.

The current workspace-scoped asset directory must be replaced by a Beryl-home-wide asset store or per-thread asset store. The design must preserve runtime-readable path conversion for host and WSL submissions, stable labels, collision checks, and cleanup behavior.

The operator must decide the ownership and user-visible cleanup semantics because they affect future thread deletion and `Collect Garbage`. After that, content addressing, deduplication, reference tracking, path conversion, and collision mechanics are AI-delegated.

## Semantic Graph Removal Fallout

Decision owner: **Split ownership.** Mechanical removal is AI-delegated. Replacement product and GUI behavior is operator-owned.

Removing semantic graph functionality also removes several dependent surfaces.

Resolved removal scope includes:

- Dynamic graph tools and their tool registration.
- Graph upkeep hidden instructions.
- Checklists in their entirety.
- The current checklist-bound threaded-decision workflow.
- Graph-started thread creation.
- Graph link-thread menus.
- Graph diagnostics and retained-state counters.
- Any tests, docs, settings, and theme roles that exist only for graph UI.

Semantic search remains a desired future feature but is deferred until after the rework. Graph-dependent semantic-search code and its current feature design are removed or archived now. Root `doc/design.md` `# TODO` stores the future intent and any agreed provisional decisions as a non-authoritative issue-tracker substitute. A new authoritative semantic-search feature design is created only when that work is actually scheduled.

Branch, explore, and merge remains first-class. The operator still owns the graph-independent product workflow and GUI that replace the current checklist-bound threaded-decision interaction.

The removal target is deletion, not migration. AI may remove the obsolete graph-dependent surfaces automatically once the replacement boundaries above are reflected in target docs.

## Maintenance Runtime Directories

Decision owner: **AI-delegated system design.** Any visible limitation or configuration surface discovered during design is operator-owned.

The empty maintenance directory under Beryl home is straightforward for host Windows.

For WSL runtimes, decide whether Beryl creates a neutral directory inside each distro, maps a host directory into WSL, or uses host-only maintenance turns when possible.

The policy must avoid accidental project `AGENTS.md` and skill discovery while still satisfying CAS runtime requirements.

## Home Lock Implementation Details

Decision owner: **AI-delegated implementation design under the resolved one-process-per-home rule.** Busy-home GUI composition remains operator-owned.

The file-lock design needs exact rules for:

- Canonicalizing the Beryl home path before lock acquisition.
- Handling symlinks, junctions, UNC paths, and WSL-visible paths.
- Detecting stale locks after crash.
- Reporting a second-process open attempt.
- Recovering when the lock file exists but the owning process is gone.
- Ensuring lock release during normal app quit without relying only on destructors.

## Theme System And Editor Rework

Decision owner: **Operator-owned product and GUI design, followed by AI-delegated implementation.**

The theme system must continue to let themes configure every intended piece of the GUI and let styles or roles inherit from other styles or roles.

The current hierarchy of style and theme roles is not automatically preserved. The operator must redesign the target hierarchy and decide how the theme editor presents, navigates, edits, validates, previews, and explains inheritance.

After that GUI and role contract is fixed, theme resolution, inheritance validation, persistence, cache invalidation, and compatibility-free replacement mechanics are AI-delegated.

# Architecture Consequences

The next rework is architectural.

It should not be implemented as a compatibility migration that keeps workspaces alive behind renamed APIs. Workspaces, semantic graph state, and graph-derived workflows are removal candidates once the rework is active and tracked.

Removing graph-derived workflows does not remove the branch, explore, and merge product capability. The replacement must be designed directly around Syndic threads and turns without preserving checklist or graph authority behind a new name.

Old workspace-era and graph-era live code and docs should be discarded from live authority during the rework. Useful reference material may be kept only under the rework archive, following the architectural rework rules.

The durable target docs need to replace the current authority split where workspace storage owns members, selected views, graph refs, title metadata, and workspace-scoped state. The new authority split should be based on Beryl home, CAS-optional startup, runtime environments as lazy execution capabilities, execution roots, Syndic threads with committed conversation tails and current durable draft turns, immutable committed turn parentage, CAS projection bindings, window-local state, and process-wide app state.

Syndic should stay the durable conversation-history authority and own the thread/turn data model. CAS should stay the live execution and policy authority. Beryl should own GUI-local metadata, execution-root selections or bindings where target docs place them, titles, settings, themes, assets, and window/process orchestration.

# Likely Rework Tracks

These tracks are not an implementation plan yet, but they identify the major cutover areas.

1. Define the target docs for Beryl home, CAS-optional startup, runtime availability, execution roots, Syndic threads with committed conversation tails and current durable draft turns, and CAS projection forking.
2. Create a formal `doc/rework/<name>/REWORK.md` with removal-first cutover boundaries.
3. Delete or archive semantic graph and checklist docs, source, tools, tests, settings, theme roles, diagnostics, and the current checklist-bound threaded-decision workflow from live authority.
4. Replace workspace persistence with Beryl-home-level durable Fjall metadata owned by Beryl APIs.
5. Replace workspace members with CAS-optional runtime availability plus per-thread execution-root tuples.
6. Replace workspace-registered conversation views with Syndic threads, committed conversation-tail state, and exactly one current durable draft per thread.
7. Add Syndic storage APIs for revisioned compare-and-swap committed-tail and draft updates without durable Fjall exclusive locks, including dirty autosave and atomic draft submission/replacement.
8. Add crash-consistency verification for the shared Fjall home database and its keyspace invariants.
9. Rework CAS live-thread assignment so each appending Syndic thread has an exclusive CAS thread that can be reused, forked, or freshly created with Syndic-history context through the targeted existing CAS protocol.
10. Rebuild the thread selector as a recency-first catalog with runtime/root filtering, archive state, pin state, and search, without graph or workspace concepts.
11. Design and build the graph-independent branch-discussion workflow: selected reply text, a first durable draft with immutable source parentage and context provenance, first-draft `additionalContext` without pre-submission CAS execution, readonly context presentation, queue-aware dynamic-tool resolution with retryable deferral, composer gating only after successful resolution admission, durable parent-handoff queue, and archive after successful handoff.
12. Rework multi-window state so window-local and process-wide state are separated, two windows cannot open one thread, active turns are interrupted on owning-window close, and session restore distinguishes normal close from app Exit or external termination.
13. Add the Beryl home lock and startup failure/recovery UX.
14. Rework theme-role hierarchy, inheritance, and theme-editor presentation under the operator-approved GUI contract.
15. Remove or archive graph-dependent semantic-search implementation and its current feature design, stash deferred intent and provisional decisions under root `doc/design.md` `# TODO`, and defer a new authoritative feature design until semantic-search work is scheduled after this rework.
16. Discard existing workspace-era persisted live state unless a later approved one-shot import is explicitly added outside the compatibility path.
17. Replace the startup loading screen with progressively enabled conversation shells, a complete compact metadata-only catalog snapshot, bounded selector rendering, independent selected-history loading, and post-shell coalesced CAS warm-up.
18. Add exactly one durable current draft turn per Syndic thread, dirty-only tunable autosave, lifecycle-boundary flushing, atomic submit-and-replace semantics, and draft-only thread creation/reuse when an execution context would otherwise lack a thread.
19. Replace hold-to-open `New Thread` with a split button, make its secondary segment and flyout the sole runtime/root configuration entry point, provision every runtime's non-removable home root, atomically claim or create window-scoped empty threads, and add `Ctrl+Shift+N` new-window acquisition.

# Explicitly Rejected Or Deferred

- Do not support two OS processes concurrently writing the same Beryl home.
- Do not design two separate Beryl homes sharing one Syndic storage directory.
- Do not keep semantic graph functionality through adapters or compatibility layers.
- Do not keep checklist functionality or the current checklist-bound threaded-decision workflow through renamed models or compatibility layers.
- Do not keep old workspace-era persisted state live through adapters or compatibility layers.
- Do not modify Codex App Server as part of this rework; use the targeted existing protocol.
- Do not pass discussion selection or recovered Syndic history through developer instructions when the targeted CAS `additionalContext` field owns that context channel.
- Do not make CAS thread-list, CAS metadata reads, or CAS thread names catalog or title authority.
- Do not add a GUI resolve/archive command for branch discussions; resolution and handoff are initiated conversationally through the scoped dynamic tool.
- Do not create a separate context-only turn for a new branch discussion; keep immutable branch context on its first durable draft and wait for user-authored input before starting CAS or running the model.
- Do not discard, bypass, or silently absorb accepted queued turns when resolution is requested; defer the tool call and let the AI reconsider after those turns run.
- Do not accept new discussion input after resolution intent is durably accepted, and do not archive the discussion before the parent handoff succeeds.
- Do not expose branch-sibling visualization in the immediate UI unless a later product design chooses to add it.
- Do not allow two main windows to open the same Syndic thread or one Syndic thread to run multiple active turns.
- Do not implement turn/resource garbage collection in this rework; retain the future explicit `Collect Garbage` product item.
- Do not implement graph-independent semantic search during this rework; retain its intent and provisional decisions as deferred root TODO work.
- Do not keep an authoritative semantic-search feature design while the feature is deferred; stash intent and provisional decisions in root `doc/design.md` `# TODO` until the feature is scheduled.
- Do not require startup to select or initialize a runtime environment or execution root.
- Do not require CAS availability before Beryl opens the main shell and restores readable Syndic history.
- Do not show a startup loading screen before the ordinary restored conversation shells.
- Do not redesign conversation rendering, anchor-relative chunk loading, transcript retention, or transcript virtualization as part of progressive startup.
- Do not keep the thread selector inert merely because the selected transcript or CAS is still loading after its complete metadata snapshot is ready.
- Do not gate composer text editing merely on pending thread-history or CAS readiness, and do not send user data before all submission gates are ready.
- Do not keep composer drafts only in window/session metadata or mutate committed transcript turns during autosave.
- Do not leave a main window without a selected thread after a runtime/root execution context exists; create or reuse a draft-only thread.
- Do not use hold or long-press to open `New Thread` runtime/root choices; use the split button's secondary segment.
- Do not make zero-runtime primary `New Thread` open the flyout or silently change semantics; disable it, explain the missing runtime, and emphasize the secondary `...` configuration segment.
- Do not create a runtime without its non-removable user-home root or treat one empty thread as a runtime-wide singleton shared by windows.
- Do not show a newly requested additional main window until it has atomically claimed or created its own thread when a runtime exists.
- Do not automatically reuse a thread after any draft submission, including when CAS returned no response or the resulting turn is stopped, failed, disconnected, or incomplete; durable draft existence alone is not a submission.
- Do not allow a selector row for a thread open in another main window to select that thread; keep the row visible, unavailable, and explanatory through a tooltip.
- Do not eagerly construct GUI rows for the complete thread catalog; keep the metadata snapshot complete while row rendering stays bounded or virtualized.
- Do not warm every runtime represented in the thread catalog merely because its metadata was loaded.
- Do not maintain a redundant window/session restore snapshot outside Fjall; unreadable Fjall at startup is outside the restore guarantee.
