# Conversation Threads GUI

This is the normative supplemental GUI composition file for `design.md`. It owns conversation-thread mounts, layout relationships, visible grouping, and widget composition. Product workflows, persistence, availability rules, activation semantics, and failure behavior remain in `design.md`.

## Thread Toolbar Controls

Mount-into: main-window.toolbar

Conversation-thread controls form an explicitly feature-local toolbar ordering rather than a toolbar widget. In order, they are the New Thread split button, backward and forward thread-navigation command buttons, and one project-local [`thread selector trigger`](../../gui/widgets/thread-selector-trigger/spec.md). The controls leave the trailing toolbar group available for window-level Exit and Settings commands.

The New Thread split button is a feature-local joined arrangement of two `command button` widgets. It remains feature-local because it is used once and adds no focus, activation, or state model beyond those buttons. Its text-labeled primary segment and compact secondary ellipsis segment share one outer silhouette without a gap, meet at one internal divider, and read as one control while retaining independent enabled, hover, pressed, focused, and disabled states. The secondary segment's accessible name is `Choose runtime and root`.

The backward and forward controls are compact icon-like `command button` widgets. The feature
supplies the trigger's selected-thread identity and bounded Beryl catalog title copy, `THREADS`
flyout label, catalog readiness, unavailable explanation, and command that opens the Thread
Switcher. The trigger widget owns its stretchable geometry, truncation, trailing affordance, focus,
loading, open, and unavailable presentation.

The toolbar does not display Workspaces or Graph controls, a static runtime prefix, a root path, or a thread-management action menu. The New Thread secondary segment and active thread selector trigger open selection flyouts rather than action menus.

## Thread Lineage Strip

Mount-into: main-window.thread-lineage

The feature mounts one project-local [`thread lineage`](../../gui/widgets/thread-lineage/spec.md) only for a selected thread with parent-thread lineage. The widget owns breadcrumb-strip anatomy, fixed-height navigation geometry, focus, unavailable/current presentation, truncation, bounded horizontal realization and scrolling, tooltip anchoring, and content-free diagnostics.

The feature supplies `LINEAGE` as the structural heading, one revision-bound lineage query identity,
the total parent count, bounded resident breadcrumb pages with ordered stable parent-thread
identities and bounded title projections, compact chevron separators, the current thread's readonly
endpoint, each resident parent's exact availability reason, and the exact navigation command for
available parents. It answers deduplicated page requests without constructing the complete ancestor
chain in GUI memory.

Top-level threads do not mount the widget and leave no empty spacer beneath the toolbar. A parent open elsewhere or otherwise unavailable remains represented through the widget's unavailable breadcrumb state; the feature supplies the explanatory tooltip and no activation command.

## Thread Switcher Flyout

Mount-into: main-window.overlays

The active thread selector trigger opens one project-local [`thread-root picker`](../../gui/widgets/thread-root-picker/spec.md) configured for immediate selection. The feature supplies revision-bound thread, root, and runtime query identities, total counts, bounded resident row pages, row presentation data, labels, commands, and activation effects; the widget owns its reusable flyout anatomy, focus model, stable collection switching, bounded rendering, and layout.

The picker header remains titled `Switch thread` in all of this feature's collection modes and uses scope-specific helper text. It has no trailing ellipsis, thread action menu, archive command, pin command, rename command, delete command, or other thread-manipulation affordance.

The feature configures the search field for the current collection and supplies plain collection headings such as `THREADS FOR ALL ROOTS`, `ROOTS FOR HOST`, and `THREADS FOR C:\Users\user\p\beryl`. It does not supply a separate scope chip, pill, rounded surface, or sort badge.

### Thread Rows

Thread rows configure the picker's leading icon as a thread spool, its primary label as the thread title, its secondary label as scope and activity metadata, and its trailing region as availability status. The spool is visually distinct from the folder icon configured for root rows.

In the all-roots list, the secondary line includes the runtime and root before its activity or occupancy state, for example `Host - C:\Users\user\p\beryl - current` or `OL9 - /home/user/p/beryl - open elsewhere`. When more than one configured executable runtime has the same derived Host or WSL environment label, the line inserts that thread's executable path after the environment label so those runtimes remain visibly distinguishable. In a root-scoped list, the heading already owns the full root path, so the secondary line contains only current state, activity time, or open-elsewhere state.

Path values are never rewritten into shortened aliases. When available width cannot show a complete path, the rendered line truncates at its boundary while its tooltip and accessibility text expose the complete stored path.

The current thread may carry the trailing status `OPEN`. A thread occupied by another main conversation window carries the trailing status `UNAVAILABLE`; its secondary line explains that it is open elsewhere. Status remains aligned to the trailing edge independently of title length.

Thread rows use the picker's ordinary full-row selected and focused states. They do not use checkmarks, square-bracket title decoration, folder icons, or row-edge action menus.

### Root-Browsing State

Activating `Browse roots` for one runtime preserves the flyout header and `RUNTIMES & ROOTS` section while replacing only the central thread collection with that runtime's root collection. The header command reads `Back to threads`.

Root rows configure the picker's leading icon as a folder, its primary label as the full root path, and its secondary label as `<thread count> threads - <last activity time>`. A row eligible to return to the thread list may configure its trailing region with `Choose`.

Root paths remain single-line so every row keeps the fixed collection height. Constrained paths use the full-path truncation, tooltip, and accessibility rule defined for thread metadata.

Choosing a root returns the central collection to the same exhaustive recent-first thread list scoped to that root. The collection heading becomes `THREADS FOR <full root path>`. Returning to all roots removes the root scope without changing the collection type or its ordering model.

### Runtime And Root Configuration

The feature configures each runtime row with its Host or WSL environment label as the primary label. The secondary line begins with the exact configured Codex executable path and then appends compact root-count and readiness metadata. The line remains single-height and follows the same full-path truncation, tooltip, and accessibility rule as other path-bearing rows. The row also provides a `Browse roots` command or readonly `Roots shown` state and an `Add root` command. The feature configures `Add runtime` beneath the runtime collection.

These controls create or select runtimes and roots only. They do not expose thread metadata manipulation. The thread-root picker owns their vertical layout, bounded runtime viewport, focus preservation, and scrolling mechanics.

`Add runtime` invokes the platform-native file-open dialog for selecting a Codex CLI executable. `Add root` invokes the platform-native directory dialog for the exact runtime row. These are OS-owned dialogs rather than nested Beryl forms or flyouts; while one is open, the invoking thread-root picker remains unchanged behind it. After selection, the invoking command remains visible and pending while validation and durable admission run, with duplicate activation suppressed. Cancellation or failure returns focus to that command without changing picker scope or selection.

Search starts a new revision-bound query over the feature's current exhaustive logical collection
without changing its scope or recent-first ordering. The feature supplies bounded result pages and
the in-viewport empty result only after the query proves that no row matches.

Every Thread Switcher opening starts in the all-roots thread collection with empty search and moves focus into the search field, regardless of pointer or keyboard invocation. `Browse roots`, `Back to threads`, root choice, and removal of a root filter each clear search and return focus to the search field so text for one collection is never silently applied to another.

The feature supplies a distinct stable collection key for the all-roots thread list, each runtime's root list, and each root-scoped thread list. Within one open flyout, returning to a previously visited key restores its row focus memory and scroll position without activating a row; closing and reopening starts from that collection's recent-first top.

## New Thread Flyout

Mount-into: main-window.overlays

New Thread uses the same project-local `thread-root picker` as the Thread Switcher, configured for confirmed selection with root-row presentation. Its collection remains an exhaustive logical recent-first root query backed by bounded pages; runtime selection changes scope rather than changing the collection type.

The feature configures the picker title as `New thread`, supplies helper text explaining that a root must be chosen before confirmation, and uses the collection heading `ROOTS FOR ALL RUNTIMES` or `ROOTS FOR <runtime>`.

Root rows use the same folder, full-path, and `<thread count> threads - <last activity time>` configuration as the Thread Switcher root chooser. The current root may carry the trailing status `CURRENT`. A selected root uses the picker's full-row selection state only; the feature supplies no checkmark.

The feature supplies the same runtime rows, Browse roots state, Add root commands, and Add runtime command as the Thread Switcher. It configures the optional footer with one `Confirm` command button and supplies no explanatory card, selected-thread picker, thread action menu, or secondary command group.

The picker owns the shared fixed footprint, confirmed-selection state, focus, collection virtualization, runtime-list layout, external scrollbars, and duplicate-confirmation prevention.

Every New Thread flyout opening starts with the all-runtimes root collection, empty search, no pending root selection, the recent-first top visible, and focus in the search field. Runtime scoping clears search and returns focus to the search field. The all-runtimes root collection and each runtime-scoped root collection use distinct stable keys; within one open flyout, returning to a key restores its row-focus memory and scroll position without activating a row, while closing and reopening resets to the all-runtimes recent-first top. A pending root selection survives only while that exact root remains a member of the visible runtime scope; otherwise the feature clears it and disables `Confirm` with the ordinary missing-selection explanation.

## Thread Selector Loading Presentation

Mount-into: main-window.toolbar

Before the first coherent page of the revision-bound default catalog query is ready, the feature
configures the same thread selector trigger instance as loading, dimmed, and inert while the other
independently available toolbar controls preserve their own states. First-page readiness changes it
to ready without replacing the trigger or toolbar and without waiting for all catalog pages,
transcript readiness, or runtime readiness.
