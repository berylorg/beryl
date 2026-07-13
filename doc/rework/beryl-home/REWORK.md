# Target Docs

Checkpoint 0 target authority:

- `doc/design.md`
- `doc/gui/integration.md`
- `doc/gui/external-specs.md`
- `doc/gui/widgets/activity-panel/spec.md`
- `doc/gui/widgets/code-panel/spec.md`
- `doc/gui/widgets/contracts/beryl-command-geometry.md`
- `doc/gui/widgets/contracts/expected-action-availability.md`
- `doc/gui/widgets/contracts/scroll-ownership.md`
- `doc/gui/widgets/conversation-composer/spec.md`
- `doc/gui/widgets/image-marker/spec.md`
- `doc/gui/widgets/image-preview/spec.md`
- `doc/gui/widgets/main-window-notice/spec.md`
- `doc/gui/widgets/table-panel/spec.md`
- `doc/gui/widgets/theme-editor/spec.md`
- `doc/gui/widgets/thread-lineage/spec.md`
- `doc/gui/widgets/thread-root-picker/spec.md`
- `doc/gui/widgets/thread-selector-trigger/spec.md`
- `doc/gui/widgets/transcript-view/spec.md`
- `doc/features/beryl-home/design.md`
- `doc/features/beryl-home/gui.md`
- `doc/features/main-windows/design.md`
- `doc/features/main-windows/gui.md`
- `doc/features/conversation-threads/design.md`
- `doc/features/conversation-threads/gui.md`
- `doc/features/branch-discussions/design.md`
- `doc/features/branch-discussions/gui.md`
- `doc/features/composer/design.md`
- `doc/features/composer/gui.md`
- `doc/features/image-assets/design.md`
- `doc/features/transcript/design.md`
- `doc/features/transcript/gui.md`
- `doc/features/backend-runtime-recovery/design.md`
- `doc/features/backend-runtime-recovery/gui.md`
- `doc/features/settings/design.md`
- `doc/features/settings/gui.md`
- `doc/features/notifications/design.md`
- `doc/features/notifications/gui.md`
- `doc/features/activity-panel/design.md`
- `doc/features/activity-panel/gui.md`
- `doc/features/status-line/design.md`
- `doc/features/status-line/gui.md`
- `doc/features/diagnostics/design.md`
- `doc/features/lifecycle-yield/design.md`
- `doc/features/theming/design.md`
- `doc/features/theming/gui.md`
- `doc/input-hotkeys.md`
- `doc/systems/beryl-home-storage/design.md`
- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/syndic-conversation-history/concepts.md`
- `doc/systems/cas-live-syndic-transcript/design.md`
- `doc/systems/branch-discussion-handoff/design.md`
- `doc/systems/image-assets/design.md`
- `doc/systems/backend-runtime/design.md`
- `doc/systems/transcript-presentation/design.md`
- `doc/systems/transcript-presentation/renderer-architecture.md`
- `doc/systems/transcript-presentation/shell-boundary.md`
- `crates/beryl-home-store/doc/design.md`
- `crates/beryl-state/doc/design.md`
- `crates/beryl/doc/design.md`
- `crates/syndic-storage/doc/design.md`
- `crates/beryl-model/doc/design.md`
- `crates/beryl-backend/doc/design.md`
- `crates/beryl-app/doc/design.md`

The list names target documents that must agree before Checkpoint 0 can be accepted. Presence in this list does not mark an unresolved Operator proposal as accepted and does not authorize implementation.

`doc/features/theming/design.md` is affected only where obsolete workspace, graph, checklist, or removed-GUI roles must disappear. Theme-role hierarchy, inheritance, and theme-editor replacement are not target authority for this rework.

# Cutover Boundary

- This is the single architectural replacement unit for the complete post-workspace Beryl architecture described by the archived draft input, except for theme-system and theme-editor redesign.
- The unit owns removal of workspaces, semantic graph functionality, graph upkeep, checklists, the checklist-bound threaded-decision workflow, graph-dependent semantic search, and every live adapter or authority surface that preserves those models.
- The unit also owns their replacements: Beryl-home state, runtime/root configuration, Syndic thread and durable-draft ownership, exclusive CAS execution projections, multi-window/session orchestration, progressive shell readiness, exhaustive thread navigation, graph-independent branch discussion and handoff, image assets, thread metadata, and deferred turn garbage collection boundaries.
- These areas stay in one unit because no intermediate implementation may adapt the new Syndic/home model to the old workspace, graph, selector, or shell architecture.
- Checkpoint 0 converts target authority and moves obsolete documentation bodies out of live authoritative paths into this rework archive. No live source, manifest, test, script, or persisted-state removal begins until every retained decision has an authoritative owner, every effect-sized product or GUI decision is resolved, delegated system decisions are documented, native CAS lineage plus the unmodified targeted one-time recovery-injection boundary are proven live, the exact removal and jagged-edge blueprint is recorded, and the Operator accepts the complete target.
- After Checkpoint 0, removal is first. Obsolete live docs, source, tests, settings, tools, diagnostics, persistence models, and GUI surfaces are archived and removed before replacement implementation begins, even if the workspace temporarily does not compile or run.
- Old workspace-era persisted state is discarded. No migration adapter, importer, compatibility reader, dual-write path, or renamed workspace model is allowed unless the Operator later authorizes a separate one-shot import design.
- Existing Syndic transcript presentation and anchor-relative conversation rendering remain in place only where the target docs explicitly retain them. They must not be routed through workspace or graph compatibility models.
- Graph-independent semantic search is deferred until after this rework. This unit removes the current graph-dependent implementation and authority, then records future intent in root non-authoritative TODO material.
- Theme hierarchy and theme-editor redesign begin only after this rework closes. This unit removes obsolete workspace/graph/checklist-only roles and may add the minimum roles required by its final GUI, but it must not redesign the general hierarchy, inheritance model, editor navigation, or editor workflow.
- Temporary cutover shims are allowed only after obsolete code removal and only when named here or in a checkpoint with a removal condition. No shim may import, wrap, call, expose, or preserve archived code or models.

# Reference Snapshot

- `old-doc/next-big-rework.md` is an ignored local non-authoritative conversion input retained only because the Operator requested that the throwaway draft remain locally intact. It is outside repository membership, is not an ongoing decision ledger, and is unnecessary once every retained resolution, open decision, rejection, and deferral has been mapped into target authority or this tracker.
- `old-doc/gui/integration.md` preserves the pre-cutover main-window and slot declarations.
- `old-doc/features/conversation-threads/design.md` preserves the pre-cutover workspace/member thread behavior.
- `old-doc/features/conversation-threads/gui.md` preserves the pre-cutover thread-strip and column-browser composition.
- `old-doc/features/workspaces/` preserves removed workspace product and GUI authority.
- `old-doc/features/semantic-graph/`, `old-doc/features/graph-upkeep/`, and `old-doc/features/threaded-decisions/` preserve removed graph, checklist, upkeep, and checklist-bound decision authority.
- `old-doc/features/semantic-search/` preserves the removed graph-dependent search contract; only root non-authoritative TODO intent survives.
- `old-doc/product-features.md` preserves the obsolete aggregate feature index that root `doc/design.md` replaced.
- `old-doc/mockups/` preserves superseded exploratory New Thread and unified-navigation SVGs; they are historical and do not override live GUI docs.
- `old-doc/gui/widgets/column-browser/spec.md` preserves the removed recursive selector/graph-browser widget contract.
- `old-doc/gui/widgets/discussion-context/spec.md` preserves the rejected expandable fixed-panel proposal superseded during Checkpoint 0 GUI review.
- `../../features/conversation-threads/mockups/scoped-thread-root-lists.svg` is the approved illustrative navigation mockup. Written target authority controls where it differs; its Change Root panel is now deferred and non-target.
- Additional obsolete live docs and source move under this unit's `old-doc/` and `old-code/` trees during the removal checkpoint.

# Archived Draft Conversion Map

This map is the exhaustive disposition index for the ignored local `old-doc/next-big-rework.md` copy. Line numbers refer to its 998-line snapshot and remain stable while the Operator-retained local copy exists.

## Chronological Disposition

- Lines 1-6 are draft framing only. The rework scope, authority rules, and removal-first boundary replace them.
- Lines 7-438 are superseded checkpoint history. The draft itself declares supersession at lines 11-14, 63, 138, 160, 184, 288, 325, 371, and 395. A behavior from these ranges survives only when it is restated in lines 439-923 and mapped below; contradictory earlier alternatives are discarded.
- Lines 439-678 are the consolidated target direction and resolved product behavior. Their complete topical destinations are listed below.
- Lines 680-923 are the consolidated remaining-decision inventory. Resolved delegated mechanics are mapped to system/package authority; genuine effect-sized product or GUI choices remain named in Checkpoint 0 until the Operator accepts or changes their proposed target contracts.
- Lines 925-937 are retained architectural consequences and map to root responsibility, storage, Syndic, CAS-projection, runtime, and package boundaries below.
- Lines 939-961 are obsolete implementation-track speculation. Checkpoints 1-7 replace it. Draft line 956 is explicitly discarded because theme hierarchy/editor redesign is outside this rework.
- Lines 963-998 are rejection and deferral evidence. Their durable destinations are mapped below; the rejection prose itself is not target-state authority.

## Removal, Authority, And Responsibility

- Draft lines 17-20, 188-201, 439-451, 869-890, 925-937, and 965-972 map to root `doc/design.md`, this tracker's Cutover Boundary and Forbidden Local APIs, the target package docs, and Checkpoints 1 and 7.
- Workspaces, semantic graph, graph upkeep, checklists, checklist-bound decisions, graph tools/diagnostics/settings, and graph-dependent semantic search are removed rather than renamed or adapted.
- Syndic owns durable captured conversation records, CAS owns live execution and Codex policy, and Beryl-home state owns application metadata and orchestration. CAS catalogs and historical reads are explicitly rejected as Beryl authority.
- Draft owner-label prose at lines 221-227 and 682 is discarded as temporary planning metadata; unresolved ownership is represented by the current checklist.

## Beryl Home, Storage, And Failure

- Draft lines 21-24, 50-52, 455-497, 726-738, 902-913, 965-966, and 998 map to `doc/features/beryl-home/design.md`, its `gui.md`, `doc/systems/beryl-home-storage/design.md`, `crates/beryl-home-store/doc/design.md`, `crates/beryl-state/doc/design.md`, `crates/syndic-storage/doc/design.md`, and the root Persistence decisions.
- Those destinations own one physical Fjall database, typed logical domains, one live process per home, lock canonicalization, durable session/window state, accepted-write barriers, persistent-failure gating, reopen validation, and no redundant restore snapshot.
- The accepted presentation uses an Exit-only five-second busy-home surface, a Retry-and-Exit unreadable-startup surface, and in-place main-window preservation with automatic same-home recovery after persistent running-store failure. Persistent failure triggers best-effort active-turn interruption without closing windows, and only the last successfully committed session state can survive process termination.

## Windows, Startup, Runtime, Roots, And Empty Acquisition

- Draft lines 215-219, 260-266, 286-321, 399-437, 499-577, 581-592, 684-704, 740-756, 892-900, 982-994, and 997-998 map to `doc/features/main-windows/design.md`, its `gui.md`, `doc/features/conversation-threads/design.md`, its `gui.md`, `doc/systems/beryl-home-storage/design.md`, `doc/systems/backend-runtime/design.md`, and the app/backend/model package docs.
- The target retains independent main windows, dedicated Exit restoration, ordinary-close removal, deterministic virtual-desktop fallback, invisible minimal bootstrap, progressive readiness, zero-runtime onboarding, the split New Thread command, inherited `Ctrl+Shift+N` runtime/root, non-removable home roots, exact thread claims, and reusable pristine empty threads. Closing the final ordinary window durably records an empty restore set and terminates normally; the next start acquires one empty thread under the most recently used runtime/root before showing one replacement shell.
- Runtime addition is an OS-native executable-file selection and root addition is an OS-native directory selection. Runtime identity is the canonical selected Codex CLI path plus its derived Host or exact WSL environment; Beryl introduces no nested runtime/root forms.
- Hold-to-open New Thread behavior from earlier checkpoints is discarded. Runtime and root remain filters and execution bindings, never workspace/member replacements.
- Draft line 408's earlier claim that the New Thread ellipsis is the single runtime/root configuration entry point is superseded. The accepted unified picker exposes Add runtime and Add root from both the Thread Switcher and New Thread flyouts.
- Draft lines 553-562 additionally map to the backend-runtime working-directory contract: the exact bound runtime/root controls CAS-owned `AGENTS.md`, skill discovery, sandbox, and configuration behavior. Draft line 559's non-first-class-root option is superseded by durable configured-root records in the Beryl-home storage system.

## Syndic Threads, Durable Drafts, Mutation, And Cleanup

- Draft lines 30-49, 90-92, 117-134, 197-201, 329-391, 579-650, 706-724, 758-798, 815-843, 970-972, 978-979, 988-989, and 994 map to `doc/systems/syndic-conversation-history/design.md`, its `concepts.md`, `crates/syndic-storage/doc/design.md`, `doc/features/composer/design.md`, `doc/features/conversation-threads/design.md`, and `doc/systems/cas-live-syndic-transcript/design.md`.
- The committed-tail plus exactly-one-current-draft model supersedes early mutable-head and turn-started-thread wording. Parentage is immutable, accepted incomplete turns remain durable, replacement edit branches without detaching history, and this rework exposes no manual thread deletion or automatic empty-thread cleanup.
- Draft lines 205-206 and 837-843 describe a possible future thread-reference deletion contract, not current target behavior. Manual thread deletion and its reference-removal semantics are deferred to root `doc/design.md` `# TODO`; only the no-byte-collection invariant remains current.
- Dirty-only draft autosave uses a required 30-second default and a non-disableable 5-through-300-second setting; restored drafts preload before their window appears, later activation publishes draft and transcript state atomically, and draft-only threads remain until reuse. Future explicit garbage collection maps only to root `doc/design.md` `# TODO`.

## CAS Projection, Native Lineage, And Recovery Injection

- Draft lines 43-49, 117-134, 197-201, 629-650, 758-798, and 970-972 map to `doc/systems/cas-live-syndic-transcript/design.md`, `doc/systems/backend-runtime/design.md`, `crates/beryl-backend/doc/design.md`, `crates/syndic-storage/doc/design.md`, and `doc/features/backend-runtime-recovery/design.md`.
- The target uses one exclusive CAS projection per executing Syndic thread and exact binding/proof identities. Exact CAS-native continuation, resume, fork, or rollback lineage is always preferred when it already owns the required parent context.
- Fresh recovered-history projection is a resilience fallback only after native lineage is missing, stale, unavailable, or unprovable. It creates one new empty CAS thread, calls stable `thread/inject_items` exactly once with the ordered Syndic-derived item sequence, durably publishes only the session-scoped local binding proof after success, and never replays that prefix on a later turn or steering request.
- The exact target is Codex App Server 0.144.1. Reproducible native-lineage, stable-injection, and rejected-`additionalContext` evidence lives under `doc/memory/topic/codex-app-server/` and `doc/failures/cas-additional-context-materialization.md`.
- Unmodified 0.144.1 silently truncates each additional-context value above approximately 4,000 bytes and loses replay-dedup state across omission or resume. The Operator rejected multi-entry or repeated replay as an architectural workaround. `additionalContext` is not recovered-history transport.
- Exact 0.144.1 source proves stable `thread/inject_items` ordering, ordinary later-turn/resume/full-fork visibility, no model turn, no idempotency or public readback, active-turn queueing, lossy compaction, and incomplete persistence-error propagation. Target authority therefore confines recovery injection to one fresh loaded idle session and abandons ambiguity instead of retrying in place.
- Exact target-executable captures now prove native lineage without replay; canonical user/input-text and assistant/output-text recovery projection; fresh-idle ordering; atomic invalid-batch rejection; later-turn, restart/resume, and full-fork visibility; the 262,144-byte recovery ceiling; and the 65,536-byte selected-assistant passage. Focused nextest coverage separately proves both accepted payload bounds through Beryl's authenticated WebSocket transport.
- Branch selection is one bounded, provenance-framed assistant/output-text item injected once after native fork or fresh-lineage establishment and before the first branch-local user turn. It creates no public turn or hidden user-turn boundary and depends on no CAS-private contextual wrapper.
- History and branch context are never smuggled through user text or developer instructions, silently truncated, silently summarized, or repeatedly replayed. Oversized, unsupported, or unavailable projection is explicit failure.

## Branch Discussion And Handoff

- Draft lines 188-191, 235-258, 369-391, 658-678, 800-813, 931, 953, and 973-976 map to `doc/features/branch-discussions/design.md`, its `gui.md`, `doc/systems/branch-discussion-handoff/design.md`, the CAS-projection system, the Syndic system/package, the composer and transcript features, and Beryl-home durable jobs.
- A selected assistant reply creates a new thread whose first durable draft owns immutable source provenance and discussion context; no CAS work runs until user submission.
- Resolution remains conversational through a discussion-scoped dynamic tool. Queued input yields retryable no-state-change deferral; each accepted attempt disables input, persists one idempotent parent-handoff job, and archives only after the exact parent turn succeeds. A later fresh attempt is permitted only after the prior attempt ends terminally failed and releases the child for discussion.
- The earlier separate context-only turn and fixed discussion-context panel are discarded. The accepted target renders one synthetic context item in transcript flow and one fixed discussion-status strip immediately above the composer. Successful handoff leaves the owning window on the archived readonly child; retryable work remains temporarily input-locked, while terminal failure reopens the unarchived child and permits a later explicit fresh resolution attempt. No parent-deletion command is exposed.

## Catalog, Navigation, And Bounded Rendering

- Draft lines 49, 53, 210, 296-303, 348-350, 541-543, 652-656, 845-857, 977, 986, and 995-997 map to `doc/features/conversation-threads/design.md`, its `gui.md`, `doc/systems/beryl-home-storage/design.md`, `crates/beryl-app/doc/design.md`, and root performance decisions.
- Shared flyout anatomy, focus, collection switching, fixed geometry, variants, virtualization, tooltip preservation, and UI roles map to `doc/gui/widgets/thread-root-picker/spec.md`; feature-specific labels, row meaning, modes, and mounts remain in the conversation-thread GUI document.
- The accepted target is one exhaustive compact home-wide recent-first thread collection with runtime/root scoping, visible configured Codex executable paths on runtime rows, conditional executable-path disambiguation on thread rows, full-path root identity, open-elsewhere unavailability, fixed-height virtualized rows, stable identity, bounded overscan, and preserved focus, scroll, selection, and tooltip behavior.
- Recursive column navigation, eager row construction, row-edge manipulation menus, selection checkmarks, and renamed `target` language are discarded.
- Branch-sibling visualization and click-to-focus for an occupied thread map only to root `doc/design.md` `# TODO`.

## Images And Heavy Assets

- Draft lines 449-451, 475, and 859-867 map to `doc/features/image-assets/design.md`, `doc/systems/image-assets/design.md`, `doc/features/composer/design.md`, the Beryl-home storage system, and the app/backend package boundaries.
- The accepted target uses one Beryl-home-wide content-addressed asset store with typed logical references, stable labels, collision-safe identity, and verified Host/WSL runtime projection.
- Asset bytes live in Beryl-home-wide content-addressed sidecar files rather than Fjall values; Fjall stores durable metadata and references. Reference removal does not imply byte cleanup, and physical cleanup remains part of the future garbage-collection design.

## Removed Graph Features And Deferred Semantic Search

- Draft lines 188-196, 443-447, 869-890, 945, 957, 967-968, and 980-981 map to this tracker's removal checkpoints and root `doc/design.md` `# TODO`.
- Current semantic graph, graph upkeep, checklists, checklist-bound threaded decisions, graph tools/settings/diagnostics, and graph-dependent semantic-search authority have no target replacement or compatibility surface.
- Only the future intent to design graph-independent document and conversation-history search survives. It remains non-authoritative and has no live feature design until that work begins.

## Theme Boundary

- Draft lines 447, 884, 915-923, and 956 map to the mechanical role cleanup in `doc/features/theming/design.md`, this tracker's removal checkpoints, and root `doc/design.md` `# TODO`.
- This rework removes roles that exist only for deleted workspace/graph/checklist surfaces and adds only roles required by accepted replacement GUI.
- Theme hierarchy, inheritance semantics, editor navigation, and editor workflow redesign are explicitly deferred until this rework closes; draft line 956 is discarded.

## Secondary Consumers And Package Boundaries

- Draft lines 925-937 and every affected decision above map secondarily into settings, notifications, activity, status, diagnostics, lifecycle-yield, hotkey, GUI-integration, theming, and package docs named in Target Docs.
- These consumers must use Beryl-home, runtime/root, stable Syndic-thread, durable-draft, and exclusive-CAS-projection terms. They do not become alternative owners of storage, history, or product workflow.
- `crates/beryl-home-store` owns the physical database/lock/writer boundary; `beryl-state` owns typed Beryl metadata domains; `syndic-storage` owns Syndic records and APIs; `beryl-model` owns pure cross-package values; `beryl-backend` owns normalized CAS/runtime integration; and `beryl-app` owns shell orchestration and bounded GUI projections.
- Draft implementation-track lines 939-961 are fully replaced by Checkpoints 1-7. No track in the draft authorizes source work or supplies target-state behavior.

## Rejection And Deferral Index

- Draft lines 965-966 map to Beryl-home storage non-goals and failure rules.
- Draft lines 967-969 map to the Cutover Boundary and Forbidden Local APIs.
- Draft lines 970-972 map to root, CAS-projection, backend-package, and catalog non-goals.
- Draft lines 973-976 map to branch-discussion feature/system decisions.
- Draft line 977 maps to the flat catalog target and optional later-navigation TODO.
- Draft lines 978-979 map to the one-thread/one-window invariant and future garbage-collection TODO.
- Draft lines 980-981 map to deferred semantic-search TODO and removal of current graph-search authority.
- Draft lines 982-984 map to Beryl-home and main-window startup contracts.
- Draft lines 985-988 map to unchanged anchor-relative transcript presentation, catalog readiness, composer typing, and durable drafts.
- Draft lines 989-994 map to runtime/root/thread acquisition, window claims, and empty-thread behavior.
- Draft lines 995-997 map to catalog virtualization and coalesced runtime warm-up.
- Draft line 998 maps to the no-external-snapshot store and main-window restore boundary.

# GUI Ownership Inventory

This inventory controls the Checkpoint 0 GUI documentation batch. A feature may configure a canonical widget with product labels, content, commands, state meaning, and data, but reusable anatomy, state families, interaction, focus, scrolling, layout, variants, and UI roles belong to the classified widget.

## Integration-Owned Structure

- Main conversation, busy-home, and home-failure OS windows remain Beryl integration-owned application windows. The Settings OS window is registered in the same integration authority but directly owned by the external `settings-window`; none is a project-local widget.
- Main-window toolbar, conditional lineage and discussion regions, conversation body, transcript/activity/composer/status placement, and overlay bounds remain integration slots and outer layout authority.
- The external `settings-window` directly supplies the Settings OS window. `settings-window.page-content` is its routed nested integration slot for Beryl feature pages and subpages; no second Beryl-owned outer settings body wraps it.
- `transcript.code-panel-actions` is the routed nested slot for feature commands contributed to eligible transcript code panels.

## Bundled And External Widgets

- Bundled command button, segmented status bar, context menu, anchored context menu, hold-to-confirm button, tooltip, and their reusable contracts remain canonical for matching controls.
- External `settings-window`, `settings-row`, `color-input`, and `color-picker` own generic Settings shell, row, and color mechanics.
- External `text-input` owns editor, selection, clipboard, IME, atom-range, undo/redo, and multiline text mechanics.
- External `scrollbar` owns shared scrollbar chrome; project widgets own their viewport, range, routing, and bounded rendering policy.

## Existing Project-Local Widgets

- `code-panel` owns reusable code-like text presentation, range-backed realization, syntax, wrap, copy, bounded scrolling, and nested scroll ownership.
- `thread-root picker` owns reusable thread/root flyout anatomy, focus, selection modes, collection switching, virtualization, and layout.

## Required Project-Local Widgets

- `transcript-view` owns resident transcript presentation, realized viewport mechanics, exact scrolling, selection geometry, anchor preservation, bounded rendering, stable fallbacks, synthetic-context group presentation, nested-widget placement, and menu anchoring.
- `conversation-composer` owns the panel around external multiline text input, growth and clamping, editor chrome, inline-atom placement, and writable, submission-disabled, and inert visual states.
- `image-marker` owns the shared compact inline image-reference presentation used by composer and transcript contexts without owning label allocation or asset lifetime.
- `image-preview` owns the shared bounded image-preview overlay used from composer and transcript contexts.
- `table-panel` owns bounded large-table presentation, row/column viewport behavior, selection and copy affordances, stable fallbacks, and nested scroll ownership.
- `theme-editor` owns the currently retained role-navigator and selected-role property-editor GUI mechanics without redesigning the theme hierarchy or editor workflow.
- `activity-panel` owns its resize handle, bounded viewport, virtualized fixed-height rows, scrolling, and stable row interaction geometry.
- `main-window-notice` owns one bounded visible notice's anatomy, selectable detail, optional close control, at-most-three owner-command region, warning/error/info severity, and dismissible/persistent variants; feature policy owns queueing, deduplication, command effects, and recovery semantics.
- `thread-lineage` owns breadcrumb-strip anatomy, ordered navigation controls, unavailable/current presentation, truncation, and overflow behavior.
- `thread-selector-trigger` owns the stretchable active-thread trigger's primary title, trailing flyout affordance, open/loading/unavailable states, truncation, and trigger geometry.

## Feature-Local Composition

- The main toolbar is a one-off ordering of canonical command and trigger widgets. The joined New Thread split button remains the already justified feature-local composition of two command buttons.
- Busy-home and home-failure bodies are one-off heading, explanatory text, and command-button stacks with no reusable state or interaction model beyond their children.
- The Beryl Settings body and ordinary settings pages configure the external settings-window family; section names, page order, labels, values, and commands do not create a second settings-shell widget.
- The Themes settings page is a feature configuration of external settings rows. The Theme Editor itself is the project-local widget named above.
- Status-line presentation configures the bundled segmented status bar and anchored menus; it does not require a Beryl-specific status widget under the current contract.
- Branch-discussion context configures the transcript view's synthetic-context record anatomy, while branch lifecycle state configures a composer-adjacent bundled segmented status bar; neither requires another project-local widget.
- Backend-unavailable recovery configures a persistent per-window `main-window notice` with one Retry command. Running-store failure configures the same persistent notice without a manual recovery command and replaces it with an informational notice after automatic same-home recovery.
- Transcript selection actions, turn actions, composer marker actions, and status operations configure bundled context-menu widgets. Their product commands remain feature-owned.

## Resolved GUI Outside The Widget Batch

- Add runtime and Add root invoke OS-native file and directory pickers and require no Beryl form or reusable widget.
- Runtime/root removal, thread rebinding, and manual rename, pin, archive, and delete are explicitly deferred and expose no GUI in this rework.
- Empty-restore startup creates one ordinary main shell after acquiring an empty thread under the most recently used runtime/root, with the runtime home root as fallback.

# Checkpoint 1 Removal Blueprint

Checkpoint 1 is a destructive archive-and-removal checkpoint, not an implementation checkpoint. Every archived file moves to `old-code/<original repository-relative path>` inside this rework unit, then disappears from its live source path. No replacement package, target service, widget, compatibility facade, importer, or forwarding adapter may be introduced until every cut below and every zero-match scan has completed.

The cuts are ordered to sever obsolete authority before removing its implementations. A cut may leave Cargo metadata, compilation, tests, the executable, or the GUI unavailable. That is the intended evidence that the target is not being routed through the removed architecture.

## Cut 1A: Sever Composition And Export Roots

Archive and remove these complete live files first:

- `crates/beryl/src/cli.rs`, `crates/beryl/src/lib.rs`, `crates/beryl/src/main.rs`, and every file under `crates/beryl/tests/`.
- `crates/beryl-model/src/lib.rs` and every other file under `crates/beryl-model/src/` and `crates/beryl-model/tests/`.
- `crates/syndic-storage/src/lib.rs` and every other file under `crates/syndic-storage/src/` and `crates/syndic-storage/tests/`.
- `crates/beryl-app/src/lib.rs`, `crates/beryl-app/src/shell.rs`, `crates/beryl-app/src/shell/render.rs`, `crates/beryl-app/src/shell/render/conversation.rs`, and `crates/beryl-app/src/shell/render/startup.rs`.

This removes the current `BootstrapCli` Host/WSL workspace targeting, `RuntimeTarget`, `resolve_workspace`, old `AppBootstrap` initial-workspace contract, `run_app` and diagnostic startup signatures, every old `beryl-model` module export, direct `SyndicStore` construction, the monolithic `ShellView` state machine, and visible startup-shell renderer. Later checkpoints create new target composition roots; this cut does not leave a forwarding entry point.

## Cut 1B: Remove Workspace, Graph, Checklist, And Decision Source

Archive and remove these complete `beryl-app` roots and subtrees:

- `src/graph_dynamic_tools.rs`, `src/graph_dynamic_tools/`, `src/graph_tools.rs`, `src/graph_tools/`, `src/graph_upkeep_context.rs`, and `src/workspace_graph_commit.rs`.
- `src/threaded_decision_archive_core.rs`, `src/threaded_decision_branch_core.rs`, `src/threaded_decision_child_thread.rs`, `src/threaded_decision_context.rs`, `src/threaded_decision_dynamic_tools.rs`, `src/threaded_decision_dynamic_tools/`, `src/threaded_decision_graph_presentation.rs`, and `src/threaded_decision_resolution_core.rs`.
- `src/shell/graph.rs`, `src/shell/graph/`, `src/shell/graph_link_menu.rs`, `src/shell/graph_link_menu/`, `src/shell/graph_link_menu_state.rs`, `src/shell/graph_node_action_policy.rs`, `src/shell/graph_node_delete.rs`, `src/shell/graph_thread_start.rs`, and `src/shell/graph_worker.rs`.
- `src/shell/render/graph_link_menu.rs`, `src/shell/render/graph_link_menu_rows.rs`, `src/shell/render/graph_overlay.rs`, and `src/shell/render/graph_overlay/`.
- `src/shell/threaded_decision_archive.rs`, `src/shell/threaded_decision_branch.rs`, `src/shell/threaded_decision_branch/`, `src/shell/threaded_decision_progress.rs`, `src/shell/threaded_decision_resolution.rs`, and `src/shell/threaded_decision_resolution/`.
- `src/shell/semantic_thread_start.rs` and `src/shell/settings/graph.rs`.

Remove the graph and decision registrations from any retained registry before that registry can return to live membership. The removed tool names are `read_workspace_graph_summary`, `beryl_workspace_state`, `read_graph_neighborhood`, `read_checklist`, `upsert_graph_node`, `set_graph_node_parent`, `upsert_graph_soft_link`, `set_checklist_item_status`, `beryl_workspace_thread_ref_upsert`, `start_decision_branch`, `start_topic_decision`, and `resolve_decision_branch`.

No semantic-search source exists independently of these graph surfaces. Checkpoint 1 therefore records a verified zero implementation rather than inventing a placeholder or feature package.

## Cut 1C: Remove Obsolete State And Direct Storage

Archive and remove these complete `beryl-app` files:

- `src/beryl_home_dir.rs`, `src/persistence.rs`, `src/startup_state.rs`, `src/workspace_persistence.rs`, `src/workspace_image_assets.rs`, and `src/preferences.rs`.
- `src/member_thread_inventory.rs`, `src/shell/member_thread_inventory.rs`, `src/shell/workspace_members.rs`, `src/shell/workspace_open.rs`, `src/shell/workspace_persistence_worker.rs`, `src/shell/workspace_picker.rs`, `src/shell/workspace_picker_actions.rs`, `src/shell/workspace_rename_policy.rs`, and `src/shell/workspace_title.rs`.
- `src/shell/syndic_ingestion.rs`, `src/shell/syndic_transcript_storage_provider.rs`, and `src/shell/thread_activation.rs` plus `src/shell/thread_activation/`.
- `src/shell/composer_draft.rs`, `src/shell/composer_history.rs`, `src/shell/composer_image_assets.rs`, `src/shell/composer_image_delivery.rs`, and `src/shell/composer_image_label_frontier_worker.rs`.

The removed readers and writers include `StartupPersistence::{load, save}`, every `BerylWorkspacePersistence` reader/writer and rename transaction, `GuiPreferencesStore::{load, save}`, `AppearanceSettingsStore`, `SyndicStore::{open, commit}`, every direct `StoreOpenOptions` caller, and all app-side filesystem creation of per-workspace Syndic stores. The obsolete durable names are `startup-state.json`, `workspace.redb`, `workspace-rename-transaction.json`, `preferences.toml`, legacy `theme.toml`, per-workspace `syndic-storage`, and per-workspace image-asset directories.

The exact discarded record path is `startup-state.json` fields `recent_workspaces`, `last_opened_workspace`, and `next_untitled_workspace_sequence`; then `workspaces/<workspace-id>/workspace.redb` table `workspace_metadata` keys `manifest`, `conversation_state`, `ui_state`, `graph_upkeep_policy`, `semantic_graph_state`, `semantic_graph_revision`, `threaded_decision_state`, and `image_assets`. The current restore chain from last workspace through its active registered conversation and workspace-local Syndic view is removed as a unit. Timestamp/process/sequence workspace image ids are not imported into the target content-addressed asset store.

The entire current `syndic-storage` implementation is removed because it owns a standalone Fjall database and an old conversation/view schema. This includes its `conversations`, `conversation_views`, `conversation_source_threads`, inline `resource_bytes`, `recovery_markers`, and view-keyed CAS-binding keyspaces; stringly runtime/lineage proof fields; and title-bearing `ConversationRecord`. The replacement package later registers typed domains through `beryl-home-store`; it is not a wrapper around `SyndicStore`.

## Cut 1D: Remove Obsolete Navigation And Execution Orchestration

Archive and remove these complete `beryl-app` files and subtrees:

- `src/shell/column_selector.rs`, `src/shell/render/column_selector.rs`, `src/shell/thread_selector.rs`, `src/shell/thread_selector/`, and `src/shell/render/thread_selector.rs`.
- `src/shell/thread_navigation.rs`, `src/shell/thread_navigation_actions.rs`, `src/shell/thread_selection.rs`, and `src/thread_strip_breadcrumbs.rs`.
- `src/shell/render/workspace_picker.rs` and `src/shell/render/workspace_picker_row_menu.rs`.
- `src/branch_bootstrap_core.rs`, `src/branch_bootstrap_core/`, `src/shell/resident_branch_edit.rs`, and `src/shell/resident_branch_worker.rs`.
- `src/shell/pending_turn_input.rs`, `src/shell/turn_worker.rs`, `src/shell/turn_worker/`, `src/shell/thread_helpers.rs`, and `src/shell/thread_title/worker.rs`.
- `src/shell/backend_availability.rs` and `src/shell/lifecycle.rs`.
- `src/dynamic_tools.rs` and `src/thread_start_options.rs`, which currently register one global all-user-thread tool set.

Preserve the anchor-relative `src/shell/syndic_transcript/` host, provider contract, residency, frame, selection, context-menu, media, and diagnostic modules. They intentionally lose their direct storage adapter and activation wiring. Preserve the theme repository, theme editor subtree, transcript rendering, code panel, syntax highlighting, notification helpers, low-level stop helpers, and other target-compatible source unless a zero-match scan proves that an obsolete type remains inside them.

The old branch bootstrap and resident-edit paths are removed because they derive Beryl state from `ThreadSummary`, CAS metadata reads, backend fork/rollback results, workspace registrations, or app-local queues. Their target replacements begin from Syndic drafts, immutable parentage, exact Beryl metadata, and durable jobs.

## Cut 1E: Remove Backend Catalog And Workspace Launch Shapes

Preserve backend transport, authenticated WebSocket framing, JSON-RPC correlation, normalized live events, process-tree control, fork, rollback, steering, interruption, model/config reads, and narrow thread-metadata normalization. Remove their obsolete launch and catalog surfaces rather than archiving the backend package wholesale:

- Remove the current `BackendLaunchSpec` shape and constructors that accept only `RuntimeMode`, `WorkspaceId`, and `cwd`, including `managed_stdio_for_workspace`. Remove the literal Host `codex` PATH launch and WSL login-shell `codex` resolution.
- Remove `ManagedBackendServer::launch_and_probe_for_workspace` and every launch/probe overload that lacks the exact configured executable identity.
- Remove `WorkspacePathError`, `list_wsl_distros`, and the current Beryl-owned workspace path-selection helpers from `discovery.rs`; retain only helpers justified by later exact executable/root admission.
- Archive and remove `crates/beryl-backend/src/thread_archive.rs`. Remove `ManagedBackendSession::{archive_thread, unarchive_thread, probe_thread_archive_capabilities}`, every `ThreadArchive*` export, and archive/unarchive stream-event variants and tests.
- Remove `CompatibilityProbe::ThreadLoadedList`, `ThreadLoadedListResponse`, and the `thread/loaded/list` probe from `protocol.rs`, `session.rs`, exports, and tests.
- Keep `ThreadSummary` and exact metadata reads only for narrow live backend facts such as subagent labels. `shell/tool_activity_nickname.rs` is the sole app-side metadata-read exception; no app source may consume `ThreadSummary` as catalog, restore, selection, title, runtime, root, or durable-history authority.

Surgically update `crates/beryl-backend/tests/launch_and_protocol.rs`, `managed_websocket.rs`, and `managed_process_lifecycle.rs` to remove PATH/workspace launch, loaded-list, archive, and graph/checklist fixture cases while preserving target-compatible transport, process, protocol, fork, rollback, stop, model/config, and event coverage. The Phase 8 test named `websocket_outbound_frames_preserve_rework_context_transport_bounds` sends its canaries through ordinary `turn/start`; delete it or rename and rewrite it as neutral payload-framing coverage with no branch/recovery frame, canary, or semantic claim. The retained `probes/cas-phase8-live.ps1` and its recorded exact CAS request captures remain the authoritative `thread/inject_items` proof.

No live `thread/inject_items` backend implementation exists yet. Checkpoint 1 must leave that capability absent rather than disguise `turn/start`, developer instructions, `additionalContext`, or repeated replay as a temporary substitute.

## Cut 1F: Clean Settings, Diagnostics, Themes, And Secondary Surfaces

Lifecycle, theme, settings, diagnostic, and future discussion-resolution tool families survive through their feature-owned schema modules, but later orchestration registers them only within exact target feature/projection scope. There is no all-user-thread compatibility registry.

Preserve the generic diagnostic child process mechanism and target-compatible process, renderer, transcript-frame, media, settings-window, and bounded retained-state diagnostics. Surgically remove obsolete fields, commands, predicates, dispatch arms, and tests from `diagnostic_child_control.rs`, `diagnostic_child_dynamic_tools.rs`, `diagnostic_child_protocol.rs`, `diagnostic_dynamic_tools.rs`, `gui_control_dynamic_tools.rs`, and `memory_diagnostics.rs`. Archive `shell/diagnostics.rs` and `shell/diagnostic_fixtures.rs` when removal leaves no independent target behavior in those old shell bindings.

The diagnostic replacement must not retain `selected_workspace_id`, workspace-picker or graph popup facts, workspace transitions, workspace-thread listing, workspace switching, graph counters, graph columns, graph mutation queues, or workspace-persistence work counts. The old `WorkspaceIdle` and `WorkspaceSelected` wait predicates and `workspaceId` arguments are removed rather than renamed.

Remove file-backed active settings while retaining installed theme documents:

- Remove `GuiPreferencesStore`, `AppearanceSettingsStore`, their environment fallbacks, and their save workers.
- Surgically remove file-store handles, graph settings, workspace rebinding, and direct save tasks from `shell/settings.rs`, while preserving generic settings rows, theme pages, and the theme editor subtree for typed-service rewiring.
- Remove `active_theme_id` from the theme repository manifest and loaded repository snapshot. Active theme identity moves to the future typed Beryl settings domain; `.beryl/themes/manifest.toml` remains only the installed-theme index and `.beryl/themes/installed/*.toml` remains the installed-document store.
- Preserve theme parsing, validation, inheritance, preview, install, update, save-as, activation behavior, and the existing theme editor for later rewiring. This rework does not redesign the theme hierarchy or editor.
- Preserve submission projection, label allocation, status, rendering helpers, and any composer clipboard code only where independent of removed storage. The final audit proved the current clipboard source was not independent: it imported archived app-local draft code and retained `PendingNewThread`, so Cut 1G archives it and its test rather than inventing replacement draft or asset identities. Remove workspace/default-new-thread sections and direct-store methods from mixed files such as `composer_image_label_frontier.rs`, `status_line.rs`, `status_operation.rs`, and `status_operation_state.rs`.

Remove the following role families from `appearance/theme/built_in/roles.rs`, `capabilities.rs`, and `schema.rs`, including their enum variants, ids, inheritance edges, capability arms, schema defaults, snapshots, and tests:

- `MainThreadStrip`, `MainThreadStripActiveThread`, and `MainThreadStripActiveThreadLabel`.
- Every `Graph*` and `Checklist*` role.
- Every old `ThreadSelector*`, `WorkspacePicker*`, and `ColumnSelector*` role.

Also remove `AppearanceChromeSettings::conversation_thread_strip_background`, its runtime/theme conversions, obsolete frame/cache accessors, and graph/checklist authoring-guide prefixes. The rest of `appearance.rs`, `appearance/chrome.rs`, `appearance/runtime/`, and `appearance/theme/` remains live target-compatible theme implementation.

Retained mixed files require exact cleanup: remove startup-frame helpers from `shell/render/common.rs`; thread-strip, old selector, graph, workspace-picker, and old breadcrumb geometry from `shell/layout.rs`; removed-role accessors from `shell/render_theme.rs` and `shell/render_theme/frame.rs`; graph-upkeep injection from `shell/developer_instructions.rs`; workspace execution-target inputs from `shell/tool_activity.rs`; and `graph.` or `checklist.` authoring hints from `theme_dynamic_tools/authoring.rs`. Archive `shell/surface_accessors.rs` and `shell/diagnostic_fixtures.rs` when their old graph/workspace state has no independent target behavior left.

New thread-root picker, selector-trigger, lineage, discussion-status, notice, and progressive-shell roles are replacement work in their owning GUI checkpoint. Checkpoint 1 does not alias them to removed selector roles.

Remove the stale semantic-graph product bullet and old screenshot mount from `README.md`. Move `beryl-demo.png` to this unit's `old-doc/mockups/beryl-demo.png`; it depicts the removed Workspaces, Graph, and old thread-strip shell and is not target UI evidence.

## Cut 1G: Remove Obsolete Tests And Manifest Edges

Archive all current `beryl-model` and `syndic-storage` tests with their removed implementations. Archive these complete `beryl-app` test families: `workspace_*.rs`, `threaded_decision_*.rs`, `graph_overlay*.rs`, `graph_upkeep_*.rs`, `column_selector.rs`, `member_thread_inventory.rs`, `semantic_thread_start.rs`, `startup_persistence.rs`, `thread_navigation.rs`, `thread_selection.rs`, `branch_bootstrap_core.rs`, `backend_availability.rs`, `app_bootstrap.rs`, `conversation_layout.rs`, `composer_draft.rs`, `composer_history.rs`, `composer_image_asset_worker.rs`, `composer_image_delivery.rs`, `syndic_live_ingestion.rs`, `syndic_transcript_storage_provider.rs`, `thread_title_worker.rs`, and `transcript_branch_edit_target.rs`.

Archive `gui_preferences.rs` with the removed file store. Surgically remove obsolete cases from `appearance_settings.rs`, `appearance_settings_window.rs`, `chrome_theme_source.rs`, `composer_image_label_frontier.rs`, `developer_instructions_settings.rs`, `diagnostic_child_dynamic_tools.rs`, `diagnostic_child_protocol.rs`, `diagnostic_dynamic_tools.rs`, `gui_control_dynamic_tools.rs`, `memory_diagnostics.rs`, `phase7_theme_source.rs`, `status_line.rs`, `theme_document_format.rs`, `theme_repository.rs`, `theme_schema.rs`, `theme_settings_dynamic_tools.rs`, `tool_activity.rs`, and `transcript_rework_source_boundary.rs`. The last test must preserve only the exact `shell/tool_activity_nickname.rs` metadata-read exception and forbid loaded-list, direct store opens, and old catalog shapes everywhere else. A retained test may stay only if it has no obsolete path, type, role, tool, diagnostic, or persistence dependency.

Remove `redb` from root workspace dependencies and `beryl-app/Cargo.toml`, `deunicode` from root workspace dependencies and `beryl-model/Cargo.toml`, and direct `syndic-storage` from `beryl-app/Cargo.toml`. Remove Fjall from `syndic-storage/Cargo.toml` when its direct store is archived. Do not add `beryl-home-store`, `beryl-state`, or replacement dependencies during this checkpoint; their `Cargo.toml` files and source begin in Checkpoint 2.

Regenerate `Cargo.lock` only from those removals. Do not hand-edit it and do not retain a dependency solely to keep the intentionally broken cutover compiling.

The Operator explicitly authorized permanent new target code on 2026-07-13. Cut 1G may therefore add final package roots needed for Cargo manifest validity and small permanent target primitives needed to sever retained source from archived implementation. The binary root carries one explicit compile-time cutover gap until the real target bootstrap replaces it; no runnable placeholder is permitted. New target code must not mount archived modules, implement placeholder runtime behavior, restore old composition, or introduce adapters. Substantive home, state, bootstrap, shell, and feature implementation remains owned by later checkpoints.

## Removed Public API And Type Families

- `beryl-model` loses the complete current `workspace`, `conversation`, `cas_projection`, `provenance`, `semantic_graph`, and `threaded_decision` exports. This includes `RuntimeMode`, `WorkspaceId`, `ExecutionTargetId`, every `WorkspaceMember*` and `BerylWorkspace*` type, `RegisteredConversationThread`, `WorkspaceConversationState`, `SyndicConversationId`, `SyndicConversationViewId`, conversation member/rebind types, `CasGraphAction*`, `CasReflectionOutcome`, every graph/checklist/thread-ref type, `ThreadedDecision*`, `MutationProvenance`, and `MutationSource`.
- `beryl-app` loses the current workspace-shaped `AppBootstrap`, home-directory store constructors, `StartupMetadata`, `StartupPersistence`, `ResolvedStartupState`, `WorkspaceDeletionResolution`, all graph service/tool exports, all threaded-decision tool exports, every `BerylWorkspacePersistence` export, workspace image exports, and global `beryl_thread_start_options` functions. Reusing a future symbol name does not preserve its removed signature or behavior.
- The shell loses `ShellState::{Discovering, Picker, Opening, WorkspaceIdle, WorkspaceLoaded}`, `LoadedWorkspaceState`, `WorkspaceChoice`, `MemberThreadInventory*`, `ThreadSelectorState`, `ThreadSelectorProjection`, `ThreadSelectorColumnKey`, `ColumnSelector*`, `WorkspaceThreadNavigationHistory`, `ThreadStripBreadcrumbTrail`, `WorkspacePickerState`, `WorkspaceMembersState`, app-local `ComposerDraft` durability, and `ComposerHistoryScope::PendingNewThread`.
- `syndic-storage` loses `StoreOpenOptions`, `SyndicStore`, current batch and raw commit APIs, current `ConversationRecord` and view-centered index schema, inline resource-byte storage, and current stringly CAS projection binding records. Their concepts may return only in target records registered through the home store.
- `beryl-backend` loses workspace launch constructors, bare-PATH runtime launch, loaded-thread-list compatibility, and CAS archive APIs. Its transport/session/process types survive only where independent of those removed inputs and capabilities.

Retained source may not re-export one of these types as an alias, facade, deprecated compatibility name, or target-shaped wrapper around archived implementation.

## Intentional Jagged Edges After Checkpoint 1

- `beryl` has only its permanent process-entry root with an explicit compile-time cutover gap; its target CLI, bootstrap, home-open composition, and application launch modules remain absent, so it cannot launch the application.
- `beryl-model` has only its permanent crate root and no live values until target pure home, window, runtime, root, execution-binding, revision, command, and provenance types are introduced.
- `syndic-storage` has no physical database, record schema, current-draft API, provider implementation, or commit path until it can register through `beryl-home-store`.
- `syndic-storage` and `beryl-app` have permanent crate roots but no mounted target implementation. `beryl-app` still has no main shell controller, startup/session discovery, runtime/root registry, complete catalog, thread claim service, durable composer projection, target New Thread flow, branch-handoff coordinator, or multi-window orchestration.
- The retained transcript host and renderer have no concrete home-bound provider or activation path. They remain source for later wiring and are not backed by an empty, path-based, or compatibility provider.
- The retained settings and theme structures cannot commit scalar settings or active-theme identity until typed Beryl-state services exist.
- The retained diagnostic process and observation primitives have no workspace-era list/switch commands and no target thread/window command bridge yet.
- `beryl-backend` retains low-level transport and protocol normalization but has no valid exact-executable launch specification. It has no one-time injection API until the CAS-live checkpoint implements the proven `thread/inject_items` boundary.
- The workspace is expected not to compile or run. Theme match cleanup and retained tests may also remain temporarily uncompilable until their target identities and service inputs exist.

## Permitted Cutover Shims

None.

In particular, Checkpoint 1 may not add a `WorkspaceId`-to-`ExecutionBinding` adapter, `ThreadSummary` catalog row, app-local durable-draft wrapper, path-opening Syndic provider, placeholder home/state repository, unavailable old-shell facade, user-input/developer-instruction/additional-context history fallback, repeated history replay, or `run_app` path that bypasses home locking and validation.

## Checkpoint 1 Zero-Match Verification

Every command below must exit with no matches after the named removal cut. Archived paths under this unit are intentionally excluded because they are reference-only.

```powershell
rg --files crates | rg '(^|[\\/])(workspace[^\\/]*|semantic_graph|threaded_decision|branch_bootstrap_core|resident_branch_(edit|worker)|member_thread_inventory|column_selector|thread_strip_breadcrumbs|syndic_ingestion|syndic_transcript_storage_provider|thread_archive)([\\/]|\.rs$)'
```

```powershell
rg -n 'WorkspaceId|ExecutionTargetId|BerylWorkspace(Id|Manifest|Kind)|WorkspaceMember|WorkspaceConversationState|RegisteredConversationThread|SyndicConversation(Id|ViewId)|ConversationThread(MemberBinding|RebindRequirement)|PrimaryWorkspaceMember|CasGraphAction|CasReflectionOutcome|classify_cas_graph_action' crates --glob '*.rs'
```

```powershell
rg -n 'semantic_graph|SemanticGraph|SemanticNode|SoftLink|ThreadRef|Checklist|ThreadedDecision|threaded_decision|graph_upkeep|WorkspaceGraph|BERYL_GRAPH_DYNAMIC_TOOL_NAMESPACE|START_DECISION_BRANCH_TOOL|START_TOPIC_DECISION_TOOL|RESOLVE_DECISION_BRANCH_TOOL' crates --glob '*.rs'
```

```powershell
rg -n 'startup-state\.json|workspace\.redb|workspace-rename-transaction\.json|WORKSPACE_[A-Z_]+_KEY|workspace_syndic_storage_dir|WorkspaceImageAsset|workspace_image_asset|NEXT_IMAGE_ASSET_SEQUENCE|preferences\.toml|GuiPreferencesStore|AppearanceSettingsStore' crates --glob '*.rs'
```

```powershell
rg -n 'SyndicStore::open|StoreOpenOptions|syndic_storage::' crates/beryl-app/src --glob '*.rs'
rg -n 'Database::builder|resource_bytes|open_keyspace\(&db|sync_after_commit' crates/syndic-storage --glob '*.rs'
```

```powershell
rg -n 'thread/loaded/list|ThreadLoadedList|known_threads|KNOWN_THREADS|thread/archive|thread/unarchive|ThreadArchiveCapability|archive_thread\(|unarchive_thread\(|ThreadArchived|ThreadUnarchived' crates --glob '*.rs'
rg -n 'read_thread_metadata(_details)?\(' crates/beryl-app/src --glob '*.rs' --glob '!**/shell/tool_activity_nickname.rs'
```

```powershell
rg -n 'RuntimeTarget|resolve_workspace|initial_workspace|--host-path|--wsl-distro|--wsl-path|managed_stdio_for_workspace|launch_and_probe_for_workspace' crates --glob '*.rs'
rg -n 'BackendCommandLine::new\("codex"|Command::new\("codex"' crates/beryl-backend/src --glob '*.rs'
```

```powershell
rg -n 'list_workspace_threads|switch_workspace|selected_workspace_id|selectedWorkspaceId|workspace_idle|workspace_selected|workspace_picker_open|workspace_transition_pending|workspace_persistence_pending_work|graph_nodes|graph_soft_links|graph_thread_refs|graph_columns' crates/beryl-app crates/beryl --glob '*.rs'
```

```powershell
rg -n 'PendingNewThread|pending_new_thread|composer_draft: ComposerDraft|thread-strip-new-thread|render_thread_strip|toggle_thread_selector|current_new_thread_target' crates/beryl-app/src --glob '*.rs'
```

```powershell
rg -n 'MainThreadStrip|GraphOverlay|GraphColumn|GraphRow|ChecklistSidebar|ChecklistStatus|WorkspacePicker|ColumnSelector|ThreadSelectorColumn|main\.thread_strip|graph\.(overlay|column|row)|checklist\.(sidebar|header|row|status)|workspace_picker\.|column_selector\.' crates/beryl-app/src/appearance crates/beryl-app/src/shell --glob '*.rs'
rg -n 'active_theme_id' crates/beryl-app/src/appearance/theme/repository --glob '*.rs'
```

```powershell
rg -n 'redb|deunicode' Cargo.toml crates --glob 'Cargo.toml'
rg -n 'syndic-storage|syndic_storage' crates/beryl-app/Cargo.toml crates/beryl-app/src
```

```powershell
rg -n 'RECOVERY_CONTEXT_BOUND|BRANCH_SELECTED_ASSISTANT_PASSAGE|websocket_outbound_frames_preserve_rework_context_transport_bounds' crates --glob '*.rs'
rg -n 'doc/rework/beryl-home/old-code|rework\\beryl-home\\old-code' Cargo.toml crates --glob '*.rs' --glob 'Cargo.toml'
```

```powershell
rg -n 'Built-in semantic graph|graph management|Manually manage the graph|beryl-demo\.png' README.md
rg --files doc/features | rg 'workspaces|semantic-graph|graph-upkeep|threaded-decisions|semantic-search'
```

Positive replacement checks for `thread/inject_items`, target ids, Beryl-home/state Cargo membership, durable current drafts, autosave setting, and target widgets begin only in later checkpoints. Their deliberate absence is part of this removal checkpoint, not a reason to create a shim.

# Forbidden Local APIs

Target implementation must not introduce or retain:

- Workspace identity, workspace members, workspace pickers, workspace-scoped thread registration, or workspace-scoped persistence as renamed Beryl-home concepts.
- Semantic graph records, graph tools, graph upkeep, checklists, graph thread refs, graph-started threads, graph diagnostics, or checklist-bound threaded decisions.
- CAS thread lists, metadata reads, names, working-directory inventories, or historical transcript reads as Beryl catalog, title, restore, or durable-history authority.
- Adapters that present new Syndic threads, drafts, roots, windows, or home records through obsolete workspace, graph, member, conversation-view, or selector models.
- Dual reads, dual writes, compatibility fallbacks, or incremental persisted-state conversion between the removed and target architectures.
- `main-window.thread-strip`, workspace/member selectors, graph selectors, or a recursive column browser as surviving shell navigation.
- Hold or long-press as the New Thread runtime/root chooser, selector-row metadata action menus, checkmarks as selection indicators, or eager rendering of exhaustive catalog rows.
- A separate context-only discussion turn, a fixed expandable discussion-context panel outside transcript flow, developer instructions or ordinary user input as discussion/history materialization context, repeated recovered-history replay, or a GUI resolve/archive command for branch discussions.
- Beryl-owned runtime/root creation forms, manual thread rename/pin/archive/delete commands, runtime/root removal commands, or an existing-thread Change Root command during this rework.
- Multiple active turns on one Syndic thread or simultaneous ownership of one Syndic thread by multiple main windows.

# Checklist

## Checkpoint 0: Complete And Accept Target Authority

Checkpoint 0 distinguishes accepted product authority, autonomous completion gaps, final target review, and the separate source-cutover authorization gate.

### Conversion And Product Authority

- [x] Done: consolidated the navigation work and the complete post-workspace architecture under this single rework unit.
- [x] Done: archived the root throwaway draft as a temporary conversion input instead of treating it as live authority or a continuing ledger.
- [x] Done: preserved the already formalized main-window and unified-navigation proposal as partial target authority pending complete review.
- [x] Done: removed the obsolete aggregate product-feature index so feature and system entry points are owned by root `doc/design.md`.
- [x] Done: explicitly excluded theme-role hierarchy and theme-editor redesign from this unit.
- [x] Done: archived obsolete workspace, semantic-graph, graph-upkeep, checklist, threaded-decision, graph-search, recursive-selector, and superseded mockup documentation under `old-doc/` without changing source membership.
- [x] Done: built the exhaustive line-based and topical disposition map from every nonblank item in `old-doc/next-big-rework.md` to target authority, root TODO intent, rejection, discard, or a visible Checkpoint 0 item.
- [x] Done: rewrote root authority to remove workspace and semantic-graph target state, define the Beryl-home responsibility split, and record deferred semantic-search, garbage-collection, and theme-rework intent outside current authority.
- [x] Done: created the Beryl-home lifecycle/state, branch-discussion, and image-asset feature and system homes and drafted the affected root, package, GUI, hotkey, diagnostic, settings, notification, activity, status, lifecycle, transcript, runtime, and theming-boundary contracts in target-state form.
- [x] Done: drafted the AI-delegated Fjall keyspace and atomicity, lock, Syndic draft, CAS projection/recovery, queue/idempotency, runtime readiness, window claim, crash recovery, and bounded catalog decisions in their owning system and package documents.
- [x] Done: resolved Add runtime as an OS-native Codex-executable file picker and Add root as an OS-native directory picker, with path-derived Host/WSL identity, visible executable-path runtime metadata, cancellation preservation, validation failure, and no Beryl-owned form.
- [x] Done: explicitly deferred runtime/root removal and existing-thread rebinding, and defined empty-restore startup as one most-recent-runtime/root empty-thread acquisition before one ordinary shell appears.
- [x] Done: excluded manual rename, pin, archive, and delete from this rework while retaining generated titles and automatic branch-discussion archive after successful handoff.

### GUI Authority Formalization

- [x] Done: inventoried every live GUI control and nontrivial composite as bundled built-in, externally registered, project-local reusable, explicitly justified feature-local, or unresolved pending Operator-owned product design.
- [x] Done: kept application windows and their top-level layout in `doc/gui/integration.md`, classified the transcript viewport rather than the whole conversation window as a project-local widget, and recorded the exact widget-spec set for this batch.
- [x] Done: corrected Settings Window ownership so the external `settings-window` directly owns its top-level OS window and cross-feature pages/subpages mount through `settings-window.page-content`; the inventory found no additional Beryl settings-shell widget contract.
- [x] Done: formalized the transcript view, conversation composer, image marker, image preview, and bounded table panel as the exact project-local widgets selected by the inventory, while leaving Quote, Discuss, Edit, submission, persistence, resources, and provenance meaning feature-owned.
- [x] Done: formalized the retained theme editor as one bounded project-local widget over external settings rows, without beginning the deferred theme hierarchy, inheritance, navigation, or workflow redesign.
- [x] Done: formalized the supporting activity panel, main-window notice, thread lineage, and thread selector trigger as project-local widgets with bounded rendering and content-free diagnostics where applicable; synthetic discussion context is part of the transcript-view contract.
- [x] Done: configured global and branch-discussion status presentation through bundled segmented status bars plus anchored and ordinary context menus, hold-to-confirm button, tooltip, and disabled-command-tooltip contracts without adding redundant Beryl status widgets; explicitly retained the toolbar groupings and startup failure bodies as feature-local arrangements.
- [x] Done: added every accepted project-local widget spec to Target Docs, refactored every affected feature `gui.md` to mount and configure canonical widgets, and kept dependency references acyclic and direct.
- [x] Verification: every GUI mount resolves through the real containment hierarchy, every nontrivial composite has one classification, every repeated or stateful scroll surface has a bounded widget contract, every widget spec has complete required sections and UI roles, and no product, storage, or system architecture is duplicated into widget authority.

### Operator Review And Acceptance

- [x] Done: accepted the exact executable path as visible runtime-row secondary text, with affected thread rows also showing it when multiple runtimes share one Host/WSL environment label.
- [x] Done: accepted the required 30-second dirty-only draft autosave default, 5-through-300-second non-disableable setting, restored-draft preload, atomic activation presentation, and no automatic empty-thread cleanup.
- [x] Done: accepted the busy-home Exit-only five-second surface, unreadable-startup Retry-and-Exit surface, running-store in-place window preservation and automatic same-home recovery, and best-effort interruption of active CAS turns once persistent store failure is established.
- [x] Done: accepted one in-flow synthetic branch-context item and one fixed composer-adjacent discussion-status strip, with no context-only Syndic turn and no GUI Resolve or Archive command.
- [x] Done: accepted remaining on the archived readonly child after successful handoff; a live retryable handoff remains temporarily input-locked, while terminal failure reopens the unarchived child for continued discussion and a later explicit fresh resolution attempt without automatic duplicate parent turns.
- [x] Done: accepted Beryl-home-wide content-addressed image ownership with bytes in ordinary sidecar files rather than Fjall values, durable metadata and references in Fjall, memory-mappable decode/upload preparation, and no byte deletion until future garbage collection.
- [x] Done: accepted persistent per-window CAS recovery notices with Retry, final ordinary-window close as empty-restore-set termination, and end-turn sound eligibility when any known attention trigger is active.
- [x] Done: on 2026-07-13 the Operator accepted the complete Checkpoint 0 target authority as written, with later product and GUI tuning deferred until after the rework when needed.
- [x] Done: reconciled the accepted decisions and independent-review findings through their target feature, system, package, GUI, and widget authority.
- [x] Done: proved the targeted unmodified CAS `additionalContext` schema, capability gate, trust-role transformation, ordering, persistence, replay, steering, and per-entry truncation behavior through exact source, upstream tests, live stdio probes, and rollout inspection.
- [x] Done: rejected recovered-history transport and repeated replay through `additionalContext`; accepted exact CAS-native lineage as the ordinary path and stable `thread/inject_items` once on a fresh CAS thread as the resilience-fallback direction.
- [x] Done: proved exact 0.144.1 source behavior for stable `thread/inject_items`, including loaded-thread lookup, whole-vector validation, idle ordering, no turn lifecycle, normal persistence/resume/full-fork visibility, no idempotency/readback, active-turn queueing, lossy compaction, and incomplete persistence-error propagation.
- [x] Done: proved the remaining live 0.144.1 replacement boundary, including exact native lineage without replay, canonical recovery item projection, accepted transport bounds, next-request visibility, restart/resume, full fork, fresh-idle enforcement, atomic validation, and the abandon-instead-of-retry ambiguity rule.
- [x] Done: proved one clean exact channel for the accepted 65,536-byte selected assistant passage through one assistant/output-text injected item and Beryl's authenticated WebSocket transport, without reducing the limit, creating a user turn, or relying on `additionalContext` or private CAS wrappers.
- [x] Done: added the exact live-source removal, forbidden-API, jagged-edge, expected-gap, zero-match, and no-shim blueprint for Checkpoint 1.
- [x] Done: proved that no retained decision or unresolved item exists only in `old-doc/next-big-rework.md`, excluded that exact local copy from repository membership, and preserved it locally only because the Operator explicitly requested that it remain intact.
- [x] Gate satisfied: on 2026-07-13 the Operator separately authorized the Checkpoint 1 removal-first cutover, including its intentional temporary non-building state and no-shim rule.
- [x] Verification: all 31 authoritative `design.md` files begin with `# Goals` and `# Decisions`, own only their proper feature/system/package boundary, and contain target state rather than migration narrative.
- [x] Verification: every GUI mount resolves through `doc/gui/integration.md`, every exhaustive repeated surface has a bounded rendering contract, and exact disabled/loading/failure behavior is owned by a feature contract.
- [x] Verification: an independent completion review confirmed the conversion map is exhaustive, all findings were reconciled through owning target authority, target docs do not contradict one another, and no required behavior remains authoritative only in the ignored local draft.

Checkpoint 1 was authorized on 2026-07-13 after the Operator accepted the complete target authority and separately authorized the removal-first cutover.

## Checkpoint 1: Archive And Remove The Obsolete Architecture

- [x] Done: no removal work has begun while Checkpoint 0 remains incomplete.
- [x] Cut 1A: archive the obsolete executable, model, Syndic-store, and app-shell composition/export roots so no replacement can route through them.
- [x] Cut 1A progress: archived and removed `crates/beryl/src/cli.rs`, `crates/beryl/src/lib.rs`, `crates/beryl/src/main.rs`, and every file under `crates/beryl/tests/` byte-for-byte at `old-code/<original repository-relative path>`; preserved the package manifest and design authority, added no replacement entry point, and left the expected executable gap visible.
- [x] Cut 1A progress: archived and removed every file under `crates/beryl-model/src/`, `crates/beryl-model/tests/`, `crates/syndic-storage/src/`, and `crates/syndic-storage/tests/` byte-for-byte at `old-code/<original repository-relative path>`; preserved both package manifests and design authorities, retained no skeleton module, re-export facade, direct Fjall store, or compatibility type, and left the expected package-level gaps visible.
- [x] Cut 1A progress: archived and removed `crates/beryl-app/src/lib.rs`, `crates/beryl-app/src/shell.rs`, `crates/beryl-app/src/shell/render.rs`, `crates/beryl-app/src/shell/render/conversation.rs`, and `crates/beryl-app/src/shell/render/startup.rs` byte-for-byte at `old-code/<original repository-relative path>`; verified every other `beryl-app` source file remained byte-for-byte unchanged, introduced no replacement composition or export surface, and left the expected app-shell gap visible.
- [x] Cut 1B progress: archived and removed all 35 named graph, graph-upkeep, checklist, checklist-bound decision, graph GUI, semantic-thread-start, and graph-settings roots as 44 byte-for-byte Git-HEAD-matching files at `old-code/<original repository-relative path>`; removed tool literals have no app-source definition, semantic-search implementation remains zero, and the only residual aggregate imports are confined to the already-doomed `src/dynamic_tools.rs` registry assigned to complete Cut 1D archival and excluded from crate membership by Cut 1A.
- [x] Cut 1B: archive graph, graph-upkeep, checklist, checklist-bound decision, graph tool, and graph GUI source; verify that semantic search has no independent live implementation. Graph-only tests remain assigned to Cut 1G.
- [x] Cut 1C progress: archived and removed all 24 named Beryl-home, persistence, preference, workspace image, member inventory/opening, direct Syndic ingestion/provider/activation, app-local draft/history, and image-worker files as 24 byte-for-byte Git-HEAD-matching files totaling 442,573 bytes at `old-code/<original repository-relative path>`; the move was confined to repository source and archive roots, preserved the live theme implementation and ignored local draft, imported no old state, and introduced no replacement service or compatibility path. Residual old-store callers are confined to `shell/resident_branch_edit.rs` for Cut 1D and `shell/diagnostic_fixtures.rs` for Cut 1F; residual old persistence types are confined to mixed files already assigned to those cuts.
- [x] Cut 1C: archive workspace persistence, startup restore, workspace-local Syndic/image storage, file-backed scalar settings, old catalog assembly, direct store adapters, and app-local draft ownership.
- [x] Cut 1D progress: archived and removed all 24 named navigation, selector, workspace-picker rendering, branch bootstrap/edit, pending-input, turn-worker, backend-lifecycle, global dynamic-tool, and thread-start-option roots as 30 byte-for-byte Git-HEAD-matching files totaling 373,584 bytes at `old-code/<original repository-relative path>`; preserved the anchor-relative transcript host, theme repository/editor, transcript/code rendering, syntax highlighting, notifications, and low-level stop helpers unchanged; and introduced no replacement orchestration, registry, or compatibility path. Residual old theme-role, diagnostic pending-input, status-operation queue, direct-store fixture, and file-setting tokens remain only in unmounted mixed files explicitly assigned to Cut 1F.
- [x] Cut 1D: archive old workspace navigation, recursive selectors, thread strip, CAS-derived activation, branch bootstrap/edit, pending-input, global tool registration, and workspace-shaped turn orchestration while preserving target-compatible transcript/theme/rendering source.
- [x] Cut 1E progress: archived and removed `crates/beryl-backend/src/thread_archive.rs` as a byte-for-byte Git-HEAD-matching file; surgically removed the workspace/PATH launch specification and overloads, obsolete discovery helpers, loaded-thread-list compatibility, CAS archive requests/capabilities/events, launch-coupled startup orchestration, and the misleading branch/recovery `turn/start` canary test; retained independent authenticated WebSocket transport, JSON-RPC correlation, connected-session compatibility probing, process-tree supervision, fork, rollback, steering, interruption, model/config, normalized live-event, and narrow metadata source; preserved the Phase 8 live probe and memory evidence; and introduced neither `thread/inject_items` nor an exact-executable launch replacement.
- [x] Cut 1E: remove PATH/workspace backend launch, loaded-thread-list compatibility, CAS archive APIs, catalog misuse, and semantically misleading context transport tests while preserving target-compatible backend transport and protocol normalization.
- [x] Cut 1F settings/diagnostics progress: archived and removed `shell/diagnostic_fixtures.rs` and obsolete `shell/dynamic_settings.rs` as two byte-for-byte Git-HEAD-matching files totaling 20,126 bytes; removed workspace/graph diagnostic fields, commands, predicates, popup facts, counters, wire names, and focused tests; removed scalar file-store handles, environment fallbacks, graph settings, workspace rebinding, and settings save workers from retained settings source; and reduced the installed-theme manifest and repository snapshot to installed-document metadata with no active-theme identity. Generic settings-window/theme-editor composition and target-compatible process, renderer, transcript-frame, media, settings-window, retained-state, transport, theme document, and theme repository mechanics remain live with explicit typed-service gaps.
- [x] Cut 1F secondary-surface progress: archived and removed `shell/surface_accessors.rs` byte-for-byte at `old-code/<original repository-relative path>`; moved `beryl-demo.png` byte-for-byte to `old-doc/mockups/beryl-demo.png`; removed the old thread-strip, graph, checklist, thread-selector, workspace-picker, and column-selector role families, chrome conversions, frame accessors, startup frame, layout geometry, authoring hints, graph/workspace/default-new-thread branches, workspace execution-target inputs, README graph claims, and screenshot mount. Retained theme parsing, inheritance, repository documents, theme editor, transcript/composer/status/tool-activity leaves, and generic GUI primitives remain live without replacement roles, active-theme manifest fallback, or theme-editor redesign; obsolete retained-test fixtures remain explicitly assigned to Cut 1G.
- [x] Cut 1F: remove obsolete settings, diagnostics, theme roles, layout geometry, README claims, and old screenshot authority without beginning the deferred theme-system/editor redesign.
- [x] Cut 1G: remove obsolete tests and manifest edges, regenerate `Cargo.lock`, run every recorded zero-match scan, and record the expected non-building boundary.
- [x] Cut 1G test and source-boundary progress: archived 44 obsolete `beryl-app`, nine `beryl-model`, and one `syndic-storage` test files at `old-code/<original repository-relative path>`; surgically removed obsolete cases from retained tests; archived additional old shell bindings whose cleanup left no independent target behavior; and removed hidden workspace-model, CAS-archive, old-durable-token-snapshot, direct-settings, and dead-runtime-selection couplings from retained source. The shared dynamic-tool namespace and authenticated backend client connector remain as permanent target primitives, without restoring global tool registration or runtime launch behavior.
- [x] Cut 1G manifest and verification progress: removed the specified `redb`, `deunicode`, direct `syndic-storage`, and direct Syndic Fjall edges; regenerated `Cargo.lock` through Cargo with only the resulting dependency removals; proved Cargo metadata and retained library targets valid; passed all Checkpoint 1 zero-match commands, all 55 independently mountable retained `beryl-app` integration-test targets, and all 98 `beryl-backend` tests with lifecycle support. The package-wide `beryl-app` test command remains intentionally blocked at ten retained theme/integration suites that import the deliberately unmounted crate root, and the `beryl` process entry remains an explicit compile-time cutover gap rather than a runnable placeholder.
- [x] Independent completion review: audited the archive/removal diff, all 20 blueprint zero-match checks, manifests and lockfile, authoritative docs, retained backend/transcript/theme boundaries, and the documented build gaps. It identified one accidentally archived target-compatible lifecycle leaf and otherwise found no surviving adapter, importer, fallback reader, archive reference, forbidden API, or unplanned compile failure.
- [x] Independent-review finding closure: restored `shell/lifecycle_continuation.rs` byte-for-byte to its final live path because its fixed phase-continuation request remains target-compatible authority, and removed its duplicate from `old-code`. The four cleanup-then-archive shell snapshots remain intentional exact last-live snapshots rather than asserted Git-HEAD copies. The deleted five-line `features/theming/theme-editor.md` navigation pointer had no obsolete normative body to archive; its authority remains in the theming design, GUI composition, and theme-editor widget spec.
- [x] Blueprint decision: no forward-facing cutover shim is permitted; clean jagged gaps are sufficient.
- [x] Gate satisfied: Checkpoint 0 is accepted and the Operator explicitly authorized this removal checkpoint on 2026-07-13.
- [x] Verification: no live authoritative docs describe removed workspace, graph, checklist, threaded-decision, graph-search, or old selector behavior as current or transitional authority.
- [x] Verification: no live manifest, registry, entry point, source, test, setting, diagnostic tool, or script references archived source or forbidden local APIs.
- [x] Verification: source scans prove no adapter, compatibility facade, fallback read, dual write, or renamed obsolete model survives.

The next checkpoint begins when the obsolete architecture is absent from live authority and source, with any intentional build/runtime gap recorded explicitly.

## Checkpoint 2: Beryl-Home Storage And Process Foundation

- [x] Done: no target foundation implementation has begun before removal.
- [x] Phase 3 physical home-open foundation: implemented one-process-per-home ownership, opened-object canonical identity, fixed non-blocking OS locking with retained handles, typed busy/capability/schema/unreadable/open outcomes, strict layout admission, non-destructive recovery, and post-lock durable home identity creation.
- [x] Phase 3 physical-open verification: 23 elevated focused tests pass across subprocess contention and death, stale and orderly lock release, retained-path protection, case and extended aliases, real symlinks and junctions, reserved-state reparse rejection, schema and unreadable-state preservation, plus a separate real mapped-SMB rejection proof through `\\localhost\C$`.
- [x] Phase 4 typed-domain and writer foundation: implemented the private exact-schema domain registry, versioned keyspace families and records, opaque handles, bounded typed snapshot reads, reopen validators, sealed contributors, persistent home/domain revisions, deterministic conflict reporting, pre-admission cancellation, reentry rejection, one serialized cross-domain Fjall batch, and `SyncAll` before durable success.
- [x] Phase 4 verification: 42 elevated tests pass across atomic multi-domain reopen, validation and assembly rollback, exact schema/record rejection, missing control and domain keyspaces, explicit point/cursor limits, writer/read concurrency, cancellation timing, same-store reentry, and immediate process abort after durable success; raw Fjall API, obsolete-source, dependency, source-size, formatting, lint, and documentation checks pass.
- [x] Phase 8 Beryl session domain: registered one exact-schema `beryl-session` domain containing the active header, fixed-size window records, claims by window, and claims by thread. Implemented bounded restore-set authority, exclusive active/restoring claims, revision-checked placement/claim/window/Exit/restore commands, retained empty-set fallback, and no raw Fjall or encoded-record exposure.
- [x] Phase 8 minimal bootstrap: added session-first Beryl-state registration and a bounded header-to-exact-windows-to-header discovery query. Unrelated Beryl domains remain unregistered until explicit completion, so malformed unrelated state cannot delay the initial session read and no catalog, transcript, CAS, or placeholder draft enters the pre-window result.
- [x] Phase 5 health, recovery, and sidecar foundation: implemented one generation-aware fail-closed admission gate, bounded verification, single-flight exact same-home forced recovery with retained outer ownership, registered-domain and sidecar-aware reopen validation, stale-generation rejection, the accepted retry schedule, content-addressed sidecar publication and retained metadata-ordering tokens, and no durable-byte deletion surface.
- [x] Phase 5 package verification: 62 elevated tests pass across surfaced commit/persist faults, subprocess crash cuts, verification and repeated reopen, validator and referenced-sidecar disagreement, old-handle/command/token rejection, removal without replacement creation, and sidecar write, flush, rename, directory-sync, verification, deduplication, and orphan-retention boundaries. Production/all-feature checks, warnings-denied lint and docs, formatting, dependency and metadata inspection, source-size, raw-Fjall exposure, obsolete-source, and whitespace scans pass. Fjall issue #304 remains explicitly outside the injectable package boundary and is not claimed as covered.
- [x] Phase 6 Beryl product domains: implemented exact-schema additive runtime/root registries with executable, root-id, root-path, and mandatory home-root indexes; atomic runtime-plus-home-root creation; retained availability and root-activity revisions; immutable thread execution bindings; generated-title, activity, token-usage, and one-way branch-archive metadata transitions; bounded typed reads; and strict reopen validation without exposing encodings or raw storage.
- [x] Phase 6 verification: all 16 focused `beryl-state` nextest cases pass across atomic and concurrent creation, direct uniqueness, Host/WSL scope, missing-home-root reopen rejection, retained unavailable facts, stale revisions, bounded reads and values, immutable bindings, one-way metadata, persistence, and record versions; the 62-case all-feature home-store foundation suite remains green. Locked checks, warnings-denied lint and docs, formatting, workspace/dependency/source-size/forbidden-import/whitespace audits pass; `beryl-state` directly depends only on `beryl-home-store` and `beryl-model`.
- [x] Phase 7 Beryl non-session metadata domains: implemented closed typed settings with atomic Apply, kind-specific durable jobs with request idempotency and checkpoint-aware attempts, bounded deterministic catalog rows and recency indexes with exact source staleness, and content-addressed asset metadata with typed owner references and first-reference sidecar coupling. No generic payload, transcript body, raw storage, Fjall image bytes, or durable-byte deletion path exists.
- [x] Phase 7 verification: all 42 `beryl-state` and 16 `beryl-model` tests pass across schema, revision, bounds, atomicity, ordering, idempotency, lifecycle, staleness, sidecar, reference, and reopen invariants. The elevated all-feature `beryl-home-store` suite passes 63 cases after adding ordinary-startup sidecar-aware registration coverage. Scoped locked all-target checks, warnings-denied lint and docs, formatting, metadata, dependency, source-size, forbidden-import, raw-storage-exposure, durable-byte-deletion, and whitespace audits pass; the already-recorded Checkpoint 1 `beryl-app` retained-test cutover boundary remains unchanged.
- [x] Phase 8 verification: all 53 `beryl-state`, 16 `beryl-model`, and 63 elevated all-feature `beryl-home-store` tests pass. Session coverage proves exact 6,188-byte header and 655-byte window records, canonical tagged options with valid all-zero identities, 256-window bounds, reverse claim uniqueness, stale/newer generation rejection, atomic restore publication, retained fallback after final close, exact revision conflicts, unrelated-domain deferral, cross-home bootstrap rejection, and authoritative reopen failure. Locked scoped checks, warnings-denied Clippy and Rustdoc, formatting, metadata, dependency, source-size, forbidden-import, raw-storage-exposure, and whitespace audits pass; the declared Checkpoint 1 retained-theme-test workspace boundary remains unchanged.
- [x] Gate satisfied: Checkpoint 1 removal and the accepted storage/lock contracts are complete; Checkpoint 2 may begin.
- [x] Dependency gate resolved on 2026-07-13: the Operator approved exact official Fjall 3.1.6 with its known discarded journal `write_batch` result, reported upstream as `fjall-rs/fjall#304`. Checkpoint 2 proceeds without an adapter, retry, dual write, batch-size restriction, or claim that the suppressed-error path is safe; a corrected release or owned fork remains a later explicit decision.
- [x] Phase 2 package/value foundation: added permanent `beryl-home-store` and `beryl-state` workspace packages in the accepted dependency direction; resolved exact Fjall 3.1.6 with default `lz4`; and reconstructed only pure bounded Beryl-home, window, runtime, root, Syndic-thread, revision, command, idempotency, availability, execution-binding, placement, and provenance values in `beryl-model`.
- [x] Phase 2 verification: focused nextest, warnings-denied Clippy, Cargo docs, locked metadata, formatting, package/source-size checks, dependency trees, and forbidden-dependency scans pass; `beryl-model` depends only on Serde and imports no GUI, Fjall, async runtime, filesystem, process, backend, archived, or obsolete model boundary.
- [x] Phase 9 fault verification: exact I/O-kind, writer-panic, parent-forced termination, post-`SyncAll` surfaced failure, repeated recovery failure, sidecar truncation, final directory-sync, and final-verification cases now cover every Beryl-controlled Checkpoint 2 boundary. Literal injection inside Fjall's private batch commit remains only the accepted issue #304 gap and is not counted as covered.
- [x] Phase 9 integrated verification: one populated home crosses all seven Beryl domains, close/reopen, surfaced failure, failed verification, same-home recovery, complete handle reacquisition, stale-authority rejection, and final validation without mutating its caller-owned session snapshot. Deterministic races cover every accepted Checkpoint 2 concurrency-matrix family.
- [x] Phase 9 verification: all 72 elevated all-feature `beryl-home-store`, 61 all-feature `beryl-state`, and 16 `beryl-model` nextest cases pass with scoped locked checks, warnings-denied lint/docs, formatting, metadata/dependency/source-size/boundary/whitespace audits, and the exact expected `beryl` bootstrap failure. The full workspace remains intentionally non-building only at that process-entry gap and the already declared retained-theme-test cutover boundary.
- [x] Checkpoint boundary: unreadable-startup window composition, disabled live controls, resident GPUI-window preservation, and best-effort active-turn interruption require the later target process and multi-window shell. Checkpoint 2 retains their authoritative contracts but gates only the storage/session foundations they consume; the intentional `beryl` compile gap is not filled with a placeholder.
- [ ] Independent completion review remaining: Phase 10 must audit the complete Checkpoint 2 package graph, public boundary, durability and recovery behavior, schemas, fault coverage, and rework boundaries before Checkpoint 3 begins.

The next checkpoint begins when the target home store, lock, durability, and session foundations compile and pass their focused recovery tests.

## Checkpoint 3: Syndic Threads, Durable Drafts, And CAS Projections

- [x] Done: no target thread/draft/CAS implementation has begun before the home foundation.
- [ ] Remaining: implement stable Syndic threads with committed conversation tails, exactly one mutable current draft, immutable historical parentage, explicit submitted/incomplete lifecycles, and revisioned compare-and-update APIs.
- [ ] Remaining: implement atomic thread-plus-draft creation, dirty-only autosave, lifecycle flushing, draft freeze plus replacement, queued/steered input correlation, active-turn gates, replacement edit, and restart recovery.
- [ ] Remaining: implement one exclusive CAS execution projection per live Syndic thread, exact native-lineage precedence, stale/lost binding recovery, exact CAS/Syndic proof records, and one-time fresh recovery through stable `thread/inject_items` without modifying CAS.
- [ ] Remaining: implement exact per-thread active-turn routing and process-wide runtime/account correlation while permitting simultaneous turns on different threads.
- [ ] Blocked: implementation waits for Checkpoint 2 and accepted CAS protocol/system design, including any user-visible oversized-history or unavailable state.
- [ ] Verification: competing draft revisions, duplicate submission, stream loss, crash recovery, stale CAS bindings, forked shared history, incomplete turns, replacement edits, and simultaneous different-thread turns preserve exact identities and never create competing same-thread children.
- [ ] Verification: browsing remains wholly Syndic-backed and no CAS list, name, metadata, or historical transcript API becomes catalog, title, restore, or durable-history authority.

The next checkpoint begins when the target thread/draft model and CAS execution projection pass focused storage, protocol, concurrency, and recovery verification.

## Checkpoint 4: Multi-Window Shell, Runtime/Root Flows, And Thread Navigation

- [x] Done: formalized a partial merged-toolbar, lineage-strip, scoped-flyout, progressive-shell, and main-window lifecycle proposal.
- [ ] Remaining: implement independent main windows, thread claims, ordinary close versus application Exit, virtual-desktop restoration, active-turn interruption on close, and `Ctrl+Shift+N` acquisition.
- [ ] Remaining: implement zero-runtime onboarding, OS-native Codex-executable runtime selection, path-derived Host/WSL identity, runtime creation with non-removable home root, OS-native root-directory selection, remembered runtime/root state, empty-restore acquisition, eligible empty-thread claim/reuse, and split New Thread behavior.
- [ ] Remaining: implement invisible minimal bootstrap, independent catalog/history/CAS readiness, writable drafts with gated submission, coherent dimmed activation, per-window failures, and coalesced runtime warm-up.
- [ ] Remaining: implement the exhaustive compact recent-first catalog, root/runtime scoping, search, stable snapshots, open-elsewhere unavailability, navigation history, lineage, and fixed-height virtualized row presentation.
- [ ] Blocked: implementation waits for Checkpoint 3 and the accepted shell, flyout, row, failure, and activation contracts to be available through target-only boundaries.
- [ ] Verification: cover zero-runtime, first-runtime, duplicate executable selection, Host/WSL path derivation, native-dialog cancellation, invalid executable/root selection, empty restore set, current pristine thread, simultaneous window acquisition, stale claims, all-roots/root-scoped search, unavailable runtime/root/CAS, open elsewhere, failed activation, draft readiness, restart restoration, unreadable-startup presentation, and in-place preservation of every live window and its resident surface while the shared store is failed.
- [ ] Verification: rendered row count remains bounded as catalog size grows and content-free diagnostics cover stable identity, focus, tooltip anchoring, visible range, overscan, scroll position, and activation without logging titles, paths, or search text.

The next checkpoint begins when the ordinary shell and complete runtime/root/thread-navigation workflow operate only on target Beryl-home and Syndic boundaries.

## Checkpoint 5: Branch Discussion And Resolution Handoff

- [x] Done: resolved the thread-native branch, context-bearing first draft, conversational resolution-tool, deferred-while-queued, pending-composer gate, parent handoff, and archive-after-success direction in the conversion input.
- [ ] Remaining: implement `Discuss in new branch`, immutable selection provenance, readonly context presentation, the proven exact first-submission selected-context projection, thread-owned parent binding, and ordinary discussion conversation.
- [ ] Remaining: implement exact resolution admission, retryable deferral while queued input exists, durable parent-handoff queue, busy-parent ordering, restart recovery, idempotency, failure/retry, composer gating, and archive only after successful handoff.
- [ ] Remaining: implement the accepted post-archive navigation and missing/unavailable/open-elsewhere parent behavior without a GUI resolve/archive command.
- [ ] Blocked: implementation waits for Checkpoint 4 and the accepted branch contracts to be implemented through target-only boundaries.
- [ ] Verification: branch creation performs no CAS work before first user submission; its synthetic context remains presentation-only and outside turn counts; queued input is never discarded; deferred resolution changes no state; accepted resolution forbids later input; retries cannot duplicate a parent handoff; and failed handoff never archives the discussion.

The next checkpoint begins when branch, explore, resolve, hand off, and archive work entirely through target Syndic, CAS projection, and Beryl-home boundaries.

## Checkpoint 6: Assets, Automatic Metadata, And Deferred Cleanup

- [x] Done: deferred manual thread rename, pin, archive, delete, existing-thread rebinding, and runtime/root removal until later product designs.
- [x] Done: deferred explicit turn/resource garbage collection and graph-independent semantic-search implementation until after this rework.
- [ ] Remaining: implement the accepted Beryl-home or per-thread image-asset ownership model, runtime-readable Host/WSL path projection, labels, collision handling, references, and user-visible cleanup semantics.
- [ ] Remaining: wire the completed generated-title and one-way branch-archive metadata contributors into their owning Syndic projection and successful-handoff flows without introducing a general manual thread-management surface.
- [ ] Remaining: remove graph-dependent semantic-search authority and implementation, then record only non-authoritative future intent and provisional decisions in root TODO material.
- [ ] Remaining: preserve unreachable turns and resources until a separately designed future `Collect Garbage` operation; do not smuggle collection into asset cleanup or any later reference-removal path.
- [ ] Blocked: asset implementation waits for the preceding target storage and conversation boundaries.
- [ ] Verification: image submission works for Host and WSL roots without workspace directories; no manual metadata, rebind, removal, or deletion command leaks into this rework; and deferred features have no live compatibility implementation or authoritative feature contract.

The next checkpoint begins when all remaining durable state formerly owned by workspaces has a target Beryl-home, Syndic, or explicitly deferred owner.

## Checkpoint 7: Final Integration, Hardening, And Rework Closure

- [x] Done: no completion claim has been made while design and implementation checkpoints remain incomplete.
- [ ] Remaining: reconcile final root, feature, system, package, GUI, settings, hotkey, diagnostics, and source authority after implementation exposes the complete target state.
- [ ] Remaining: remove every temporary cutover shim and all obsolete exports, tests, settings keys, diagnostics, theme roles, docs, source archives from live membership, and references to forbidden APIs.
- [ ] Remaining: verify storage recovery, runtime/CAS failure, multi-window concurrency, thread/draft mutation, navigation, branch handoff, assets, deletion, performance bounds, and platform-specific Windows behavior end to end.
- [ ] Remaining: obtain an independent architectural completion review and address all findings through this tracker and the durable plan.
- [ ] Blocked: closure waits for every preceding checkpoint and all accepted verification evidence.
- [ ] Verification: formatting, full workspace build, focused and full tests, source-boundary scans, old-code membership scans, GUI diagnostics, crash harnesses, and manual Windows window/desktop scenarios pass.
- [ ] Verification: no live authority or source retains workspace, semantic graph, checklist, old threaded-decision, graph-search, old selector, or compatibility behavior; theme hierarchy/editor redesign remains clearly deferred and unstarted.

The rework closes only when the single target architecture is live, the obsolete architecture is absent, every intentional gap is closed, and no adapter or transition bridge remains.
