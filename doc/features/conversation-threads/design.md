# Goals

Let users create, select, and navigate durable Syndic conversation threads across independent Beryl windows without requiring workspace or semantic-graph navigation.

Keep thread selection responsive across a large Beryl home while preserving exact runtime, root, thread, window-occupancy, and user-intent boundaries.

## Non-goals

- Treating a CAS thread inventory, backend working-directory list, workspace member hierarchy, or semantic graph as thread-catalog authority.
- Showing branch siblings or the full Syndic turn DAG in the immediate thread selector.
- Exposing manual rename, pin, archive, delete, runtime removal, root removal, or thread-rebinding commands.
- Changing transcript chunk loading, transcript virtualization, or transcript viewport anchoring as part of thread navigation.
- Rebinding an existing thread to a different runtime or root.

# Decisions

## Supplemental Material

- `gui.md` is the normative supplemental GUI composition file for mounting and configuring the thread toolbar controls, project-local `thread lineage` and `thread selector trigger` widgets, Thread Switcher, and root-selection flyouts.
- [`mockups/scoped-thread-root-lists.svg`](mockups/scoped-thread-root-lists.svg) is illustrative review material. It does not override written product behavior or GUI composition; its Change Root panel is illustrative only and is not target behavior.

## Product Vocabulary

- A `runtime` is one configured absolute path to a Codex CLI executable together with the Host or exact WSL environment Beryl derives from that path.
- A `root` is one configured execution directory inside one runtime.
- A `thread` is a durable Syndic conversation thread bound to one runtime and root for execution.
- Every thread owns one committed conversation tail when submitted history exists and exactly one current durable draft.
- Product copy uses `root` consistently. It does not use `target` as an interchangeable visible synonym for root.
- `Runtime target` may remain an internal backend term when it identifies backend process ownership rather than a visible execution root.

## Main-Window Thread Invariant

- Each main conversation window owns independent selection, navigation history, draft presentation, transient flyout state, and visible loading or failure presentation.
- A Syndic thread may be open in at most one main conversation window at a time.
- Once at least one runtime exists, every visible main conversation window has one selected Syndic thread.
- A fresh Beryl home with zero configured runtimes is the sole state in which the initial main conversation window may be threadless.
- Two windows never share a default new thread. Each window claims or creates its own eligible thread.
- Closing a main conversation window with an active turn interrupts that turn before releasing the window's thread claim.
- Activating an existing thread adopts that thread's bound runtime and root. Activation never silently changes the thread's binding.

## New Thread Split Button

- New Thread is a split button with a primary text segment labeled `New Thread` and a compact secondary ellipsis segment.
- Activating the primary segment creates or reuses an eligible empty thread in the selected thread's current runtime and root, claims it for the current window, and switches to it.
- If the current window already owns an eligible pristine empty thread, primary New Thread is a no-op that remains on that thread.
- Activating the secondary segment opens the New Thread runtime/root flyout.
- Holding or long-pressing the primary segment does not open the flyout or invoke another command.
- The secondary segment has the accessible name `Choose runtime and root`.
- The two segments are independently keyboard-focusable. `Enter` and `Space` activate the focused segment.
- With zero configured runtimes, the primary segment remains visible but disabled. Its tooltip says `Add a runtime with the … button before creating a thread.`
- With zero configured runtimes, the enabled secondary segment receives the theme's attention-drawing secondary treatment. It returns to its ordinary presentation after a runtime exists.

## Eligible Empty Thread Reuse

- Automatic reuse is scoped to the selected runtime and root and may claim only a thread not occupied or reserved by another window.
- A reusable thread has never had a draft submitted into conversation history. A durable current draft existing by itself does not count as submission.
- Any submission permanently disqualifies automatic reuse, including a submission whose model turn later stops, fails, disconnects, remains incomplete, or produces no assistant response.
- At claim time, the current draft contains no user-authored text, image, or other composer payload.
- The thread is ordinary and has no branch-discussion binding, active turn, queued input, pending resolution or handoff, or window claim.
- When multiple eligible threads exist, selection is deterministic but is not exposed as another user choice.
- When none exists, Beryl creates a new durable thread with an empty current draft.

## Runtime And Root Configuration

- The New Thread secondary segment is the main-shell entry point for adding or selecting runtimes and roots for new-thread creation.
- A new Beryl home has no configured runtimes by default.
- `Add runtime` opens the platform-native file-open picker. On Windows this is the native Windows file picker, and the user selects one Codex CLI executable rather than completing a Beryl-owned form.
- Beryl derives Host versus one exact WSL distribution from the selected executable path. A path outside a supported Host or WSL filesystem, a non-file path, an inaccessible executable, an incompatible Codex CLI, or a path whose environment cannot be derived is rejected without creating a runtime.
- Selecting an already configured canonical executable path resolves to that existing runtime instead of creating a duplicate runtime record.
- `Add root` opens the platform-native directory picker for the exact runtime row that invoked it. On Windows this is the native Windows folder picker; a selected directory must resolve inside that runtime's derived Host or WSL environment.
- Native-picker cancellation returns to the unchanged thread/root flyout. Validation failure likewise preserves the prior runtime registry, root registry, pending selection, and current thread and uses the established per-window error alert.
- Successfully adding a runtime also adds that runtime's user home directory as its non-removable default root.
- Runtime addition is not presented as successful unless its default home root is also available in the root list.
- Adding the first runtime creates or claims the initiating window's first thread under that runtime and home root, closes the setup flyout, and switches the conversation shell to that thread.
- Later Add runtime and Add root commands return to the same flyout with the new runtime or root represented in its exhaustive collection.
- The New Thread flyout may show roots from all runtimes or scope the same root collection to one runtime.
- Choosing a root in the New Thread flyout updates only the flyout's pending selection. `Confirm` creates or reuses and activates a thread for that root.
- Closing or dismissing the flyout before confirmation does not change the current thread or the window's remembered new-thread runtime/root.
- A successful thread activation or new-thread confirmation updates the window's remembered runtime/root. Hover, focus, pending selection, failed confirmation, and cancellation do not.

## Additional Main Windows

- `Ctrl+Shift+N` creates an additional main conversation window using both the runtime and root of the invoking window's selected thread.
- Beryl claims or creates the new window's own eligible thread before showing the window.
- The new window may reuse an eligible unoccupied empty thread but never the invoking window's occupied thread.
- `Ctrl+Shift+N` is disabled while zero runtimes exist. Its unavailable explanation directs the user to add a runtime through the New Thread ellipsis segment in the existing initial window.

## Thread Catalog

- The Thread Switcher reads from one Beryl-home-wide catalog of Syndic threads joined with Beryl-owned presentation metadata.
- The catalog contains all thread metadata required for row labels, runtime and root scope, recency ordering, availability, current-window state, search, and filtering.
- Thread turns, transcript items, and rendered transcript bodies are not catalog rows or catalog-loading prerequisites.
- The catalog is exhaustive. `Recent-first` is its ordering policy, not a truncated recent-items mode.
- Threads whose conversation history contains only an empty current draft remain visible in the catalog.
- Catalog row labels use Beryl-owned title metadata and Syndic-derived title facts rather than CAS thread names.
- Equal-recency ordering remains deterministic and stable while a flyout is open.
- Opening a flyout never starts CAS enumeration or transcript-history loading.
- A catalog snapshot presented by an open flyout remains stable for that flyout interaction. Background refresh may update the next opened projection but does not reorder rows under the pointer or keyboard focus.

## Catalog Readiness And Bounded Presentation

- The Thread Switcher stays dim and inert until one complete compact metadata snapshot is ready.
- It becomes interactive without waiting for the selected transcript or CAS readiness.
- Thread, root, and runtime collections use fixed-height virtualized rows whenever their complete collection can exceed its bounded viewport. Total catalog or runtime-registry size does not determine live render-tree size.
- Virtualization preserves stable row identity, selected and focused state, keyboard traversal, selected-row reveal, scroll position, exact row activation, and intentional tooltip dismissal when an anchor row leaves the rendered range.
- Search and scope changes operate over the complete compact catalog while keeping rendered rows bounded.
- Thread and runtime search matches the exact configured executable path in addition to the existing title, environment-label, and root-path fields.
- Search does not change recent-first ordering among matching rows.
- An empty search result is an in-flyout empty state. It does not close the flyout, discard the query, or replace the main conversation shell.

## Thread Switcher

- Activating the toolbar's active thread selector opens the Thread Switcher flyout.
- The default collection contains every catalog thread from every configured root, ordered by most recent activity.
- Activating an available thread row immediately requests activation of that exact thread and closes the flyout after the request is accepted.
- `Enter` activates the focused available row. `Escape` dismisses the flyout without changing the selected thread or remembered runtime/root.
- Activating the already selected thread closes the flyout without reloading the transcript or changing navigation history.
- A thread open in another main conversation window remains visible but unavailable. Its row identifies the open-elsewhere state and its hover/focus tooltip explains that one thread cannot be open in two windows.
- Unavailable rows do not activate through pointer, keyboard, or programmatic acceptance paths.
- The Thread Switcher contains only runtime creation, root creation, runtime/root browsing, and thread selection. It contains no thread metadata manipulation commands.

## Root Scoping In The Thread Switcher

- The Thread Switcher starts with the heading `THREADS FOR ALL ROOTS`.
- `Browse roots` on a runtime temporarily replaces only the central thread collection with that runtime's root collection.
- The flyout header, search placement, and runtime/root section remain in place while choosing a root.
- The root chooser heading is `ROOTS FOR <runtime>` and its return command is `Back to threads`.
- Activating a root immediately returns to the exhaustive recent-first thread collection scoped to that root.
- The scoped heading is `THREADS FOR <full root path>`.
- Clearing root scope returns to `THREADS FOR ALL ROOTS` without changing the collection type or ordering model.
- Search always applies to the collection and scope currently named by the heading.

## Visible Row Information

- Every thread row has a thread title, activity or occupancy metadata, and enough runtime/root context to identify where it executes.
- Every runtime row visibly includes its exact configured Codex executable path beneath the derived Host or WSL environment label.
- In the all-roots list, each thread row includes the runtime environment label and full root path on its secondary line. When multiple configured runtimes share that environment label, the exact executable path is also visible in the affected thread rows.
- In a root-scoped list, the collection heading owns the full root path and the row secondary line may omit that repeated path.
- Current, open, unavailable, and open-elsewhere presentation remains factual. Missing metadata is omitted or shown as unknown rather than guessed.
- Every root row shows its full path on the primary line and `<thread count> threads - <last activity time>` on the secondary line.
- Thread status appears at the trailing row edge. Selection is shown by the selected-row visual state, not a checkmark.

## Thread Titles

- Display-title precedence is generated Beryl title, Syndic history-derived title summary, then an untitled fallback.
- CAS thread names and metadata are never display-title authority.
- An ordinary thread becomes eligible for automatic title generation after its first real user-authored input is durably captured in Syndic and it has no accepted generated title.
- Automatic title generation runs as bounded background maintenance through a fresh ephemeral CAS thread with fixed Beryl instructions and medium reasoning. It does not use the selected thread's foreground stream, inject global developer instructions, or expose its maintenance thread in the catalog or transcript.
- Successful output is validated, committed as generated Beryl metadata, and published through the next catalog revision. Failure leaves the current lower-precedence title source intact and may retry only through bounded maintenance scheduling.
- Background title cleanup and failure never gate foreground submission, selected-thread activation, or transcript reading.

## Current Management Boundary

- Runtime and root registries are additive. Beryl exposes no runtime-removal or root-removal command.
- A thread's runtime/root execution binding is immutable. Unavailable runtime, root, or CAS state leaves history readable and the draft preserved but does not expose Change Root or another rebind command.
- Beryl exposes no manual thread rename, pin, archive, or delete command.
- Successful branch-discussion handoff may still archive that discussion automatically according to the branch-discussion contract. This system-owned transition is not a general thread-management command.
- Beryl performs no automatic empty-thread cleanup. A pristine draft-only thread remains visible until it is claimed for reuse.

## Replacement Editing

- `Edit message` lets the user replace one historical user-input turn on the selected thread's current path without mutating that historical turn or its descendants.
- The action originates from the exact user-input turn's transcript context menu and remains visible but disabled when its closest actionable gate can be explained.
- Editing requires an idle selected thread, no accepted or queued input, no compaction, activation, resolution, or handoff work, an empty current draft, exact resident Syndic provenance for the target and selected tail, reconstructable input and image references, and a provable CAS rollback or fresh-recovery path.
- Starting edit mode durably attaches the exact replacement target to the current draft and fills that draft with an editable copy of the target input. It closes the context menu and does not mutate the committed tail.
- The target turn and its later turns on the selected path are dimmed while edit mode is active, but they remain readable, selectable, copyable, quoteable, and scrollable.
- `Escape` cancels edit mode after higher-priority popups handle the key. Cancellation removes the durable replacement target and dimming but preserves the current draft content, caret, selection, image markers, and undo history.
- Submitting in edit mode validates the draft and exact replacement proof before committing any path change. Validation failure leaves edit mode and the draft intact.
- Accepted replacement submission creates a new durable turn from the edited turn's parent, moves only the selected thread's committed tail and replacement current-draft binding to the new path, and leaves the original tail immutable and durable.
- The visible selected path changes to the replacement path as one coherent commit. Filesystem changes, settings, assets, activity records, other threads, and external effects from the old tail are not rolled back.
- Delivery failure, disconnection, interruption, crash, or stop after durable acceptance leaves the replacement turn on the selected path with its exact incomplete or failed state; Beryl never silently restores the old tail or reports the edit as absent.
- If exact backend rollback or fresh-recovery proof is unavailable, Beryl disables replacement editing rather than copying CAS history, rewriting Syndic parentage, or approximating a rollback count.

## Thread Navigation History

- Backward and forward toolbar commands navigate exact threads previously activated in that main conversation window.
- Each main window owns its own in-memory backward and forward history.
- Successful user-initiated activation from the Thread Switcher, lineage breadcrumbs, transcript thread links, and backward or forward navigation updates history.
- Failed, cancelled, already-selected, restore-time, background-only, or pristine-thread acquisition does not add a navigation-history entry.
- Activating a new thread after navigating backward clears the forward history.
- Backward and forward controls remain visible when unavailable and explain the unavailable reason through their disabled tooltip.
- Navigation never substitutes another runtime, root, or thread when an exact recorded thread cannot be activated.

## Thread Lineage

- A selected thread with parent-thread lineage shows a lineage strip directly below the toolbar.
- The strip orders parent-thread breadcrumbs from the top-level ancestor toward the current thread.
- Activating an available parent breadcrumb requests activation of that exact thread through the ordinary activation path.
- A missing, unavailable, or open-elsewhere parent remains represented but does not silently redirect to another thread.
- A top-level thread has no lineage strip and does not reserve empty space for one.
- The immediate Thread Switcher remains a flat recent-first catalog; lineage does not turn it into a branch tree.

## Thread Activation Presentation

- An accepted activation keeps the previous coherent transcript visible until the requested thread's initial anchor-relative transcript chunk is ready.
- While that replacement is pending, the prior transcript is dim and inert rather than replaced by a loading screen or temporary transcript message.
- The requested thread's transcript content and initial viewport state become visible together.
- Activation must not visibly correct its initial scroll position in a later render callback.
- Successful activation applies the active thread title, lineage, transcript, draft, and remembered runtime/root coherently.
- Failed, rejected, cancelled, or stopped activation restores the prior selector state and coherent transcript, leaves navigation history unchanged, and reports the established per-window error alert.
- Thread activation never waits for CAS merely to browse complete Syndic history. CAS readiness gates submission and other CAS-backed operations for the activated thread.

## Progressive Shell Readiness

- The first visible surface is the ordinary main conversation shell; Beryl does not show a separate startup loading screen.
- Thread-catalog loading, selected-thread history loading, and CAS warm-up proceed independently.
- The thread selector is dim and inert only until catalog readiness.
- The transcript viewport is dim and inert only until the selected thread's initial history chunk is ready.
- Composer text entry remains available while the catalog, transcript, root, or CAS is loading when the current draft itself is available and writable.
- Submission remains unavailable and sends no user data until the selected thread, its current draft, its runtime/root binding, and CAS are ready.
- Each main window presents shared runtime or storage readiness through its own shell controls and error alerts.

## Runtime, Root, And CAS Unavailability

- A thread remains visible and its durable Syndic history remains browsable when its runtime, root, or CAS is unavailable.
- Composer text editing and draft preservation remain available when local durable state is healthy, even if submission is unavailable.
- Commands that require the unavailable runtime, root, or CAS remain visible but disabled and explain the closest blocking condition.
- A CAS launch failure uses the persistent per-window backend-unavailable notice with Retry defined by `doc/features/backend-runtime-recovery/design.md`; it does not remove the selected thread or replace its transcript with a loading failure.
- Root unavailability includes a missing path, inaccessible path, non-directory path, permission failure, or unavailable WSL distribution.
- Add runtime, Add root, New Thread confirmation, and thread activation report failure without partially changing the visible selection.

## Accessibility And Focus

- Every icon-only or symbol-only command has an accessible name independent of its glyph.
- Thread and root rows expose their complete title or path, runtime/root context, activity, and availability status to accessibility output even when visible text is truncated.
- Disabled commands and unavailable selector rows expose a hover/focus tooltip with the closest actionable reason.
- Opening either thread/root flyout moves focus into its search field. Collection and scope transitions use the reset and per-collection restoration rules in `gui.md`; dismissing the flyout returns focus to its trigger.
- Keyboard focus and selected-row state are distinct. Moving focus does not commit a thread or root selection unless the user activates the focused row.
