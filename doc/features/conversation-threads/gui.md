# Conversation Threads GUI

This is the normative supplemental GUI composition file for `design.md`. It owns conversation-thread mounts, layout relationships, labels, visible grouping, widget composition, and mapping of design-owned states. Product workflows, availability, activation, and failure behavior remain in `design.md`; persistence and recovery mechanics remain in the system authorities linked there.

## Thread Toolbar Controls

Mount-into: main-window.toolbar

Conversation-thread controls form an explicitly feature-local toolbar ordering rather than a toolbar widget. In order, they are one project-local [`two-segment split button`](../../gui/widgets/two-segment-split-button/spec.md), backward and forward thread-navigation command buttons, and one project-local [`thread selector trigger`](../../gui/widgets/thread-selector-trigger/spec.md). The controls leave the trailing toolbar group available for the window-level New Window, Exit, and Settings commands.

The feature configures the two-segment split button's primary label as `New Thread`, its secondary glyph as an ellipsis, and the secondary accessible name as `Choose runtime and root`. It supplies the exact commands, availability explanations, secondary attention state, and New Thread flyout association from `design.md`. The widget owns joined geometry, independent segment focus and visual states, and the stable secondary flyout anchor.

The backward and forward controls are compact icon-like `command button` widgets. The feature
supplies the trigger's selected thread and displayed title, `THREADS`
flyout label, catalog readiness, unavailable explanation, and command that opens the Thread
Switcher. The trigger widget owns its stretchable geometry, truncation, trailing affordance, focus,
loading, open, and unavailable presentation.

The toolbar does not display Workspaces or Graph controls, a static runtime prefix, a root path, or a thread-management action menu. The New Thread secondary segment and active thread selector trigger open selection flyouts rather than action menus.

## Thread Lineage Strip

Mount-into: main-window.thread-lineage

The feature mounts one project-local [`thread lineage`](../../gui/widgets/thread-lineage/spec.md) only for a selected thread with parent-thread lineage. The widget owns breadcrumb-strip anatomy, fixed-height navigation geometry, focus, unavailable/current presentation, truncation, bounded horizontal realization and scrolling, tooltip anchoring, and content-free diagnostics.

The feature supplies `LINEAGE` as the structural heading, one revision-bound logical lineage
identity, the total parent count, bounded resident breadcrumb pages with ordered stable
parent-thread identities and bounded title projections, compact chevron separators, the current
thread's readonly endpoint, each resident parent's availability explanation, and the navigation
command for available parents. It answers bounded page requests without supplying the complete
ancestor collection to the widget.

Top-level threads do not mount the widget and leave no empty spacer beneath the toolbar. A parent open elsewhere or otherwise unavailable remains represented through the widget's unavailable breadcrumb state; the feature supplies the explanatory tooltip and no activation command.

## Thread Switcher Flyout

Mount-into: main-window.overlays

The active thread selector trigger opens one project-local [`thread-root picker`](../../gui/widgets/thread-root-picker/spec.md) configured for immediate selection. For each active thread, root, or runtime collection, the feature supplies one revision-bound logical collection identity, total logical row count, bounded resident row pages, stable row identities, row presentation data, labels, status mapping, and commands. It answers bounded page requests without supplying a complete caller-unbounded collection; the widget owns its reusable flyout anatomy, focus model, collection switching, bounded rendering, and layout.

The picker header remains titled `Switch thread` in all of this feature's collection modes and uses scope-specific helper text. It has no trailing ellipsis, thread action menu, archive command, pin command, rename command, delete command, or other thread-manipulation affordance.

The feature configures the search field for the current collection and supplies plain collection headings such as `THREADS FOR ALL ROOTS`, `ROOTS FOR HOST`, and `THREADS FOR C:\Projects\Example`. It does not supply a separate scope chip, pill, rounded surface, or sort badge.

### Thread Rows

Thread rows configure the picker's leading icon as a thread spool, its primary label as the thread title, its secondary label as scope and activity metadata, and its trailing region as availability status. The spool is visually distinct from the folder icon configured for root rows.

In the all-roots list, the secondary line includes the runtime and root before its activity or occupancy state, for example `Host - C:\Projects\Example - current` or `Linux - /home/user/projects/example - open elsewhere`. When more than one configured executable runtime has the same derived Host or WSL environment label, the line inserts that thread's executable path after the environment label so those runtimes remain visibly distinguishable. In a root-scoped list, the heading already owns the full root path, so the secondary line contains only current state, activity time, or open-elsewhere state.

Path values are never rewritten into shortened aliases. When available width cannot show a complete path, the rendered line truncates at its boundary while its tooltip and accessibility text expose the complete stored path.

The current thread may carry the trailing status `OPEN`. A thread occupied by another main conversation window carries the trailing status `UNAVAILABLE`; its secondary line explains that it is open elsewhere. Status remains aligned to the trailing edge independently of title length.

Thread rows use the picker's ordinary full-row selected and focused states. They do not use checkmarks, square-bracket title decoration, folder icons, or row-edge action menus.

### Root-Browsing State

The root-browsing state defined by `design.md` maps the header command to `Back to threads`, the central collection to the selected runtime's root rows, and the runtime section heading to `RUNTIMES & ROOTS`.

Root rows configure the picker's leading icon as a folder, its primary label as the full root path, and its secondary label as `<thread count> threads - <last activity time>`. A row eligible to return to the thread list may configure its trailing region with `Choose`.

Root paths remain single-line so every row keeps the fixed collection height. Constrained paths use the full-path truncation, tooltip, and accessibility rule defined for thread metadata.

The root-scoped thread state maps the central collection to that root's thread rows and the heading to `THREADS FOR <full root path>`. The all-roots state maps the same region to all-root thread rows and the all-roots heading.

### Runtime And Root Configuration

The feature configures each runtime row with its Host or WSL environment label as the primary label. The secondary line begins with the exact configured Codex executable path and then appends compact root-count and readiness metadata. The line remains single-height and follows the same full-path truncation, tooltip, and accessibility rule as other path-bearing rows. The row also provides a `Browse roots` command or readonly `Roots shown` state and an `Add root` command. The feature configures `Add runtime` beneath the runtime collection.

These controls create or select runtimes and roots only. They do not expose thread metadata manipulation. The thread-root picker owns their vertical layout, bounded runtime viewport, focus preservation, and scrolling mechanics.

`Add runtime` maps to the platform-native file-open dialog for selecting a Codex CLI executable. `Add root` maps to the platform-native directory dialog for the exact runtime row. These are OS-owned dialogs rather than nested Beryl forms or flyouts. The feature maps the pending and failure states defined in `design.md` to the invoking command and established per-window error alert.

The feature maps search results for the current collection to picker rows and maps the completed no-match result defined in `design.md` to the picker's empty state.

The feature maps the all-roots, runtime-root, and root-scoped collections and their search text to the picker. The `thread-root picker` contract owns reusable focus, collection restoration, and scroll-preservation mechanics.

## New Thread Flyout

Mount-into: main-window.overlays

New Thread uses the same project-local `thread-root picker` as the Thread Switcher, configured for confirmed selection with root-row presentation. The feature maps the exhaustive logical recent-first root collection and runtime scope defined in `design.md` into the same revision-bound, bounded-residency presentation.

The feature configures the picker title as `New thread`, supplies helper text explaining that a root must be chosen before confirmation, and uses the collection heading `ROOTS FOR ALL RUNTIMES` or `ROOTS FOR <runtime>`.

Root rows use the same folder, full-path, and `<thread count> threads - <last activity time>` configuration as the Thread Switcher root chooser. The current root may carry the trailing status `CURRENT`. A selected root uses the picker's full-row selection state only; the feature supplies no checkmark.

The feature supplies the same runtime rows, Browse roots state, Add root commands, and Add runtime command as the Thread Switcher. It configures the optional footer with one `Confirm` command button and supplies no explanatory card, selected-thread picker, thread action menu, or secondary command group.

The picker owns the shared fixed footprint, confirmed-selection presentation, focus, collection virtualization, runtime-list layout, and external scrollbars.

The feature maps the all-runtimes or runtime-scoped root collection, current search, and the pending root selection defined in `design.md` to the picker. It maps missing selection to the disabled `Confirm` presentation and confirmation in progress to the picker's pending footer-command presentation. The `thread-root picker` contract owns reusable focus, collection restoration, and scroll-preservation mechanics.

## Thread Selector Loading Presentation

Mount-into: main-window.toolbar

Before the first coherent visible catalog rows are ready, the feature configures the same thread
selector trigger instance as loading, dimmed, and inert while the other independently available
toolbar controls preserve their own states. Initial-row readiness changes it to ready without
replacing the trigger or toolbar and without waiting for the complete catalog, transcript
readiness, or runtime readiness.
