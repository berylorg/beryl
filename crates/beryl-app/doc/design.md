# Goals

Own Beryl's GPUI desktop-shell package boundary and compose user-visible features over typed Beryl-home, Syndic, transcript, and Codex App Server services.

Keep independent main-window presentation responsive while process-wide services coordinate exact shared storage health, runtime readiness, catalog state, thread claims, and live execution facts.

## Non-goals

- Owning process entry, CLI bootstrap, or selection of the configured Beryl home.
- Owning the physical Fjall database, home lock, Syndic record schemas, CAS protocol transport, backend process construction, or transcript-provider storage.
- Repeating feature-level product behavior, GUI composition, or system-level consistency policy inside this package boundary.
- Exposing raw Fjall handles, Syndic storage encodings, backend JSON, CAS historical transcripts, or GPUI-owned state across public package APIs.
- Retaining workspace, workspace-member, semantic-graph, checklist, graph-upkeep, checklist-bound decision, recursive column-selector, pending-new-thread, or compatibility-adapter models.

# Decisions

## Authority And Dependencies

- Root `doc/design.md` and the linked feature, system, GUI, and package documents own behavior consumed by this crate.
- This crate is the Beryl package that depends on `gpui` and assembles the OS windows and slots declared by `doc/gui/integration.md`.
- It consumes pure shared values from `beryl-model`, typed home/domain services from `beryl-home-store` and domain packages, normalized backend services from `beryl-backend`, and the transcript host/provider boundaries from the transcript-presentation and Syndic systems.
- Product policy remains in feature docs, cross-package durability and execution policy remains in system docs, and stored representation remains in the owning storage package.
- App-local orchestration types may correlate those boundaries, but they must not become a second durable authority or a compatibility facade over removed models.

## Process Shell Services

- One process-shell service graph is created after the configured Beryl home opens successfully.
- Shared services include typed Beryl-home domains, store-health publication, the runtime/root registry, managed runtime supervisors, the complete compact thread catalog, settings and theme repositories, durable orchestration-job coordinators, and bounded process-wide worker pools.
- Shared services publish versioned facts and command outcomes. They do not retain references to individual GPUI views or mutate window-local presentation directly.
- A busy-home startup creates only the compact busy-home window declared by GUI integration and does not create the ordinary process-shell service graph.
- An unreadable startup store creates no ordinary main conversation window from unvalidated state.

## Main Window Ownership

- Each main conversation window owns one `WindowId`, one exact selected-thread claim, its local navigation history, focus, flyouts, transient menus, selection, editor projection, transcript host, activity/status presentation, notices, and window geometry.
- No main-window controller or GPUI entity is shared between windows.
- Shared facts such as store failure, runtime readiness, catalog revisions, and thread occupancy are independently projected into every affected window.
- Window-local commands carry exact window, thread, draft, revision, runtime, root, CAS, turn, or job identities required by their owning system. A stale result is rejected instead of being applied to the currently visible window by coincidence.
- Ordinary close, dedicated Exit, unexpected termination, restoration, virtual-desktop placement, and additional-window acquisition delegate durable state changes to the main-window and Beryl-home contracts.
- Settings and busy-home windows are separate top-level controllers and never receive main-window thread claims or session-restore records.

## Startup And Progressive Readiness

- Before ordinary windows become visible, this crate consumes the validated minimal session discovery result and each restored window's validated selected-thread/current-draft seed. An empty restore set first produces either one atomic most-recent-runtime/root empty-thread acquisition or the one permitted zero-runtime threadless seed.
- The first visible ordinary surface is the final main conversation shell. There is no app-local loading-window state machine.
- Catalog, selected transcript, and runtime/CAS readiness are independent versioned inputs. Their feature-owned controls become available independently without rebuilding the outer shell.
- The initial zero-runtime shell is the only threadless main-window state. Once a runtime exists, window creation and activation require an exact claimed Syndic thread before publication.
- Existing coherent title, draft, transcript, focus, and viewport state remains published while a later activation is prepared. One activation transaction replaces them only after the target claim, draft, transcript seed, and revisions are ready.

## Beryl-Home Integration

- This crate never opens Fjall or reads raw keyspaces. It receives typed domain handles and repository services from the Beryl-home boundary.
- Draft flush, input admission, thread/draft creation, runtime/root creation, claims, generated-title mutation, automatic branch-discussion archive mutation, session update, settings update, asset reference, CAS-binding transition, and handoff-job transition use typed revision-checked commands.
- Correctness-sensitive success is not published until the home-store command reports the required durability barrier complete.
- Store-health transitions invalidate outstanding mutation authority by revision. Persistent failure preserves existing window controllers and their last coherent in-memory presentation while feature-owned gates reject further store-dependent work.
- Reopen rebinds services only to the same validated home generation; it never creates a substitute home, imports old state, or treats cached presentation as durable proof.

## Catalog, Threads, And Drafts

- The process holds one complete compact Beryl-home catalog snapshot and exact search/scope indexes without loading draft text, turns, transcript items, Markdown, or resource bytes.
- Main windows derive filtered recent-first row identities from that snapshot. GPUI creates only the visible fixed-height rows and bounded overscan required by the conversation-thread GUI contract.
- Catalog construction and refresh never enumerate CAS threads, read CAS historical transcripts, or use backend names and working directories as Beryl metadata.
- Thread creation, pristine-thread reuse, current-thread no-op, first-runtime creation, additional-window acquisition, selection, and release use atomic home-store/Syndic claim operations rather than app-local check-then-act logic.
- The selected composer is an editor projection over the exact durable current draft. Dirty revision tracking, timed autosave, flush barriers, activation, replacement-edit intent, and acceptance call typed Syndic/home services rather than persisting a window buffer.
- Thread activation keeps the prior coherent selection until the target is ready and publishes title, lineage, draft, composer-history scope, transcript seed, runtime/root memory, and claim transition together.
- The application exposes no manual title, pin, archive, delete, runtime/root removal, or thread-rebind command path.

## Runtime And Live Execution Orchestration

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while availability and errors are projected per window.
- This crate requests normalized backend sessions and capabilities through `beryl-backend`; it does not construct commands, parse JSON-RPC, inspect backend storage, or own CAS schema compatibility.
- Accepted input and exact selected-path proof enter the CAS-live Syndic system, which owns binding validity, CAS-native lineage precedence, one-time recovered-history injection, live capture, same-thread operation gates, and durable incomplete-turn outcomes.
- App-local active-turn presentation keeps only the bounded exact identities and state needed for controls and routing. It is not selected transcript history or CAS-binding authority.
- Stop, hard stop, compaction, steering, queue delivery, replacement edit, and retry commands name exact targets and flow through their owning operation gate. This crate never guesses a process, turn, parent, or rollback scope.
- Automatic thread-title maintenance uses a bounded background backend session and commits only validated generated Beryl metadata. It never occupies a selected foreground stream or exposes maintenance CAS threads as user threads.

## Branch Discussion And Durable Jobs

- `Discuss in new branch` consumes exact selection provenance from the transcript host and calls the atomic Syndic/home branch-creation command before activating the result.
- Branch creation performs no CAS request; its context-bearing draft and parent binding come from durable target-system records.
- Selected-discussion activation supplies the transcript host with the immutable context-owner descriptor and branch insertion parent; transcript residency reads the exact envelope and publishes the synthetic context group without creating a turn.
- Discussion-scoped resolution tool calls cross a bounded request/response bridge to the branch-handoff coordinator. Turn workers receive structured outcomes and never hold direct GPUI, store, repository, or main-window handles.
- Resolution admission, queued-input deferral, composer gating, parent ordering, retry, idempotency, recovery, and archive publication are projections of the durable handoff system, not app-local flags.
- One discussion revision publishes the composer-adjacent discussion-status strip and composer writable or inert state together so presentation cannot disagree about whether input is accepted.
- App-local presentation may invoke retry only for an already admitted exact job and may not synthesize a resolve, merge, archive, parent, or replacement destination.

## Image Assets

- Image paste, preview, clipboard reconstruction, label allocation, submission preparation, transcript resolution, and unavailable-state presentation consume the image feature and system boundaries.
- The app package owns GPUI editor atoms, marker menus, preview-window state, and correlation of asynchronous preparation results to exact draft and asset revisions.
- Content addressing, bytes, durable references, Host/WSL projection, and cleanup policy remain outside this crate's presentation state.
- Image preparation and decode run away from the GPUI thread under deterministic item and byte budgets. Stale completions cannot update another draft, marker, preview, runtime, or transcript row.

## Transcript Host And Rendering

- Main-window transcript code interacts with one transcript host through the shell boundary defined by `doc/systems/transcript-presentation/shell-boundary.md`.
- The app package does not call `syndic-storage` directly, retain full-history clones, derive transcript narrative from CAS reads, or let renderer callbacks initiate authoritative history transitions.
- Activation seeds, provider responses, live events, renderer demand, selection, quote, context-menu targets, media commands, and scroll commands retain stable Syndic provenance and presentation revisions.
- Anchor-relative chunk loading, resident-data release, row virtualization, huge-turn streaming, media admission, nested scrolling, selection pins, and measurement caches remain bounded by the transcript-presentation contracts.
- Render, prepaint, deferred-frame, scrollbar, and status paths consume prepared snapshots and facts; they do not parse full Markdown, scan resident history, decode media, query storage, or call the backend.

## Activity, Status, Notices, And Notifications

- Activity and status are bounded presentation projections over normalized exact backend events, Syndic facts, Beryl metadata, and transcript-host facts defined by their feature contracts.
- Operational activity never becomes transcript narrative or durable conversation authority.
- Status chrome does not estimate token usage, context limits, rate limits, turn counts, or active targets. Unknown exact facts remain unknown.
- Surface notices are bounded window-local queues. Dismissal changes presentation only and cannot acknowledge, retry, repair, or mutate the underlying failure by itself.
- End-turn sounds and operator-attention signals consume exact foreground-turn and lifecycle eligibility; maintenance, metadata, restore, catalog, compaction-continuation, and background work do not masquerade as ordinary user-turn completion.

## Settings And Themes

- The settings window consumes the app-neutral settings-window package through Beryl adapters and mounts the sections defined by the settings and theming feature docs.
- Scalar settings drafts validate and commit through typed Beryl-home settings commands. Unapplied drafts remain window-local and never mutate active state.
- Theme documents, role resolution, preview, install, update, activation, and editor presentation use the theming feature boundary and theme repository; the app package owns only GPUI integration, cache invalidation, and bounded UI bridges.
- Theme and settings operations cannot reach Syndic history, runtime/root/thread metadata outside their declared setting, backend-owned Codex configuration, or unrelated Beryl-home domains.
- No graph, checklist, workspace-member, or removed-surface settings adapter remains in this package.

## Dynamic Tools And Lifecycle Yield

- Beryl-owned dynamic tools are registered only for the exact CAS projections and feature scopes that authorize them.
- Normalized tool calls cross bounded typed bridges keyed by exact CAS thread, turn, call, and Beryl/Syndic target revisions.
- Tool workers never retain `ShellView`, GPUI handles, window controllers, raw repositories, or storage mutation handles.
- Lifecycle yield, branch resolution, theme, settings, and future tool families each keep their feature-owned schema and authorization. A generic tool bridge does not combine their permissions.
- Tool responses report durable admission, rejection, deferral, conflict, or bounded failure accurately; request acceptance is not turn completion or downstream job completion.
- Secret-like values and unbounded content are rejected or redacted before diagnostic retention.

## Diagnostics And Isolated Child Control

- Supervisor diagnostics expose bounded content-free process, memory, renderer, retained-state, settings-window, transcript-frame, media, and catalog summaries through explicit snapshot builders.
- Diagnostic reads never require loading nonresident conversation history, rendering hidden rows, scanning full catalogs on the GPUI thread, or querying CAS history.
- Isolated-child controls dispatch through the same exact Beryl-home window, thread, composer, stop, popup, scroll, and activation command paths used by direct interaction.
- Child control requests use exact child-known ids and expected state, reject ambiguity or stale targets, and never mutate private state behind those command paths.
- Diagnostic retention is bounded by record count and bytes and excludes transcript text, draft text, titles, root paths, search text, credentials, capability tokens, raw tool payloads, and other user content unless a separately authorized diagnostic contract explicitly requires a redacted value.

## Concurrency And Responsiveness

- The GPUI thread performs no blocking filesystem, Fjall, process, transport, protocol, history, Markdown, image, persistence, or model work.
- Background work is keyed by exact durable identity plus revision or cancellation generation. Completion applies only when every target fact still matches.
- Correctness-sensitive commands use revision checks and short typed commits, not long-lived locks across external work or await points.
- Worker pools, channels, retry sets, title jobs, tool requests, backend notifications, activity rows, notices, catalog projections, transcript caches, media, and diagnostic rings have deterministic count and byte bounds.
- Foreground turn streaming, visible transcript demand, draft persistence, and exact user commands take priority over speculative preload, title generation, metadata decoration, catalog maintenance, and diagnostic work.
- Rendering exhaustive thread/root/runtime collections uses stable fixed-height virtualized rows with bounded overscan, stable identities, focus and tooltip preservation, and content-free diagnostics.
- Quiet backend streams are not failures, and ordinary bounded request timeouts do not impose an inactivity timeout on live turns.

## Dependency Boundary

- This crate may depend on `gpui`, Beryl widget/adaptor packages, `beryl-model`, `beryl-home-store`, Beryl metadata-domain packages, `syndic-storage` only through higher-level Syndic service boundaries, `beryl-backend`, and transcript-host/provider packages as allowed by their own designs.
- Renderer-facing modules must not depend directly on `syndic-storage`, `beryl-home-store`, `beryl-backend`, Fjall, or raw app-server protocol types.
- Storage, backend, and provider workers return typed bounded results that contain no GPUI entities.
- Cycles are prevented by keeping pure identities in `beryl-model`, physical-store ownership in `beryl-home-store`, Syndic records in `syndic-storage`, backend protocol integration in `beryl-backend`, and shell composition in this crate.
