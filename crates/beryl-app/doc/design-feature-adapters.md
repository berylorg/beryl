# Feature Adapters

This supplement is normative only for its bounded beryl-app feature-adapter role and is governed by
[design.md](design.md). It does not independently declare engineering rigor.

## Adapter Rule

- Adapters translate exact typed service facts and commands into bounded GPUI or tool-worker
  projections. They do not define visible policy, stored representation, system lifecycle policy,
  or dependency-private mechanics.
- Every asynchronous result is fenced by exact home/service generation and applicable feature,
  window, thread, draft, turn, asset, request, presentation, and publication revisions. Stale
  results are discarded and release capacity.
- Renderer-facing adapters consume prepared bounded facts only and perform no blocking storage,
  filesystem, backend, decode, history, or model work on the GPUI thread.

## Discussions, Images, And Transcript

- Branch-discussion adapters transport exact selection provenance, context-owner descriptors,
  durable-job commands, and structured tool outcomes. Workers receive no GPUI, store, repository,
  or window handles, and the adapter invents no resolve, archive, parent, retry, or replacement
  authority.
- Image adapters bind bounded GPUI atoms and preparation results to exact draft, marker, asset, and
  presentation revisions. They do not become byte, sidecar, reference, label, decode, or recovery
  authority. Visible behavior is in the
  [image-assets feature](../../../doc/features/image-assets/design.md) and durability in the
  [image-assets system](../../../doc/systems/image-assets/design.md).
- Each conversation surface uses one transcript host through the
  [transcript shell boundary](../../../doc/systems/transcript-presentation/shell-boundary.md).
  Adapters accept activation seeds, bounded live fragments, demand facts, and narrow commands and
  expose only prepared snapshots.
- Transcript adapters never call `syndic-storage`, retain full history, derive narrative from CAS,
  or let renderer callbacks initiate history transitions. Retirement cancels demand, rejects late
  results, and releases bounded state under the
  [transcript-presentation system](../../../doc/systems/transcript-presentation/design.md).

## Activity, Status, Notices, And Audio

- Activity and status adapters expose stable revision-bound pages and statically bounded facts.
  They materialize no complete activity history, backend bucket map, raw command, CAS history, or
  provider aggregate.
- One runtime-activity-period identity scopes process-wide activity across turns and switches.
  Runtime teardown, replacement, restart, or same-home replacement ends it; late facts cannot enter
  the next period.
- Notice adapters accept bounded typed records and exact eligibility and route them to the
  [notifications feature](../../../doc/features/notifications/design.md); they do not choose
  treatment, persistence, dismissal, or sound eligibility.
- One process audio lane owns at most one active open/read/decode/playback attempt and one latest
  waiting metadata-only event. It reserves configured encoded and decoded bytes before acquisition,
  moves charges with resources, and releases all handles, buffers, work, and charges on every
  failure, cancellation, replacement, terminal, and disposal cut.

## Settings And Themes

- The settings adapter hosts the app-neutral settings window, sends caller-validated scalar
  mutations through typed home commands, and returns exact outcomes. Draft, validation, Apply/OK,
  and visible result policy stays in the
  [settings feature](../../../doc/features/settings/design.md).
- The theme adapter consumes typed theme service and assembles the process-wide appearance/preview
  coordinator, GPUI window adapters, exact window-set publication, cache invalidation, and bounded
  UI/tool brokers from the
  [theme-runtime system](../../../doc/systems/theme-runtime/design.md).
- Repository parsing, serialization, watching, mutation, and durability stay outside. The app
  retains only bounded manifest pages, finite resolved appearances, exact publication identities,
  and typed reconciliation or retry custody.
- One atomic window-set barrier publishes a complete appearance generation. Rejection or stale
  identity preserves the prior appearance. Replacement transfers no cursor, subscription, preview,
  observation, reconciliation descriptor, or publication authority.

## Discussion Creation Adapter

- The non-GUI creation operation owns one exact captured request, stable child/draft identities,
  timestamp and typed Syndic source witness. Its admission uses the configured bounded operation
  capacity and never retains more than the contract's selected-text limit per admitted operation.
  Cancellation before commit drops unpublished preparation; it cannot cancel a committed creation.
- Process-owned operation capacity survives home-service replacement. Retained audit evidence shares
  the same bounded operation slot and immutable selected text, without retaining an old home service.
  Preparation captures the process admission epoch; execution holds that admission through commit
  outcome handling and any synchronous registry handoff. Shutdown/reopen cannot revive old preparation.
- The app composes Syndic creation with the exact unclaimed Beryl catalog row in one `SyncAll`
  command. No session/window claim or backend request belongs to that command. Exact created
  outcome returns the same discussion identity for later ordinary activation; activation failure
  neither repeats creation nor deletes the discussion.
- Reconciliation combines only the exact Syndic creation closure and catalog row under stable
  typed revisions. An indeterminate outcome transfers custody synchronously to the sole home
  registry before responding or cancelling. Exact old grants ordinary noncommit handling; exact
  new reconstructs the committed creation result; partial or conflicting state stays unavailable.
  Evidence retained before execution shares the exact attempt's disposition and installed registry
  handle. Visible natural records alone cannot turn an unresolved indeterminate attempt into success;
  the audit must join that scope and honor pending or collision outcomes before reporting creation.
  GUI busy state, selection presentation and activation retry remain feature-owned.

## Discussion Resolution Admission

- The non-GUI admission boundary consumes the broker's exact discussion, resolving turn and CAS
  request identity under the matching home and projection-service generations. Its scoped live
  command remains owned through durable execution and the caller's typed outcome publication or
  custody transfer. The ordinary broker remains the sole correlated response owner; lost response
  delivery cannot roll back admission or authorize a replacement attempt.
- Historical request lookup precedes fresh-attempt eligibility and returns the original job even
  after terminal completion or a later attempt. A different request while live preserves the
  original payload. Fresh admission derives immutable child/parent/context facts and the parent
  accepted-input frontier from bounded typed reads. Future-turn input defers without mutation;
  archived discussions or parents reject fresh admission.
- One home command composes the prepared State admission with Syndic's pending discussion gate.
  Original handle, domain and home revisions fence preparation; Syndic repeats the active CAS,
  archive and empty-queue checks at the writer. No candidate path admits a new resolution.
- Process operation custody retains uncertain audits in their existing bounded reconciliation
  slots even if a response handler drops its copy. Exact-job lookup recovers that audit; its slot
  stays unavailable until natural State/Syndic evidence and the home registry agree. Collision
  remains unavailable. Retained audits hold no old home service; fresh candidate handles can
  reconcile them. The same custody rule applies to subsequent handoff settlements.

## Discussion Parent Admission

- Parent delivery is a non-GUI handoff operation owned by the process coordinator and its exact
  home generation. A configured reconciliation slot owns one job, stable generated input/turn/item
  identities, prepared sealed visible text and bounded exact outcome witnesses. Input id derives
  from the admitted job; turn/item ids and timestamp are captured once. The State resolution is
  the only payload source; no later tool response, selection, draft or window supplies replacement
  content. Preparation follows the handoff system's exact visible-text contract.
- Ordinary admission reads one exact waiting-parent job, asks Syndic for current parent eligibility,
  and returns waiting without admission when temporary gates remain. A ready result joins one
  Syndic generated-input mutation with the exact State starting-parent transition in one home
  command. State witness identity and resolution agree with the Syndic receipt and sealed content;
  mismatches reject preparation. Process admission remains held through writer outcome and any
  synchronous registry handoff. The normal provider dispatcher remains a later owner.
- An archived parent joins one Syndic proof-checked child-gate release and the State terminal
  `ParentArchived` transition. The same operation can converge an already-admitted pre-append job
  through explicit candidate access. Candidate convergence never prepares or creates a new parent
  input, starts CAS, or transfers ordinary ready work into publication.
- Audit evidence shares the admitted slot and retains no old service reference. Uncertain outcome
  installs its exact home reconciliation scope before returning, and no parent dispatch or tool
  success is authorized until both participants and that registry scope agree. Exact old grants
  noncommit handling; exact new returns the same immutable parent identities or terminal failure;
  partial or conflicting evidence stays unavailable. Candidate reconciliation uses fresh handles
  and the same bounded closures. Cancellation and service replacement cannot revive old prepared
  commands; cancellation never rolls back committed parent input.
- Each slot retains at most the admitted resolution and one canonical visible-text preparation,
  bounded by 262,144 and 262,168 UTF-8 bytes respectively, two bounded State job records and the
  fixed-count Syndic command closure. Ordinary content chunking and record ceilings apply; no
  staging stream, queued job collection or retry can extend these per-operation bounds. The
  original job page is released before unrelated work is admitted.
- Explicit retry names the exact job and captured job revision. It requires the same retryable
  checkpoint, pending child gate and immutable parent provenance, including any accepted parent
  Syndic/CAS identities. Stable original home revisions fence these source observations before
  changing only State's job/live pair. Retry preserves the attempt and payload and performs no
  Syndic mutation or CAS request. Candidate and ordinary scans do not invoke retry automatically;
  outcome uncertainty uses the same retained process custody and exact State witness.

## Tools And Lifecycle Yield

- Every persistent conversation lineage uses one canonical versioned, deterministically ordered
  conversation-tool registry. Its exact identity is SHA-256 of canonical serialization;
  continuation, resume, and fork require the same profile.
- The generic broker authorizes exact connection, registration, loaded generation, CAS thread,
  turn, call, and installed tool before bounded argument ingress. Registry membership describes
  capability and grants no mutation authority.
- Feature sinks incrementally admit closed schemas and yield one non-cloneable bounded typed
  request. Routing retains no `serde_json::Value`, raw spool, complete request clone, or second
  response owner.
- Lifecycle-yield, branch-resolution, and theme tools keep separate feature schemas and
  authorization. Unknown tool, invalid envelope, schema failure, cancellation, loss, and handler
  failure produce one typed response or connection failure under exact dispatch state.
- Branch dispatch preserves the validated CAS thread/turn/call identity in a distinct typed
  handler context alongside ordinary home/service and Syndic correlation. Construction stays
  inside the ordinary dispatcher; payload parsing cannot manufacture that context. The handler
  borrows correlation independently of the non-cloneable request and sole response owner.

## Diagnostics

- One process-wide supervisor is the sole app owner of at most one diagnostic child, its bounded
  stdio channel, request correlation, and lifecycle custody through exit or cleanup.
- Child controls use the same exact window, thread, composer, stop, popup, scroll, and activation
  command paths as direct interaction. They never mutate behind those paths or substitute a target.
- Diagnostic snapshots are fixed-size and content-free where required. They do not load nonresident
  history, render hidden rows, decode media, scan catalogs on GPUI, query CAS history, or retain
  user content, paths, credentials, capabilities, or raw tool payloads outside an authorized bound.
