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

- [`gui.md`](gui.md) is the normative supplemental GUI composition file for the thread toolbar
  controls, lineage, Thread Switcher, and root-selection flyouts.
- [`mockups/scoped-thread-root-lists.svg`](mockups/scoped-thread-root-lists.svg) is illustrative review material. It does not override written product behavior or GUI composition; its Change Root panel is illustrative only and is not target behavior.
- Durable thread, title, selected-path, replacement, and lineage authority is defined in
  `doc/systems/syndic-conversation-history/design.md`.
- Catalog, runtime/root registry, window-claim, mutation-reconciliation, and home-recovery mechanics
  are defined in `doc/systems/beryl-home-storage/design.md`.
- CAS projection, replacement execution, and exact soft-stop mechanics are defined in
  `doc/systems/cas-live-syndic-transcript/design.md`; bounded collection mechanics are defined in
  `doc/systems/bounded-resource-dataflow/design.md`.
- Ordinary main-window close behavior is defined in `doc/features/main-windows/design.md`.

## Product Vocabulary

- A `runtime` is one configured absolute path to a Codex CLI executable together with the Host or exact WSL environment Beryl derives from that path.
- A `root` is one configured execution directory inside one runtime.
- A `thread` is a durable Syndic conversation thread bound to one runtime and root for execution.
- Product copy uses `root` consistently. It does not use `target` as an interchangeable visible synonym for root.

## Main-Window Thread Invariant

- Each main conversation window owns independent selection, navigation history, draft presentation, transient flyout state, and visible loading or failure presentation.
- A Syndic thread may be open in at most one main conversation window at a time.
- Once at least one runtime exists, every visible main conversation window has one selected Syndic thread.
- A fresh Beryl home with zero configured runtimes is the sole state in which the initial main conversation window may be threadless.
- Two windows never share a default new thread. Each window claims or creates its own eligible thread.
- Activating an existing thread shows that thread with its already bound runtime and root.
  Activation never changes the binding.

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
- A reusable thread has never had user input submitted into its conversation history. Merely typing and then removing an unsubmitted draft does not count as submission.
- Any submission permanently disqualifies automatic reuse, including a submission whose model turn later stops, fails, disconnects, remains incomplete, or produces no assistant response.
- At claim time, the current draft contains no user-authored text, image, or other composer payload.
- The thread is ordinary, is not open in another window, and has no active turn, queued input, pending resolution, or handoff.
- When multiple eligible threads exist, selection is deterministic but is not exposed as another user choice.
- When none exists, Beryl creates a new empty thread.

## Runtime And Root Configuration

- The New Thread secondary segment is the main-shell entry point for adding or selecting runtimes and roots for new-thread creation.
- A new Beryl home has no configured runtimes by default.
- `Add runtime` opens the platform-native file-open picker. On Windows this is the native Windows file picker, and the user selects one Codex CLI executable rather than completing a Beryl-owned form.
- Beryl derives Host versus one exact WSL distribution from the selected executable path. A path outside a supported Host or WSL filesystem, a non-file path, an inaccessible executable, an incompatible Codex CLI, or a path whose environment cannot be derived is rejected without creating a runtime.
- Selecting an already configured canonical executable path resolves to that existing runtime instead of adding a duplicate runtime.
- `Add root` opens the platform-native directory picker for the exact runtime row that invoked it. On Windows this is the native Windows folder picker; a selected directory must resolve inside that runtime's derived Host or WSL environment.
- After the user chooses a runtime executable or root directory, the invoking command remains
  visible and pending while validation and admission complete. A second pointer, keyboard, or
  programmatic activation cannot start a duplicate request.
- Native-picker cancellation changes no runtime, root, or selection and returns to the unchanged
  thread/root flyout. Ordinary validation failure, pre-admission rejection, or proven durable
  noncommit restores the invoking command, preserves the prior runtime list, root list, flyout
  scope, pending selection, and current thread, and shows the established per-window error alert.
- Successfully adding a runtime also adds that runtime's user home directory as its non-removable default root.
- Runtime addition is not presented as successful unless its default home root is also available in the root list.
- Adding the first runtime creates or claims the initiating window's first thread under that runtime and home root, closes the setup flyout, and switches the conversation shell to that thread.
- Later Add runtime and Add root commands return to the same flyout with the new runtime or root represented in its exhaustive collection.
- The New Thread flyout may show roots from all runtimes or scope the same root collection to one runtime.
- Choosing a root in the New Thread flyout updates only the flyout's pending selection. `Confirm` creates or reuses and activates a thread for that root.
- Closing or dismissing the flyout before confirmation does not change the current thread or the window's remembered new-thread runtime/root.
- A successful thread activation or new-thread confirmation updates the window's remembered runtime/root. Hover, focus, pending selection, failed confirmation, and cancellation do not.
- Every New Thread flyout opening starts with all runtimes in scope, an empty search, and no pending
  root selection. `Confirm` remains visible but disabled with the ordinary missing-selection
  explanation until an eligible root is selected.
- Changing the New Thread runtime scope clears search so a query from another scope is not silently reused.
- A pending root selection survives only while that exact root remains in the current runtime
  scope. Changing scope so it is absent clears the selection and disables `Confirm`.
- While confirmation is pending, `Confirm` remains visible and busy, and duplicate confirmation is
  suppressed. Ordinary failure or proven noncommit restores the flyout with the same current thread
  and eligible pending root selection; cancellation or dismissal before acceptance changes neither
  the current thread nor the remembered runtime/root. A terminally unavailable confirmation follows
  the mutation-reconciliation behavior below instead of being presented as an ordinary failure.

## Visible Mutation Reconciliation

- While Add runtime, Add root, New Thread, thread activation, or another thread-owned durable
  mutation has an indeterminate outcome, the initiating control remains visibly reconciling and
  cannot submit a duplicate request. The current thread, transcript, remembered runtime/root,
  flyout scope, pending selection, and focus remain at their last coherent state, and the exact
  locally held mutation intent and evidence remain preserved.
- Reconciliation may prove exact success, prove noncommit, or return `Collision`, presented as
  terminal `Unavailable`, because it can prove neither. Proven success publishes all affected
  visible state together; proven noncommit restores the command with the prior visible state intact
  and reports the failure. `Unavailable` publishes no success and keeps the prior presentation only
  as coherent retained context, never as proof that the request did not commit.
- A terminally unavailable request keeps its initiating control and every action that depends on its
  result unavailable, permanently suppresses duplicate or repeat submission of that request, and
  shows a persistent bounded explanation. Only established same-home recovery and bounded
  diagnostic reporting may address or explain it; no operation-level Retry, resubmission,
  rollback, or manual repair command is exposed. Unrelated healthy catalog, thread, and window work
  remains available.
- Beryl never guesses success, combines old and new rows, or redirects the user to a substitute
  runtime, root, or thread.
- Unrelated catalog changes become visible only through a coherent refreshed collection and do not
  steal focus, activate a row, or move the user's selection merely because reconciliation completed.

## Additional Main Windows

- The main-window feature's visible `New Window` command and `Ctrl+Shift+N` accelerator create an
  additional main conversation window using both the runtime and root of the invoking window's
  selected thread.
- Beryl claims or creates the new window's own eligible thread before showing the window.
- The new window may reuse an eligible unoccupied empty thread but never the invoking window's occupied thread.
- `New Window` is disabled while zero runtimes exist. Its visible unavailable explanation directs
  the user to add a runtime through the New Thread ellipsis segment in the existing initial window.

## Thread Catalog

- The Thread Switcher presents one exhaustive Beryl-home-wide collection with the title,
  runtime/root scope, recency, availability, current-window state, and search information required
  for every thread row.
- Thread turns, transcript items, and rendered transcript bodies are not catalog rows or catalog-loading prerequisites.
- The catalog is exhaustive. `Recent-first` is its ordering policy, not a truncated recent-items mode.
- Threads whose conversation history contains only an empty current draft remain visible in the catalog.
- Catalog row labels use the one resolved Syndic title fact rather than CAS thread names or a
  Beryl-owned title candidate.
- Equal-recency ordering remains deterministic and stable while a flyout is open.
- Opening a flyout never starts CAS enumeration or transcript-history loading.
- The collection presented by an open flyout remains stable for that interaction. Background
  changes may appear coherently on a later opening but do not reorder rows under the pointer or
  keyboard focus.

## Catalog Readiness And Bounded Presentation

- The Thread Switcher stays dim and inert until its first coherent visible rows are ready. It never
  waits for every catalog row before allowing interaction with the rows already presented.
- It becomes interactive without waiting for the selected transcript or CAS readiness.
- Thread, root, and runtime collections remain responsive regardless of their total logical size and
  reveal additional rows as scrolling or keyboard navigation requires them.
- Progressive loading preserves stable row identity, selected and focused state, keyboard
  traversal, selected-row reveal, scroll position, and exact row activation.
- Search and scope changes cover the complete durable collection. The flyout shows its established
  pending treatment until coherent results are ready and reveals more results as navigation demands
  them.
- Thread and runtime search matches the exact configured executable path in addition to the existing title, environment-label, and root-path fields.
- Search does not change recent-first ordering among matching rows.
- A completed empty search result is an in-flyout empty state. It does not close the flyout, discard the query, or replace the main conversation shell.

## Thread Switcher

- Activating the toolbar's active thread selector opens the Thread Switcher flyout.
- Every opening starts with empty search and the collection of every catalog thread from every configured root, ordered by most recent activity.
- Activating an available thread row immediately requests activation of that exact thread and closes the flyout after the request is accepted.
- `Enter` activates the focused available row. `Escape` dismisses the flyout without changing the selected thread or remembered runtime/root.
- Activating the already selected thread closes the flyout without reloading the transcript or changing navigation history.
- A thread open in another main conversation window remains visible but unavailable. Its row identifies the open-elsewhere state and its hover/focus tooltip explains that one thread cannot be open in two windows.
- Unavailable rows do not activate through pointer, keyboard, or programmatic acceptance paths.
- The Thread Switcher contains only runtime creation, root creation, runtime/root browsing, and thread selection. It contains no thread metadata manipulation commands.

## Root Scoping In The Thread Switcher

- The Thread Switcher starts with the heading `THREADS FOR ALL ROOTS`.
- `Browse roots` on a runtime temporarily replaces only the central thread collection with that runtime's root collection.
- The root chooser heading is `ROOTS FOR <runtime>` and its return command is `Back to threads`.
- Activating a root immediately returns to the exhaustive recent-first thread collection scoped to that root.
- The scoped heading is `THREADS FOR <full root path>`.
- Clearing root scope returns to `THREADS FOR ALL ROOTS` without changing the collection type or ordering model.
- Search always applies to the collection and scope currently named by the heading.
- `Browse roots`, `Back to threads`, root choice, and clearing root scope each clear search so text entered for one collection is never silently applied to another.

## Visible Row Information

- Every thread row has a thread title, activity or occupancy metadata, and enough runtime/root context to identify where it executes.
- Every runtime row visibly includes its exact configured Codex executable path and derived Host or
  WSL environment label.
- In the all-roots list, each thread row includes the runtime environment label and full root path.
  When multiple configured runtimes share that environment label, the exact executable path is also
  visible in the affected thread rows.
- In a root-scoped list, the collection heading owns the full root path and rows may omit that
  repeated path.
- Current, open, unavailable, and open-elsewhere presentation remains factual. Missing metadata is omitted or shown as unknown rather than guessed.
- Every root row shows its full path, `<thread count> threads`, and `<last activity time>`.

## Thread Titles

- A title is an intrinsic Syndic thread property. Display-title precedence is the accepted generated
  title, the Syndic history-derived title, then the localized untitled fallback. Every thread
  surface shows the same resolved title rather than choosing precedence independently.
- CAS thread names and metadata are never display-title authority.
- An ordinary thread becomes eligible for automatic title generation after its first real user-authored input is durably captured in Syndic and it has no accepted generated title.
- Automatic title generation is background maintenance: it never appears as a catalog thread or
  transcript turn, never interrupts foreground use, and does not use the user's global developer
  instructions.
- A valid generated title appears through one later coherent catalog refresh. Failure leaves the
  current lower-precedence visible title intact and does not gate submission, activation, or
  transcript reading.
- An accepted generated title is one nonempty logical line, contains at least one alphanumeric
  character, has no Unicode control character or surrounding whitespace, and is at most 512 UTF-8
  bytes. Validation rejects rather than truncates model output.

## History-Derived Title

- The source is the earliest real user-authored input on the thread's current selected path. For a
  branch-discussion thread, inherited parent history and the synthetic discussion-context item are
  excluded; the source is the earliest branch-local real user input. A draft, assistant output,
  provider-operation turn, lifecycle continuation, image marker, or CAS metadata is never a source.
- Beryl examines at most the first 4,096 logical UTF-8 bytes of that input, considers logical lines
  in order, collapses each Unicode-whitespace run to one ASCII space, discards other Unicode control
  characters, and selects the first nonempty normalized line containing an alphanumeric character.
- The derived title preserves the selected line's spelling and case and ends at the earlier of 80
  Unicode scalar values or 512 UTF-8 bytes on a valid boundary, with trailing whitespace removed.
  It adds no ellipsis or other invented text. If no eligible line exists inside the scan bound,
  the history-derived candidate is absent.
- Replacement editing or another selected-path change invalidates and rebuilds the derived
  candidate when its exact source witness changes. A generated title, once accepted, remains the
  higher-precedence title and is not replaced by rebuilding the fallback.

## Catalog Search Text

- Search is locale-independent and uses the Unicode `NFKC_Casefold` mapping defined by Unicode
  Default Caseless Matching for both indexed fields and query input. It matches title, runtime
  environment label, configured executable path, and full root path without changing their visible
  authoritative spelling.
- A nonempty normalized query matches a contiguous normalized substring of any searchable field.
  An empty normalized query preserves the current scope and recent-first ordering. Visible text is
  always the original authoritative text, never the normalized search key.

## Current Management Boundary

- Runtime and root registries are additive. Beryl exposes no runtime-removal or root-removal command.
- A thread's runtime/root execution binding is immutable. Unavailable runtime, root, or CAS state leaves history readable and the draft preserved but does not expose Change Root or another rebind command.
- Beryl exposes no manual thread rename, pin, archive, or delete command.
- Successful branch-discussion handoff may still archive that discussion automatically according to the branch-discussion contract. This system-owned transition is not a general thread-management command.
- Beryl performs no automatic empty-thread cleanup. A pristine draft-only thread remains visible until it is claimed for reuse.

## Replacement Editing

- `Edit message` lets the user replace one historical user-input turn on the selected thread's current path without mutating that historical turn or its descendants.
- The action originates from the exact user-input turn's transcript context menu and remains visible but disabled when its closest actionable gate can be explained.
- Editing requires an idle selected thread, no accepted or queued input, no repair-pending turn, no
  compaction, activation, resolution, or handoff work, an empty composer, and an exactly replaceable
  selected-path message whose text and images remain available.
- Starting edit mode fills the composer with an editable copy of the target input. It closes the context menu and does not change the visible conversation history.
- The target turn and its later turns on the selected path are dimmed while edit mode is active, but they remain readable, selectable, copyable, quoteable, and scrollable.
- `Escape` cancels edit mode after higher-priority popups handle the key. Cancellation removes the dimming and exits replacement mode but preserves the edited composer content, caret, selection, image markers, and undo history.
- Submitting in edit mode validates the edited input and exact target before changing history. It is
  subject to the same once-per-attempt free-space admission rule as ordinary composer submission. A
  below-reserve, unavailable, or indeterminate result leaves edit mode and the exact edited input
  intact and starts no model work.
- Accepted replacement submission creates a new path from immediately before the edited message,
  selects that path, and leaves the original path available as immutable history.
- The visible selected path changes to the replacement path as one coherent commit. Filesystem changes, settings, assets, activity records, other threads, and external effects from the old tail are not rolled back.
- If replacement admission itself has an indeterminate durable outcome, the last coherent selected
  path and editor remain visible but reconciling, the exact edited input, target, and locally held
  evidence remain intact, and repeat submission is unavailable. Proven success publishes the
  replacement path once; proven noncommit restores edit mode with the original path and exact
  edited input intact.
- A replacement `Unavailable` outcome publishes no path change and does not restore edit mode as if
  noncommit were proved. It retains the last coherent path and exact local edit intent, keeps repeat
  replacement and dependent selected-path mutations unavailable, and uses the persistent
  recovery/diagnostic explanation defined under Visible Mutation Reconciliation. Other healthy
  threads remain usable.
- Delivery failure, disconnection, interruption, crash, or stop after durable acceptance leaves the replacement turn on the selected path with its exact incomplete or failed state; Beryl never silently restores the old tail or reports the edit as absent.
- If exact replacement is unavailable, Beryl disables the action rather than approximating a history position or changing unrelated history.

## Thread Navigation History

- Backward and forward toolbar commands navigate exact threads previously activated in that main conversation window.
- Each main window's backward and forward history is bounded. When it fills, the oldest eligible
  navigation entry expires; closing the window clears that window's navigation history.
- Successful user-initiated activation from the Thread Switcher, lineage breadcrumbs, transcript thread links, and backward or forward navigation updates history.
- Failed, cancelled, already-selected, restore-time, background-only, or pristine-thread acquisition does not add a navigation-history entry.
- Activating a new thread after navigating backward clears the forward history.
- Backward and forward controls remain visible when unavailable and explain the unavailable reason through their disabled tooltip.
- Navigation never substitutes another runtime, root, or thread when an exact recorded thread cannot be activated.

## Thread Lineage

- A selected thread with parent-thread lineage exposes breadcrumbs ordered from the top-level
  ancestor toward the current thread.
- The lineage represents the complete ancestor chain. Scrolling or keyboard navigation
  progressively reveals additional ancestors without losing the focused breadcrumb or changing the
  requested parent identity.
- Activating an available parent breadcrumb requests activation of that exact thread through the ordinary activation path.
- A missing, unavailable, or open-elsewhere parent remains represented but does not silently redirect to another thread.
- A top-level thread exposes no lineage breadcrumbs.
- The immediate Thread Switcher remains a flat recent-first catalog; lineage does not turn it into a branch tree.

## Thread Activation Presentation

- An accepted activation keeps the previous coherent transcript visible until the requested thread's initial transcript view is ready.
- While that replacement is pending, the prior transcript is dim and inert rather than replaced by a loading screen or temporary transcript message.
- The requested thread's transcript content and initial viewport state become visible together.
- The requested transcript opens at its intended initial scroll position without a later visible jump.
- Successful activation applies the active thread title, lineage, transcript, draft, and remembered runtime/root coherently.
- Ordinary failure, rejection, pre-admission cancellation, or proven activation noncommit restores
  the prior selector state and coherent transcript, leaves navigation history unchanged, and
  reports the established per-window error alert. A terminally unavailable activation instead
  follows Visible Mutation Reconciliation and is never restored as a proven noncommit or exposed
  for repeat activation.
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
- Opening either thread/root flyout moves focus into its search field. Dismissing the flyout returns
  focus to its trigger; collection changes preserve a coherent focused row or return focus to search
  without committing a selection.
- Keyboard focus and selected-row state are distinct. Moving focus does not commit a thread or root selection unless the user activates the focused row.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers:

- `external-side-effects/v1`
