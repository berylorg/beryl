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

## Diagnostics

- One process-wide supervisor is the sole app owner of at most one diagnostic child, its bounded
  stdio channel, request correlation, and lifecycle custody through exit or cleanup.
- Child controls use the same exact window, thread, composer, stop, popup, scroll, and activation
  command paths as direct interaction. They never mutate behind those paths or substitute a target.
- Diagnostic snapshots are fixed-size and content-free where required. They do not load nonresident
  history, render hidden rows, decode media, scan catalogs on GPUI, query CAS history, or retain
  user content, paths, credentials, capabilities, or raw tool payloads outside an authorized bound.
