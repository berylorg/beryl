# Target Docs

Read these target-state docs before working on this rework:

- `doc/features/transcript/design.md`
- `doc/systems/transcript-presentation/design.md`
- `doc/systems/transcript-presentation/renderer-architecture.md`
- `doc/systems/transcript-presentation/shell-boundary.md`
- `doc/systems/syndic-conversation-history/design.md`
- `doc/systems/syndic-conversation-history/concepts.md`
- `crates/syndic-storage/doc/design.md`

# Cutover Boundary

Live Beryl transcript code now goes through the completed resident-host boundary under `crates/beryl-app/src/shell/syndic_transcript/`.

Archived legacy transcript source is reference-only under `doc/rework/syndic-to-renderer/old-crates/beryl-app/...`. Live source, manifests, tests, and build scripts must not import, path-include, or compile it.

The current live replacement boundary defines the Beryl-facing Syndic provider contract, compiles with in-memory Syndic-shaped fixture data, turns provider data into bounded resident presentation snapshots, realizes renderer-facing frame windows from resident snapshots, renders those frames through the GPUI host, and ports activation, scrolling, selection/copy, quote, context-menu/edit/branch, media, and status-line behavior over resident records.

Storage-backed provider implementation is outside this `syndic-to-renderer` replacement. This rework stops at the Beryl-facing provider boundary; a later provider implementation can satisfy that boundary from `syndic-storage` without changing renderer or storage ownership.

Transcript rendering may depend only on a Beryl-facing Syndic provider contract. It must not call `syndic-storage` directly.

Checkpoint 3 source work removed the legacy transcript renderer, history, presentation, viewport, Markdown parser, and residency modules from live `beryl-app` module trees. Remaining transcript-named live code is either the new host boundary or unrelated execution-detail/image support that is not part of the archived renderer stack.

The Cutover Blueprint section records the accepted early-source plan. It is historical reference now; live migration, source archival, and behavior-port checkpoints are complete.

When `doc/plan.md` is non-empty, it owns the current executable phase. This file owns the exhaustive rework checklist.

# Cutover Blueprint

## Temporary Cutover Behavior

The first source cutover may run with an empty or fixture-backed transcript host. That is an accepted temporary state for the compile-boundary phase and must not be hidden by adapters to legacy transcript history.

Fixture-backed snapshots are allowed for tests, diagnostic development paths, or explicitly planned implementation slices. They must not masquerade as selected real thread history.

Temporary behavior by surface:

- Transcript rendering: render only resident presentation snapshots from the new host. An empty host renders a stable empty transcript surface with ordinary shell chrome. Missing, pending, rejected, stale, or loading data does not render as transcript rows.
- Activation progress: activation may retain a previous coherent new-host snapshot until the replacement seed is coherent. If no previous new-host snapshot exists, the host enters the explicit empty or unavailable state. After source cutover starts, do not retain or reconstruct legacy transcript presentation for activation handoff.
- Scrolling: keep the exact manual scrolling algorithm. Empty or edge-clamped snapshots make scroll commands no-op except for demand facts and clamp diagnostics.
- Selection and copy: unavailable unless the current rendered resident records have stable provenance and geometry. Existing selection closes if the new host cannot maintain those invariants.
- Quote: unavailable when the host has no rendered resident source records. Composer quote insertion must not synthesize quote content from legacy history or stale caches.
- Branch, edit, and context menus: unavailable unless the target record has stable Syndic provenance and a ported command path. Menus do not open on empty space, fallback-only loading state, or legacy identifiers.
- Media actions: unavailable unless the target resident record has Syndic resource metadata and resident or demandable resource range facts. No legacy media cache or menu path is reused.
- Status-line view facts: publish only host-owned facts such as empty or unavailable state, fixture-backed state, activation revision, anchor or scroll mode, resident counts, pending demand, and rejected demand. Unknown facts are omitted or rendered unknown rather than copied from legacy transcript state.
- Diagnostics: expose new-host diagnostics only. Legacy residency, presentation, viewport, and renderer counters are omitted after cutover, even when that temporarily reduces diagnostic detail.
- Retained previous transcript visibility: only a previously coherent new-host snapshot may be retained across activation. The old legacy transcript panel is reference-only once source migration starts.
- Activity and shell chrome: non-transcript shell state may continue from existing sources when it is not derived from legacy transcript internals. It must not produce transcript content or correctness-sensitive copy, quote, menu, media, or status payloads.

During early source cutover, real selected-thread transcript content through the new host, Markdown-preserving copy, quote harvesting, branch/edit transcript menus, media preview/copy/save actions, precise transcript-derived status-line facts, and rich transcript diagnostics may be unavailable until the relevant behavior-port checkpoint is complete.

## First Source Cutover Slice

Name: empty resident-host shell cutover.

Owning live module path: `crates/beryl-app/src/shell/syndic_transcript/`.

Purpose: install the new shell-facing transcript host and GPUI panel as the visible transcript surface while the host is still empty or fixture-backed. This creates the first compile boundary for new Syndic transcript code without adapting old transcript history, presentation, viewport, Markdown, media, or residency models.

Implementation scope:

- Add a new `shell::syndic_transcript` module tree with the host state, immutable resident presentation snapshot, GPUI panel, demand-fact sink, empty diagnostics, and disabled command results needed by shell code.
- Add one `SyndicTranscriptHost`-style field to `ConversationSurfaceState` for selected conversation surfaces.
- Replace `ShellView.transcript_panel: Entity<render::transcript::TranscriptPanel>` with a new panel entity from `shell::syndic_transcript`.
- Replace old panel creation, key binding, notification, frame-metrics read, diagnostic read, popup-close, and conversation render wiring that currently refer to `render::transcript::TranscriptPanel`.
- Replace `transcript_panel_snapshot()` with a new-host snapshot path that publishes only resident new-host state. In this slice the snapshot may be empty, unavailable, or fixture-backed.
- Leave selected-thread activation, turn publication, live activity, and backend thread inventory compiling through their current code paths, but do not let those paths feed rendered transcript content through the new panel.
- Keep temporary transcript behaviors explicit: selection, copy, quote, branch/edit menus, media actions, transcript-derived status facts, and rich transcript diagnostics return unavailable or empty results unless implemented over new-host resident records.

Shell fields and accessors replaced first:

- `ShellView.transcript_panel` and its construction site.
- `render::transcript::bind_keys(cx)` for transcript-panel key handling.
- `shell::render::conversation` function signatures that accept the transcript panel entity.
- `notify_transcript_panel()`.
- `transcript_panel_snapshot()`.
- `transcript_panel.read(cx).frame_metrics_snapshot()`.
- `transcript_panel.read(cx).diagnostic_snapshot()`.
- `transcript_panel.update(cx, |panel, cx| panel.close_popups(...))`.

Legacy source left untouched until later slices:

- `shell::transcript_history`, `shell::transcript_presentation`, `shell::transcript_presentation_reconcile`, `shell::transcript_projection`, `shell::transcript_markdown`, `shell::transcript_viewport`, `shell::transcript_viewport_navigation`, `shell::transcript_viewport_scroll_coordinator`, `shell::transcript_residency_*`, `shell::transcript_media*`, `shell::transcript_quote*`, `shell::transcript_branch*`, `shell::transcript_edit*`, and `shell::render::transcript*`.
- `selected_thread_activation`, `thread_activation`, and turn publication internals that still construct `TranscriptHistoryWindow`.
- Legacy tests that directly unit-test those old modules, until the module they test is archived or rewritten in the legacy-removal checkpoint or a later behavior checkpoint.

Tests to retire or rewrite in the first implementation slice:

- Source-shape tests that assert the old panel snapshot, old transcript theme boundary, old `render::transcript::TranscriptPanel`, or old panel notification contract: `conversation_shell_source.rs`, `transcript_theme_source.rs`, and `transcript_theme_candidate_source.rs`.
- Any test assertion that expects the first cutover panel to render selected-thread transcript rows, return legacy frame metrics, expose old residency counters, or open legacy transcript menus should be rewritten to assert the documented empty or unavailable new-host behavior.

## First Implementation Verification Gates

These gates apply to the first implementation phase after Checkpoint 1 is accepted.

Build gates:

- `cargo check -p beryl-app` must pass.
- `cargo nextest run -p beryl-app --test conversation_shell_source` must pass.
- `cargo nextest run -p beryl-app --test transcript_theme_source` must pass.
- `cargo nextest run -p beryl-app --test transcript_theme_candidate_source` must pass.

Forbidden legacy API gates:

- `rg -n "TranscriptHistoryWindow|TranscriptPresentation(State|Reconcile)?|TranscriptResidency(Controller|PageRequest)?|TranscriptViewportState|transcript_markdown|transcript_projection|transcript_prepublication_preparation|selected_thread_activation|render::transcript" crates/beryl-app/src/shell/syndic_transcript` must print no matches.
- `rg -n "render::transcript::TranscriptPanel|render::transcript::TranscriptPanelSnapshot|render::transcript::bind_keys|Entity<TranscriptPanel>|TranscriptPanelSnapshot" crates/beryl-app/src/shell.rs crates/beryl-app/src/shell/render.rs crates/beryl-app/src/shell/render/conversation.rs crates/beryl-app/src/shell/surface_accessors.rs crates/beryl-app/src/shell/diagnostics.rs` must print no matches.

Renderer and storage boundary gates:

- `rg -n "syndic-storage|syndic_storage" crates/beryl-app/src/shell/syndic_transcript crates/beryl-app/src/shell/render.rs crates/beryl-app/src/shell/render/conversation.rs crates/beryl-app/Cargo.toml` must print no matches.
- `rg -n "doc/rework/syndic-to-renderer/old-crates|old-crates" Cargo.toml crates -g Cargo.toml -g "*.rs" -g "*.toml"` must print no matches.

Obsolete source-test gates:

- `rg -n "include_str!\\(\"../src/shell/transcript_panel_snapshot.rs\"|render::transcript::TranscriptPanel|TranscriptPanelSnapshot|render::transcript::bind_keys|TranscriptHistoryWindow|TranscriptPresentationState|TranscriptViewportState" crates/beryl-app/tests/conversation_shell_source.rs crates/beryl-app/tests/transcript_theme_source.rs crates/beryl-app/tests/transcript_theme_candidate_source.rs` must print no matches.
- Those three source-shape tests must assert the new visible transcript boundary or temporary unavailable behavior. They must not continue asserting the old panel snapshot, old transcript theme boundary, old panel notification path, or old selected-thread row rendering.

Transitional allowlist:

- Legacy transcript modules under `crates/beryl-app/src/shell/transcript_*`, `crates/beryl-app/src/shell/render/transcript*`, selected-thread activation internals, turn publication internals, and tests that directly unit-test those old modules may continue to reference forbidden legacy APIs until the legacy-removal checkpoint or a later behavior checkpoint archives or replaces them.
- The first implementation phase must not use that allowlist for new `shell::syndic_transcript` code, visible panel wiring, source-shape tests listed above, or direct `syndic-storage` access from renderer code.

For zero-match `rg` gates, no matches is the pass condition. A match means the implementation has crossed the cutover boundary and must be fixed or explicitly replanned before continuing.

## Related Feature-Doc Impact Decision

Phase 7 scanned related authoritative feature docs for target behavior that could contradict the Syndic transcript boundary.

Updated docs:

- `doc/features/composer/design.md`: transcript quote insertion now consumes quote payloads from the transcript feature and must not synthesize quote text from backend history, legacy transcript caches, stale projections, or nonresident transcript ranges.
- `doc/features/status-line/design.md`: transcript view counts now consume transcript host facts only, and the status line must not inspect transcript residency internals, presentation records, renderer state, Syndic storage, backend history, or rendered text to derive view values.
- `doc/features/conversation-threads/design.md`: transcript-originated title, branch, and edit actions now consume transcript context-menu targets with stable provenance and resident source data; activation progress wording no longer assigns media admission to rendering.

Docs intentionally unchanged:

- `doc/features/activity-panel/design.md`: already defines activity as transient operational presentation outside durable transcript narrative and prohibits synchronous backend rendering reads.
- Media actions have no separate feature doc. The visible target behavior remains owned by `doc/features/transcript/design.md`, while host and resource-demand boundaries are owned by `doc/systems/transcript-presentation/shell-boundary.md`.

## Checkpoint 1 Review Decision

The Cutover Blueprint is accepted with one planning correction: the next checkpoint installs the empty resident host as the visible shell transcript surface, while full legacy transcript source removal is a later checkpoint.

This keeps the first source checkpoint limited to the new compile boundary and avoids combining visible-panel replacement, selected-thread activation replacement, and source archival into one hard task.

# Reference Snapshot

Archived old docs:

- `doc/rework/syndic-to-renderer/old-doc/features/transcript/design.md`

Archived old source snapshots:

- `doc/rework/syndic-to-renderer/old-crates/beryl-app/...`

Use old source only as reference. Reuse useful leaf-level behavior by rewriting it into new source boundaries.

# Forbidden Local APIs

New Syndic transcript code must not depend on these legacy transcript concepts:

- `TranscriptHistoryWindow`
- `TranscriptPresentation`
- `TranscriptPresentationReconcile`
- `TranscriptResidencyController`
- `TranscriptResidencyPageRequest`
- `transcript_markdown::parser`
- `transcript_projection`
- `transcript_prepublication_preparation`
- `selected_thread_activation` transcript publication internals
- `render::transcript` legacy renderer modules

Do not add adapters from resident Syndic data to these legacy models.

Do not add adapters from these legacy models to the new presentation data model.

# Checklist

## Checkpoint 0: Rework Authority Established

- [x] Checkpoint: active rework authority is discoverable from root `doc/plan.md`, this tracker, and target docs.
- [x] Create active rework tracker at `doc/rework/syndic-to-renderer/REWORK.md`.
- [x] Move obsolete transcript feature doc out of `doc/features/`.
- [x] Create new target transcript feature doc at `doc/features/transcript/design.md`.
- [x] Move Syndic-native renderer note under `doc/features/transcript/`.
- [x] Remove or absorb any remaining target-state transcript notes outside `doc/features/`.
- [x] Verify no obsolete transcript design body remains in authoritative docs.
- [x] Verify `doc/plan.md` points to this tracker while the rework is active.

## Checkpoint 1: Cutover Blueprint

- [x] Checkpoint: first compile-boundary cutover is reviewed and accepted before source migration starts.
- [x] Reconcile root `doc/design.md` with the target Syndic transcript boundary.
- [x] Decide whether related feature docs need target updates for composer quote insertion, status-line view state, activity projection, conversation-thread navigation, edit/branch menus, and media actions.
- [x] Define the minimal live replacement transcript host boundary.
- [x] Decide temporary behavior while the new host is initially empty or fixture-backed for transcript rendering, selection, quote, branch/edit menus, media actions, status-line view facts, and diagnostics.
- [x] Identify the first source cutover slice, including owning live module path, shell fields and accessors to replace, tests to retire or rewrite, and verification commands.
- [x] Define forbidden-import and build verification gates for the first implementation phase.

## Checkpoint 2: Empty Resident Host Visible In Shell

- [x] Checkpoint: the visible shell transcript panel is the new empty resident-host surface, with no legacy transcript content feed and no direct Syndic storage access.
- [x] Add new live source in authoritative crate/module paths under `crates/beryl-app/src/shell/syndic_transcript/`.
- [x] Replace the visible transcript panel entity, construction site, render wiring, key binding, notification path, snapshot path, popup closure, frame metrics, and diagnostic reads with the new host surface.
- [x] Retire or replace first-slice source-shape tests that assert old panel, theme, snapshot, bind-key, or notification behavior.
- [x] Verify the first implementation gates for the new host and visible panel wiring.
- [x] Verify legacy transcript modules remain only on the transitional allowlist and do not feed rendered new-host content.

## Checkpoint 3: Legacy Transcript Removed From Live Build

- [x] Checkpoint: `beryl-app` compiles without legacy transcript renderer, history, presentation, viewport, Markdown parser, or residency modules in live module trees.
- [x] Replace selected-thread activation and thread activation publication paths that still constructed or passed `TranscriptHistoryWindow`.
- [x] Replace live turn publication and non-legacy shell command internals that still constructed or invoked legacy transcript publication.
- [x] Retire or replace tests that import legacy transcript internals by path before moving source.
- [x] Move obsolete transcript source snapshots under `doc/rework/syndic-to-renderer/old-crates/beryl-app/...`.
- [x] Remove legacy transcript modules from live `mod` trees.
- [x] Verify Cargo manifests and build scripts do not reference `old-crates`.
- [x] Search for live imports of archived source after source archiving.
- [x] Search for new code using forbidden local APIs.

## Checkpoint 4: Syndic Provider Contract Compiles And Is Tested

- [x] Checkpoint: the Beryl-facing Syndic transcript provider contract compiles with in-memory fixtures and contract tests.
- [x] Define the Beryl-facing Syndic transcript provider contract.
- [x] Add in-memory fixture provider for new transcript tests.
- [x] Add contract tests for transcript-view cursor pages.
- [x] Add contract tests for resident projection records.
- [x] Add contract tests for resource metadata and range reads.
- [x] Add invalidation/revision behavior tests.

## Checkpoint 5: Resident Presentation Core

- [x] Checkpoint: fixture-backed provider data becomes bounded resident presentation snapshots without GPUI rendering.
- [x] Define the resident presentation core boundary with resident snapshots, presentation snapshots, demand facts, residency policy, diagnostics, and provider-request bookkeeping.
- [x] Implement transcript-view page residency over the provider contract.
- [x] Implement presentation data records with Syndic provenance.
- [x] Add pure tests for projection provenance preservation, projection revision identity, narrative ordering, rejected projection handling, and absence of Beryl-side Markdown parsing.
- [x] Implement resource metadata and range residency.
- [x] Implement budget and fallback diagnostics.
- [x] Implement resident release and invalidation behavior.
- [x] Add pure tests for resource demand facts, stale resource handling, rejected resources, and bounded byte retention.
- [x] Add pure tests for budget rejection diagnostics and fallback behavior.
- [x] Add pure tests for resident release and invalidation behavior.

## Checkpoint 6: GPUI Host Renders Resident Snapshots

- [x] Checkpoint: the new GPUI transcript host renders only resident presentation snapshots and reports demand facts without direct storage access.
- [x] Implement realized frame window and scroll controller.
- [x] Implement GPUI transcript host over resident presentation snapshots.
- [x] Implement nested code/table/media demand reporting without direct storage reads.
- [x] Add targeted render or integration tests for empty, fixture-backed, bounded, and rejected resident snapshots.

## Checkpoint 7: Behavior Port Slices

- [x] Checkpoint: activation behavior is ported and verified against the new transcript boundary.
- [x] Port activation behavior.
- [x] Checkpoint: exact manual scrolling and clamp behavior are ported and verified against resident boundaries.
- [x] Port exact manual scrolling and clamp behavior.
- [x] Checkpoint: live autoscroll behavior is ported and verified without legacy viewport state.
- [x] Port live autoscroll behavior.
- [x] Checkpoint: selection and Markdown-preserving copy behavior are ported and verified over rendered resident records.
- [x] Port selection and Markdown-preserving copy behavior.
  - [x] Define resident selection and copy-payload domain behavior.
  - [x] Capture renderer selection facts over realized resident records.
  - [x] Wire shell copy command to resident copy payloads.
  - [x] Verify selection and Markdown-preserving copy behavior end to end.
- [x] Checkpoint: quote harvesting is ported and verified over rendered resident records.
- [x] Port quote harvesting behavior.
  - [x] Define resident quote-payload domain behavior.
  - [x] Capture renderer quote target facts over realized resident records.
  - [x] Wire shell quote command to resident quote payloads.
  - [x] Verify quote harvesting behavior end to end.
- [x] Checkpoint: context menus and edit/branch integration are ported and verified against Syndic provenance.
- [x] Port context menus and edit/branch integration against new provenance.
  - [x] Define resident context-menu target domain behavior.
  - [x] Capture renderer context-menu target facts over realized resident records.
  - [x] Wire shell context-menu commands to resident targets.
  - [x] Wire edit and branch actions to resident context-menu targets.
  - [x] Verify context menus and edit/branch integration end to end.
- [x] Checkpoint: media admission, preview, copy, and save behavior are ported and verified through resident resource ranges.
- [x] Port media admission, preview, copy, and save behavior.
  - [x] Define resident media action target and payload domain behavior.
  - [x] Capture renderer media target facts over realized resident resource records.
  - [x] Wire shell media preview command to resident media targets.
  - [x] Wire shell media copy command to resident media payloads.
  - [x] Wire shell media save command to resident media payloads.
  - [x] Verify media admission, preview, copy, and save behavior end to end.
- [x] Checkpoint: status-line view facts are ported and verified from new transcript-owned facts.
- [x] Port status-line view facts from new transcript-owned facts.
  - [x] Define resident status-line facts domain and source-boundary tests.
  - [x] Wire status-line projection to resident host status facts.
  - [x] Verify status-line view facts end to end.
- [x] Run targeted unit and integration tests for each completed behavior slice.

## Checkpoint 8: Final Cleanup

- [x] Checkpoint: the rework is complete, reviewed, and no obsolete transcript architecture remains live.
- [x] Search for live imports of archived source.
- [x] Search for new code using forbidden local APIs.
- [x] Reconcile root, feature, and package docs with the implemented Syndic transcript boundary.
- [x] Run final targeted and workspace verification commands selected by the implementation plan.
- [x] Resolve the reviewer-required storage-backed provider scope mismatch.
- [x] Complete review with a reviewer subagent and address required findings.
