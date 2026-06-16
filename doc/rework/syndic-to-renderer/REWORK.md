# Target Docs

Read these target-state docs before working on this rework:

- `doc/features/transcript/design.md`
- `doc/features/transcript/renderer-architecture.md`
- `doc/features/syndic/design.md`
- `doc/features/syndic/concepts.md`
- `crates/syndic-storage/doc/design.md`

# Cutover Boundary

The current live transcript source is legacy until Phase 2 archives it behind a new compile boundary.

During the incomplete rework, current Beryl shell code may continue to compile against legacy transcript modules only because they have not yet been cut over. New Syndic transcript code must not depend on those modules.

The first live replacement boundary should be a minimal transcript host or crate in authoritative source paths. It should compile with fake or in-memory Syndic data before real `syndic-storage` integration.

Transcript rendering may depend only on a Beryl-facing Syndic provider contract. It must not call `syndic-storage` directly.

Phase 2 source audit found that the live shell does not currently have a removable transcript boundary. `ShellView`, `ConversationSurfaceState`, selected-thread activation, diagnostics, render theme construction, and many tests depend directly on legacy transcript module names and types. The next implementation step must create a new target-state shell-facing transcript surface and move callers to it before legacy source can be archived.

Operator hold: preparation is complete for now. Do not begin live source migration or archival until the operator explicitly resumes this rework.

`doc/plan.md` owns the current executable phase. This file owns the exhaustive rework checklist.

# Reference Snapshot

Archived old docs:

- `doc/rework/syndic-to-renderer/old-doc/features/transcript/design.md`

Old source remains live only until the compile-boundary phase archives it under:

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

- [ ] Checkpoint: first compile-boundary cutover is reviewed and accepted before source migration starts.
- [ ] Reconcile root `doc/design.md` with the target Syndic transcript boundary.
- [ ] Decide whether related feature docs need target updates for composer quote insertion, status-line view state, activity projection, conversation-thread navigation, edit/branch menus, and media actions.
- [ ] Define the minimal live replacement transcript host boundary.
- [ ] Decide temporary behavior while the new host is initially empty or fixture-backed for transcript rendering, selection, quote, branch/edit menus, media actions, status-line view facts, and diagnostics.
- [ ] Identify the first source cutover slice, including owning live module path, shell fields and accessors to replace, tests to retire or rewrite, and verification commands.
- [ ] Define forbidden-import and build verification gates for the first implementation phase.

## Checkpoint 2: Legacy Transcript Removed From Live Build

- [ ] Checkpoint: `beryl-app` compiles without legacy transcript renderer, history, presentation, viewport, Markdown parser, or residency modules in live module trees.
- [ ] Add new live source in authoritative crate/module paths.
- [ ] Replace shell-facing transcript fields and accessors with a new `syndic_transcript` surface before archiving legacy modules.
- [ ] Retire or replace tests that import legacy transcript internals by path before moving source.
- [ ] Move obsolete transcript source snapshots under `doc/rework/syndic-to-renderer/old-crates/beryl-app/...`.
- [ ] Remove legacy transcript modules from live `mod` trees.
- [ ] Verify Cargo manifests and build scripts do not reference `old-crates`.
- [ ] Search for live imports of archived source after source archiving.
- [ ] Search for new code using forbidden local APIs.

## Checkpoint 3: Syndic Provider Contract Compiles And Is Tested

- [ ] Checkpoint: the Beryl-facing Syndic transcript provider contract compiles with in-memory fixtures and contract tests.
- [ ] Define the Beryl-facing Syndic transcript provider contract.
- [ ] Add in-memory fixture provider for new transcript tests.
- [ ] Add contract tests for transcript-view cursor pages.
- [ ] Add contract tests for resident projection records.
- [ ] Add contract tests for resource metadata and range reads.
- [ ] Add invalidation/revision behavior tests.

## Checkpoint 4: Resident Presentation Core

- [ ] Checkpoint: fixture-backed provider data becomes bounded resident presentation snapshots without GPUI rendering.
- [ ] Implement transcript residency over the provider contract.
- [ ] Implement presentation data records with Syndic provenance.
- [ ] Implement budget and fallback diagnostics.
- [ ] Add pure tests for provenance preservation, demand facts, invalidation, budget rejection, and resident release behavior.

## Checkpoint 5: GPUI Host Renders Resident Snapshots

- [ ] Checkpoint: the new GPUI transcript host renders only resident presentation snapshots and reports demand facts without direct storage access.
- [ ] Implement realized frame window and scroll controller.
- [ ] Implement GPUI transcript host over resident presentation snapshots.
- [ ] Implement nested code/table/media demand reporting without direct storage reads.
- [ ] Add targeted render or integration tests for empty, fixture-backed, bounded, and rejected resident snapshots.

## Checkpoint 6: Behavior Port Slices

- [ ] Checkpoint: activation behavior is ported and verified against the new transcript boundary.
- [ ] Port activation behavior.
- [ ] Checkpoint: exact manual scrolling and clamp behavior are ported and verified against resident boundaries.
- [ ] Port exact manual scrolling and clamp behavior.
- [ ] Checkpoint: live autoscroll behavior is ported and verified without legacy viewport state.
- [ ] Port live autoscroll behavior.
- [ ] Checkpoint: selection and Markdown-preserving copy behavior are ported and verified over rendered resident records.
- [ ] Port selection and Markdown-preserving copy behavior.
- [ ] Checkpoint: quote harvesting is ported and verified over rendered resident records.
- [ ] Port quote harvesting behavior.
- [ ] Checkpoint: context menus and edit/branch integration are ported and verified against Syndic provenance.
- [ ] Port context menus and edit/branch integration against new provenance.
- [ ] Checkpoint: media admission, preview, copy, and save behavior are ported and verified through resident resource ranges.
- [ ] Port media admission, preview, copy, and save behavior.
- [ ] Checkpoint: status-line view facts are ported and verified from new transcript-owned facts.
- [ ] Port status-line view facts from new transcript-owned facts.
- [ ] Run targeted unit and integration tests for each completed behavior slice.

## Checkpoint 7: Final Cleanup

- [ ] Checkpoint: the rework is complete, reviewed, and no obsolete transcript architecture remains live.
- [ ] Search for live imports of archived source.
- [ ] Search for new code using forbidden local APIs.
- [ ] Reconcile root, feature, and package docs with the implemented Syndic transcript boundary.
- [ ] Run final targeted and workspace verification commands selected by the implementation plan.
- [ ] Complete review with a reviewer subagent and address required findings.
