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
- Its direct `sha2` dependency computes the exact SHA-256 identity of the canonical serialized
  conversation-tool registry. The digest is durable profile correlation, not an authorization or
  secrecy primitive.
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
- Cancellation may withdraw queued draft persistence only before writer admission. An admitted save is drained; a surfaced post-admission storage or persistence failure suspends its exact draft binding until same-home verification or recovery and coherent current-draft reconciliation establish the durable revision and payload.
- Store-health transitions invalidate outstanding mutation authority by revision. Persistent failure preserves existing window controllers and their last coherent in-memory presentation while feature-owned gates reject further store-dependent work.
- Reopen rebinds services only to the same validated home generation; it never creates a substitute home, imports old state, or treats cached presentation as durable proof.

## Catalog, Threads, And Drafts

- The process holds one complete compact Beryl-home catalog snapshot and exact search/scope indexes without loading draft text, turns, transcript items, Markdown, or resource bytes.
- Main windows derive filtered recent-first row identities from that snapshot. GPUI creates only the visible fixed-height rows and bounded overscan required by the conversation-thread GUI contract.
- Catalog construction and refresh never enumerate CAS threads, read CAS historical transcripts, or use backend names and working directories as Beryl metadata.
- Thread creation, pristine-thread reuse, current-thread no-op, first-runtime creation, additional-window acquisition, selection, and release use atomic home-store/Syndic claim operations rather than app-local check-then-act logic.
- The selected composer is an editor projection over the exact durable current draft. Dirty revision tracking, timed autosave, flush barriers, activation, replacement-edit intent, and acceptance call typed Syndic/home services rather than persisting a window buffer.
- Draft persistence retains exact binding, edit, request, and timer generations around one in-flight save. A stale completion cannot clean a later edit; a lifecycle flush drains the in-flight save and, when necessary, chains the latest edit only after exact receipt or recovered-state reconciliation.
- A save executor prepares the exact chunk manifest away from GPUI, resumes or creates its content-addressed building object, appends only bounded chunk batches, and finally publishes the sealed content reference with the draft revision. Intermediate durable chunks do not change the visible or durable current draft.
- Draft-save publication consumes one opaque executor-issued completion bound to the full request identity, including home generation, thread, draft, expected revision, payload, timestamp, and scheduling generations. A caller-constructed status value or another service's numerically similar generations cannot publish success.
- Input admission resolves each draft image marker through the exact Beryl-state reference, verifies marker identity and final label against the draft, and assembles the typed Syndic admission plus bounded per-marker asset-reference move into one home command. Text-only admission uses the same path with an empty move set.
- Replacement-edit start verifies that every Syndic target marker has one matching historical submitted-item asset reference, retains that historical owner, and adds a current-draft marker owner in the same home command as the Syndic edit. Cancellation keeps the copied payload and its draft references because it clears only replacement intent.
- A surfaced post-admission failure retains the editor and reconciles the exact Syndic admission snapshot plus every expected source and destination asset-reference owner before publishing acceptance or rejection. Exact means the caller-named replacement draft and advanced input gate agree too; the app never treats a partial cross-domain match or blind mutation replay as success.
- An in-flight save or reconciliation rearms autosave only while its captured timer generation remains current. A newer committed autosave-setting publication retains its interval, record revision, generation, and publication-time deadline anchor.
- Thread activation keeps the prior coherent selection until the target is ready and publishes title, lineage, draft, composer-history scope, transcript seed, runtime/root memory, and claim transition together.
- The application exposes no manual title, pin, archive, delete, runtime/root removal, or thread-rebind command path.

## Runtime And Live Execution Orchestration

- Runtime readiness is process-wide and coalesced by exact runtime/root demand while availability and errors are projected per window.
- This crate requests normalized backend sessions and capabilities through `beryl-backend`; it does not construct commands, parse JSON-RPC, inspect backend storage, or own CAS schema compatibility.
- Accepted input and exact selected-path proof enter the CAS-live Syndic system, which owns binding validity, CAS-native lineage precedence, one-time recovered-history injection, live capture, same-thread operation gates, and durable incomplete-turn outcomes.
- Every non-GPUI projection coordinator is bound to one exact healthy Beryl-home generation. Its
  operation enters the process-wide flight registry keyed by Beryl-home id, healthy generation, and
  Syndic thread, so separately constructed coordinators cannot duplicate same-thread remote work.
  Different homes, generations, and threads remain independent, and no registry mutex is held
  during storage or backend work.
- One process-owned connection service owns each exact initialized backend client on which
  compatibility was probed plus its connection, runtime, and managed-process generations. A
  report supplied by another caller, connection, runtime, or generation cannot authorize recovery
  injection.
- The connection service rejects any initialized client that opted out of foreground stream
  notifications. One bounded non-GPUI worker exclusively owns each admitted client, serializes
  request and unsubscribe commands, polls the stream only while idle, and routes buffered
  pre-response events before releasing the matching response to its caller.
- Worker commands receive only a request-capability view with the admitted lineage and unsubscribe
  operations. They cannot poll or drain the backend stream; event consumption remains structurally
  exclusive to the connection worker.
- An approval server request interleaved before another response is denied immediately, retained in
  the same bounded incoming FIFO, and routed to its exact turn target before the original response
  becomes caller-visible. Denial does not erase the operational event, and the routed request
  explicitly distinguishes that completed automatic denial from an idle approval whose response is
  still required.
- The worker carries the exact backend operation result separately from a subsequent buffered-event
  routing failure. Whole-connection routing failure retires the connection and gates ordinary
  result publication; it does not erase dispatch evidence or turn an observed non-idempotent
  outcome into non-dispatch.
- A target-local buffered routing failure revokes only that exact target and gates the matching
  command result with the target identity and close reason. It does not falsely report connection
  retirement or let another target's ordinary request success escape the ordering boundary.
- Each connection worker accepts at most 64 queued commands and uses a 20-millisecond idle poll.
  A quiet poll advances content-free diagnostics but keeps the worker and every target active.
- Each active execution owns one non-cloneable bounded event target tied to the exact connection,
  loaded-session generation, CAS thread, Syndic owner, and one-way CAS-turn correlation. Target
  overflow, receiver abandonment, identity conflict, or connection retirement is explicit and
  cannot redirect an event to another thread or generation.
- Once an abnormal target close retires a remote thread lane, that CAS thread cannot register a
  replacement target on the same connection because wire events contain no local loaded-session
  generation. Each connection remembers at most 256 such lanes; needing another fence retires the
  connection instead of forgetting an older fence. A later proven-terminal reuse path must retain
  the same loaded authority and establish its own ordered handoff rather than clearing this fence.
- One target retains at most 256 normalized events and 16 MiB of approximate parsed event bytes.
  Exceeding either bound closes that target and revokes its exact loaded generation. Valid events
  without a target are counted but not retained; malformed routing identities retire the whole
  connection rather than being guessed, rewritten, or offered to another target.
- Exact account snapshots and bounded connection-lifecycle facts publish through one shared
  projection keyed by runtime id and managed-process generation. Every live connection to that
  process observes the same latest account fact with its exact source connection generation;
  per-turn consumers cannot steal or consume those shared facts.
- The service issues non-cloneable per-thread subscription leases from one process-wide generation
  allocator and retains only live lease entries. A recovered binding is usable or forkable only
  through the exact owning connection while its complete stored loaded generation matches; a
  second connection to the same process is not equivalent authority, and native resume does not
  promote a lost recovered injection into durable CAS history.
- Connection-wide retirement and the retired-check-plus-registry-acquisition section linearize
  through one bounded in-memory gate. An acquisition that linearizes first is removed by retirement;
  an acquisition that linearizes later is rejected. The gate covers no backend or storage work, and
  a retired connection cannot leave or insert a loaded-thread entry.
- Consuming release revokes the local lease before connection-scoped unsubscribe. Every response
  or ambiguous failure remains locally non-authorizing, and connection/process loss revokes all
  matching live leases in a bounded walk without waiting for delayed CAS unload notification.
- Dropping a lease never performs backend I/O. It revokes the exact local token immediately; if it
  was the last token, the connection is retired so forgotten cleanup cannot block GPUI or leave
  reusable authority. The explicit consuming release is the non-GPUI path that performs bounded
  unsubscribe.
- Native continuation, resume, inclusive fork, and fresh lineage are selected from bounded Syndic
  proofs before recovery. A nonempty earlier prefix creates a distinct fork through its exact
  terminal CAS turn; an empty prefix starts fresh. The coordinator never dispatches in-place
  rollback. One-time injection starts its own fresh empty thread inside the coordinator, so a
  caller cannot supply a merely fresh fork result as an injection target.
- The coordinator retires a native source only from an authoritative source-loss verdict. A
  source-preserving or unclassified resume/fork rejection retains the binding and enters bounded,
  coalesced retry against the same source; retry exhaustion returns an exact revision-bound
  recovery-decision capability and never silently selects injection. Explicit retry keeps the
  source, while explicit recover bypasses it for one fresh injected target projection. Recovery
  retires the source only when it is the target thread's own binding; another thread's fork source
  remains unchanged. After target retirement, bounded replanning must observe precisely the single
  binding revision produced by that retirement; a concurrent later mutation makes the consumed
  decision stale rather than widening its authority.
- Durable Syndic projection requests accept persistent CAS thread options only. Ephemeral
  maintenance work uses a separate backend workflow and never receives a durable Syndic binding.
- A native resume publishes establishment provenance for the exact durable source prefix it
  loaded. A later selected-path revision with the same stable tail and digest belongs to the new
  binding's represented-prefix fact, not to a fabricated newer CAS establishment event.
- Cancellation is checked at safe boundaries before remote dispatch. A synchronous request that
  has already been sent is drained and classified; its result is durably published or abandoned
  before cancellation can become caller-visible.
- A remote projection becomes caller-usable only after exact durable binding publication. Every
  failed or ambiguous fresh target is forgotten from the loaded registry and retained as stale
  provenance when its identity can still be committed; Beryl neither deletes it nor retries
  injection against it.
- Recovered-injection completion authority uses the app's local Unix wall-clock observation taken
  immediately after exact CAS success. A clock before the epoch or outside the durable range fails
  explicitly and abandons the target; the earlier request timestamp is never relabeled as
  injection completion.
- A surfaced publication ambiguity never returns a projection capability. If same-home
  verification later reveals a whole recovered binding whose process-local loaded generation was
  forgotten, the coordinator performs another fresh injection rather than resuming or reinjecting
  the ambiguous target.
- Until the branch-discussion checkpoint supplies its separate selected-context projection proof,
  this coordinator rejects a context-bearing pending turn explicitly. It never establishes a CAS
  projection that omits the required selected assistant passage or conflates it with recovered
  history.
- App-local active-turn presentation keeps only the bounded exact identities and state needed for controls and routing. It is not selected transcript history or CAS-binding authority.
- Active-turn orchestration retries only work proven not dispatched or exactly rejected. A possibly
  dispatched start or steering request with no authoritative response is durably converged through
  the CAS-live system as incomplete or delivery-unknown, retires the unprovable projection, and is
  never replayed automatically. Relaunch and fresh projection recovery restore readiness without
  starting a replacement model turn.
- Cancelling an exact not-started activation does not imply that its loaded projection survived.
  The orchestration result distinguishes a retained exact projection from a still-pending turn
  whose connection authority must be reacquired. Matching start evidence followed by target loss
  remains completion-unknown and cannot be downgraded to an identity conflict or retry proof.
- Ordinary execution pages the pending turn's one sealed user item through bounded Syndic reads,
  constructs the one CAS-owned input string required by `turn/start`, activates the exact durable
  binding, and consumes the loaded projection into one non-cloneable live target. A routed matching
  start event is itself identity proof; a response-derived turn id must confirm the target before
  live activation is claimed.
- Live capture exhaustively maps the closed normalized item union into borrowed typed provider views;
  it never reduces a public item to generic text or a fieldless activity marker. It streams one
  bounded provider-field fragment at a time into Syndic's item-owned `ProviderItemV1` content,
  answers dynamic tools through the exact target, and preserves every published prefix on loss.
- Exhaustive capture begins after backend ingress exclusions. A standalone image-generation view
  contains `savedPath` and non-binary lifecycle metadata but never the upstream base64 `result`;
  app orchestration cannot persist, inspect, decode, or recover from that discarded field.
- The submitted `UserMessage` lifecycle is correlated against the exact already durable input rather
  than duplicated; its provider metadata and checked content reference remain typed. Completion-only
  variants such as pinned `SubAgentActivity` are admitted only through their explicit typed path and
  retain their complete public payload. Unknown, malformed, or unsupported history-relevant items
  preserve the exact provider terminal fact but prevent a history-complete publication through a
  typed incomplete reason.
- MCP and dynamic-tool structured values cross the app boundary through the closed typed storage
  algebra, not raw JSON, opaque bytes, or a generic catch-all. Provider completion is reconciled
  field-by-field against bounded durable reads; unchanged field ranges are reused and only changed
  bytes are appended, so final snapshots do not duplicate large streamed output.
- Terminal capture therefore publishes the exact normalized provider outcome and the independent
  typed history-incomplete reason together. It never converts provider `Complete` into local
  `Incomplete` merely because canonical history remains unresolved.
- Every item delta carries its expected normalized kind. Capture validates the exact CAS item and
  kind plus its closed field identity, element ordinal, and protocol index before durable mutation,
  so an agent, plan, command, file, reasoning, or tool delta cannot append to another variant or
  field.
- Proven terminal handoff first closes canonical source admission and advances the valid binding,
  then resumably builds and finalizes each visible item projection and selected transcript from
  already durable canonical history.
- Live capture retains only one bounded pending provider-field fragment. It resolves item identity,
  lifecycle, kind, typed frame frontier, and exact logical-text prefix from Syndic's
  record-stabilized CAS-item/canonical-item/content reads; active-item and completed-item maps are
  forbidden. Pinned `turn/completed` is a status-only fence, not a full-item snapshot: capture flushes
  the pending fragment and audits every already admitted durable item through fixed-size cursor pages
  before allowing `TurnEnded`. Every completed item must name an exact sealed final typed frame whose
  kind, structure, field ranges, and content frontier are complete. Capture neither invents
  terminal item backfill nor treats idle state as completion proof.
- Foreground stream loss retires the exact target and converges the retained prefix as incomplete;
  app orchestration never reconnects or resumes a replacement connection into that same capture.
  CAS history reads and a later terminal notification are not notification replay or repair.
- Ordinary-turn publication and convergence use writer-admitted single-domain commands carrying
  exact logical record fences. Preflight and convergence reread only their exact thread/item/build
  anchors, so activity on another Syndic thread cannot conflict or starve the current operation.
- Stop, hard stop, compaction, steering, queue delivery, replacement edit, and retry commands name exact targets and flow through their owning operation gate. This crate never guesses a process, turn, parent, or lineage scope.
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
- Generated-output admission reads only the exact normalized `savedPath` through its retained
  runtime/process/session provenance. It never asks the backend for the discarded base64 result or
  reconstructs it from CAS history. Missing, unreadable, changed, or unsupported output leaves the
  generated-media resource unavailable or pending according to the image-asset system contract.

## Transcript Host And Rendering

- Main-window transcript code interacts with one transcript host through the shell boundary defined by `doc/systems/transcript-presentation/shell-boundary.md`.
- The app package does not call `syndic-storage` directly, retain full-history clones, derive transcript narrative from CAS reads, or let renderer callbacks initiate authoritative history transitions.
- Activation seeds, provider responses, live events, renderer demand, selection, quote, context-menu targets, media commands, and scroll commands retain stable Syndic provenance and presentation revisions.
- The exact routed live event path supplies bounded transcript-visible text deltas to the transcript
  host independently from durable Syndic coalescing. The host publishes all deltas available to
  the next GUI frame without a character-reveal timer and relinquishes its transient suffix only
  after exact durable-prefix reconciliation; it never retains a second whole response.
- Anchor-relative chunk loading, resident-data release, row virtualization, huge-turn streaming, media admission, nested scrolling, selection pins, and measurement caches remain bounded by the transcript-presentation contracts.
- Render, prepaint, deferred-frame, scrollbar, and status paths consume prepared snapshots and facts; they do not parse full Markdown, scan resident history, decode media, query storage, or call the backend.

## Activity, Status, Notices, And Notifications

- Activity and status are bounded presentation projections over normalized exact backend events, Syndic facts, Beryl metadata, and transcript-host facts defined by their feature contracts.
- Activity-panel presentation is never durable conversation authority. Exact supported command or
  file output may separately enter Syndic as provider-sourced canonical operational history, but
  it remains outside transcript narrative and is not reconstructed from presentation rows.
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

- Every persistent Beryl conversation lineage starts with one canonical versioned,
  deterministically ordered conversation-tool registry. Native continuation, resume, and fork are
  eligible only when their binding proves that same registry profile; Beryl never silently varies
  tool definitions by thread kind or later reconstructs an otherwise native lineage merely to add
  a feature tool.
- Registry membership advertises stable capabilities and does not grant mutation authority.
  Feature-owned handlers authorize each call from its exact CAS thread, turn, call, Syndic target,
  feature state, and durable revisions; a registered tool invoked outside its feature scope rejects
  without mutation.
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
