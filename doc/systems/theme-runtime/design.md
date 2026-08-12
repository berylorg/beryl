# Goals

Define one cross-package theme runtime that turns Beryl-home theme documents and the active-theme
setting into validated, coherent appearance generations for every Beryl window.

Keep repository enumeration, mutation reconciliation, theme resolution, cross-window publication,
and transient preview arbitration exact and bounded without making GUI projections durable
authority.

## Non-goals

- Owning the user-visible Themes workflow, labels, disabled states, or recovery messages.
- Owning the generic Settings window-wide draft, Apply/OK workflow, or scalar settings schema.
- Exposing raw repository files, Fjall records, GPUI entities, or backend-owned Codex configuration
  across the theme boundary.
- Treating preview, editor drafts, resolved-theme caches, or window subscriptions as durable state.
- Supporting multiple persisted theme schemas or compatibility adapters.

# Decisions

## Authority And Participating Boundaries

- The [Theming feature](../../features/theming/design.md) owns visible selection, editing,
  validation, fallback, Save/Save As, repository-command, application, and preview outcomes.
- The [Settings feature](../../features/settings/design.md) owns the window-wide settings draft and
  the atomic Apply/OK operation through which active-theme identity becomes durable.
- The [Beryl-home storage system](../beryl-home-storage/design.md) owns the configured home, physical
  repository location, bounded file access, durability, store health, and generic Fjall mutation
  outcomes.
- [beryl-state](../../../crates/beryl-state/doc/design.md) owns the typed theme-domain repository,
  parser, validator, resolver, mutation, and reconciliation service layered over physical home
  storage. It also owns
  typed storage and revision behavior for the scalar active-theme identity; installed documents and
  order never enter the Settings keyspace.
- [beryl-app](../../../crates/beryl-app/doc/design.md) consumes that service, assembles the process-
  wide publication and preview coordinator, and owns only GPUI adapters, window subscription, and
  bounded UI bridges. It does not parse repository files, become durable theme authority, or
  expose GPUI handles to repository and tool workers.
- Transcript candidate panels and dynamic tools submit typed intents to the same theme runtime.
  They do not independently parse, resolve, mutate, arbitrate, or publish appearance.
- The [bounded-resource system](../bounded-resource-dataflow/design.md) governs source pages,
  staging, queues, caches, and worker limits. Project GUI integration and widget specifications own
  window slots, composition, reusable anatomy, and rendering mechanics.

## Canonical Identities And Snapshots

- Installed theme id, exact home generation and store instance, repository generation, document
  revision and digest, feature-owned theme-document draft revision, Settings revision, appearance
  generation, preview request sequence, and window-set epoch are distinct identity domains. Equal
  numeric values never make identities interchangeable.
- One immutable manifest snapshot binds installed order, stable theme ids, names, and cursor pages
  to one manifest generation. Each document result separately binds that manifest membership, the
  exact home and service generation, one process-local document-observation revision, byte length,
  and digest. A manifest page or document result from another identity cannot be combined with it.
- The built-in fallback has the fixed internal identity `BuiltinFallback` and is an immutable
  non-installed source. It has no installed theme id, repository row, editable document, or
  repository mutation capability.
- One immutable resolved appearance contains every supported role/property value required by one
  publication. Resolved appearances retain no source document, parser buffer, manifest page, or
  feature editor draft.

## Theme Schema, Parsing, And Resolution

- The theme runtime owns one finite hardcoded schema of role ids, supported properties, static
  parents, ambient-parent eligibility, value-source keywords, value domains, and built-in fallback
  values. Region, text, primitive, control, row, menu, status, media, transcript, navigation,
  window, and settings roles expose only their declared property sets.
- Canonical expanded UI role ids come from the owning widget specifications. Widget parts, geometry,
  and interaction-state presentation remain local to those widget roles; the theme schema supplies
  only supported appearance and layout values for declared ids and cannot invent a parallel global
  role family. Nonvisual widget resource constants, including detail-row caps and virtualization
  overscan, are excluded from theme documents and resolution.
- Compact TOML documents encode roles as `[[role]]` records with `id`, optional `static_parent`, and
  supported property entries whose values are source keywords or concrete inline values.
- One incremental bounded parser, validator, and resolver serves startup, refresh, editor
  validation, preview, install, update, Save, Save As, activation preparation, and
  `validate_theme_document`. No caller provides a second parser or whole-document compatibility
  path.
- Validation proves structural completeness, stable-id uniqueness, supported property/value-source
  combinations, concrete value bounds, static-parent validity, ambient eligibility, and complete
  fallback resolution before a candidate can become publishable.
- Unsupported persisted entries are skipped during load and omitted when the runtime serializes a
  later saved document. Unsupported role/property combinations never inherit into existence.
- `read_theme_schema`, editor schema projections, validation, and authoring guidance derive from the
  same canonical schema. Guidance is explanatory and cannot introduce another accepted value.
- The finite schema bounds resolved-theme size. The maximum supported-property count for any one
  editor role plus any simultaneously composed page-local detail rows must not exceed the external
  `settings-window` `MAX_PAGE_DETAIL_ROWS` contract. Schema or composition changes that exceed that
  relationship require an authority change rather than truncation or hidden rows.

## Installed Repository And Enumeration

- The repository root is `<beryl-home>/themes`. Its Beryl-owned `manifest.toml` contains the schema
  version, manifest generation, and ordered installed entries. Each entry contains the stable theme
  id and bounded display name; the corresponding user-editable document is
  `installed/<stable-theme-id>.toml`.
- Stable theme ids contain 1 through 64 ASCII lowercase letters, digits, or interior hyphens, begin
  and end with an ASCII lowercase letter or digit, and therefore map to document filenames without
  escaping or caller-supplied path interpretation. A root-level `theme.toml` is outside the
  boundary and is never read, imported, rewritten, migrated, or deleted by the theme runtime.
- `manifest.toml` owns membership, names, and installed order but does not pin document content,
  length, digest, or observation revision. Users may edit an installed TOML document directly;
  files absent from the manifest remain inert and are never auto-installed.
- Installed-theme count is logically unbounded. Enumeration uses revision-bound cursor pages with
  stable theme ids and explicit item and decoded-byte limits; the runtime never materializes the
  complete manifest or retains every row to answer a page request.
- Document reads bind exact theme id, manifest generation, service generation, process-local
  observation revision, exact byte length, digest, and bounded source ranges. A changed document
  invalidates only work prepared from its superseded identity, including when the bytes later
  return to an earlier digest.
- Startup, refresh, and Settings snapshots retain only bounded manifest pages, compact metadata,
  the resolved durable base, and any exact document or draft currently required by an operation.

## Repository Mutation And Reconciliation

- Install, rename, delete, reorder, update, Save, and Save As are typed repository commands. Each
  carries the expected home and manifest generations plus every exact theme id, document
  revision, digest, order position, and feature-draft revision on which it depends.
- Beryl-authored document writes stream one complete canonical replacement to a sibling staged file,
  validate its exact length and digest, durably flush it, and atomically replace the stable installed
  TOML file. Save and update change only that document and its observation identity; they do not
  rewrite or advance `manifest.toml`.
- Install and Save As durably publish the new stable document before atomically replacing
  `manifest.toml` with the generation that admits it. Rename and reorder replace only the manifest.
  Delete first replaces the manifest with the generation that no longer admits the id; later file
  removal is non-authoritative and a retained file remains inert. Readers never treat an unlisted
  file as installed.
- A command that changes membership, name, or order advances the manifest generation. A command that
  changes only document bytes advances only that process service's document-observation revision.
  Both forms return the exact affected identities needed to reject stale publication.
- Repository command results are exactly `NotCommitted`, `Committed`, or `Indeterminate`.
- `NotCommitted` proves the command's authoritative replacement did not occur and carries no new
  identity; `Committed` carries the exact new manifest generation when changed plus affected
  document identities; `Indeterminate` carries no publishable identity and only one operation-
  scoped reconciliation descriptor.
- The descriptor contains only the exact old and intended new manifest/document identities needed
  for targeted reread. Reconciliation yields `ExactOld`, `ExactNew` with the reconstructed committed
  generation, or `Collision` when the repository matches neither complete side or mixes them.
- `Indeterminate` gates duplicate or dependent mutation and refresh only for its exact repository
  scope. `ExactOld` or `ExactNew` reopens that scope. `Collision` leaves it closed and never guesses,
  merges generations, deletes files, substitutes another theme, or escalates to a whole-home scrub
  without independent structural evidence.
- Cancellation may retract work only before repository admission. Admitted staging and publication
  drain to an exact result or reconciliation; caller cancellation is never proof of non-commit.
- External file writers do not participate in Beryl's command protocol. A Beryl command validates
  its expected observed identity at admission and publishes by atomic file replacement; a later
  external replacement is a new observed change. A race that leaves neither the exact admitted old
  nor intended new bytes classifies as `Collision` rather than merging or guessing user intent.
- Delete admission rechecks one exact reference snapshot and rejects before mutation when the
  target is the durable active identity, the Settings-staged active target, bound to an open
  theme-document draft, or owned by pending or reconciling repository work. Neither GUI nor tool
  callers may bypass that guard or convert rejection into implicit identity or draft mutation.

## Settings Selection And Theme-Document Operations

- The active theme identity is one ordinary scalar in the Settings window-wide draft. The theme
  runtime prepares a completely validated and resolved candidate bound to the exact Settings draft,
  repository generation, theme id, and document revision, but it neither commits that setting nor
  publishes the candidate before Apply/OK succeeds.
- Reset of that Settings row replaces only the staged active-theme scalar with the value from the
  exact durable Settings snapshot on which the draft is based. It does not issue a repository
  command, mutate or rebase the feature-owned theme-document draft, or publish an appearance.
- A current-generation Settings `Committed` result or reconciled `ExactNew` authorizes publication
  of the prepared durable base. `NotCommitted` or `ExactOld` discards it. `Indeterminate` and
  `Collision` authorize no new appearance and follow the Settings feature's scoped gate behavior.
- The Settings command may commit the active identity with unrelated scalar settings. A later theme
  application failure cannot roll back or reinterpret that durable batch; the runtime keeps the
  prior coherent appearance and retries application from the newly durable identity when requested.
- The feature-owned theme-document draft is separate from the Settings draft and binds installed
  theme id, base repository generation, document revision, and draft revision. Theme property rows
  may use settings widgets, but their events never enter Settings Apply/OK.
- Save updates the bound installed document. Save As creates a new stable installed theme and order
  entry. Neither operation commits or discards Settings values, and Save As does not change active
  identity.
- Save As snapshots the exact dirty draft while retaining its original installed-theme binding and
  original document revision as editor state. A `Committed` result or reconciled `ExactNew` adds the
  new installed identity and document but does not update the original installed document, rebind or
  clean the original draft, change its selected role, stage the new identity in Settings, publish an
  appearance, replace a preview, or select the new installed row.
- A Save As `NotCommitted` result or reconciled `ExactOld` retains the prior coherent repository
  generation and exact original dirty draft, binding, selection, and selected role; the Settings
  draft, current appearance generation, and preview are unchanged. `Indeterminate` retains that same
  coherent projection, editor state, Settings draft, appearance, and preview while its exact
  repository scope is gated; reconciliation applies only the `ExactOld` or `ExactNew` transition
  just defined. `Collision` keeps the last coherent projection and original editor state, leaves
  the Settings draft, current appearance generation, and preview unchanged, leaves the affected
  repository scope unavailable, and never fabricates, selects, activates, or binds the intended new
  theme.
- An exact repository publication for a Save of the durable active document prepares a new durable
  base. With no preview the coordinator publishes that base; with a preview it updates the hidden
  durable baseline while leaving the preview current so Stop Preview restores the newly saved
  document. If appearance publication then fails, the repository result and clean document draft
  remain committed while the prior coherent current appearance remains until Retry succeeds.
- Only a repository `Committed` result or reconciled `ExactNew` may replace that durable baseline.
  `NotCommitted`, `ExactOld`, `Indeterminate`, and `Collision` retain the prior durable base.

## External Document Change Observation

- `beryl-home-store` owns one bounded coalescing filesystem-notification lane for the physical theme
  repository and exposes package-neutral change hints without paths or file handles. Notifications
  are wakeups rather than content or commit authority; duplicate, reordered, and overflow signals
  trigger a bounded coherent refresh instead of reconstructing missed events.
- `beryl-state` maps a stable installed filename to the exact manifest member, then performs one
  bounded reread, length and digest calculation, parse, validation, and complete resolution through
  the canonical theme service. It confirms the manifest membership and document identity again
  immediately before returning a publishable result. Superseded work is rejected.
- A valid changed active document advances its process-local observation revision and prepares a
  replacement durable baseline. With no preview, the coordinator publishes it atomically; with a
  preview, the preview remains current and Stop Preview restores the externally changed valid base.
- An invalid, partial, missing, unreadable, or over-limit live document keeps the last coherent
  repository projection and appearance, records bounded typed failure provenance, and awaits a
  later coalesced change or explicit Retry. It never publishes the built-in fallback merely because
  a running process observed a bad edit.
- On startup there is no prior coherent installed appearance to retain. If the durable active
  identity's current file is unavailable, invalid, over-limit, or cannot be applied, startup
  publishes the complete built-in fallback as required by the Theming feature.
- Beryl-authored atomic replacement produces the same change signals. Matching length and digest
  make those signals idempotent; the runtime neither publishes a second appearance generation nor
  mistakes its own committed write for another repository command.
- External modification of `manifest.toml` is not an install, rename, delete, or reorder workflow.
  An unexpected manifest identity follows coherent refresh and scoped-unavailable behavior; files
  that are not admitted by the last coherent Beryl manifest never become installed implicitly.

## Appearance Generation Publication

- One process-wide coordinator owns the latest coherent durable base, at most one coherent preview,
  the resulting current appearance generation, and the exact live window-set epoch.
- Publication prepares one immutable complete appearance and offers that exact generation to every
  live window adapter in the captured epoch. No adapter changes its current generation until all
  required adapters accept; one coordinator commit then advances the shared generation and reports
  success.
- A window renders all theme roles for one generation in a frame. It never combines cached role
  values from different generations. A newly registered window receives the coordinator's current
  complete generation before its first themed presentation.
- Window creation or closure during preparation invalidates that attempt or joins it through a new
  window-set epoch. Adapter rejection, unavailable window state, or application failure leaves all
  windows on the prior coherent generation and reports no partial success.
- Home, repository, document, Settings, draft, preview-sequence, window-epoch, and appearance-
  generation checks all pass immediately before publication. A stale completion cannot update the
  current appearance by coincidence.

## Preview Arbitration

- One monotonic process-wide arbiter orders transcript-candidate and dynamic-tool preview intents.
  Each intent binds source kind and identity, candidate revision or digest, request sequence, and
  the durable base generation observed at invocation.
- A later preview invocation supersedes every earlier pending preview regardless of source. Only
  the latest sequence may publish; failure of that latest request preserves the prior coherent
  current appearance and never revives a superseded completion.
- Successful replacement publishes the new preview directly over the current appearance without
  flashing the durable base between previews. Preview publication uses the same window-set and
  generation barrier as durable appearance publication.
- Stop Preview takes the next sequence, supersedes pending preview work, and republishes the latest
  durable base. If that base cannot be validated or applied, the current preview remains coherent
  and current.
- A successfully published Settings active-theme change ends preview. A committed active-document
  Save or coherent repository refresh while preview is current updates the durable base underneath
  it without changing preview precedence.
- Preview state is process-local. Orderly Exit and process loss persist no preview identity,
  candidate document, sequence, or appearance generation.

## Startup, Refresh, And Fresh-Service Recovery

- Startup reads the active identity from the current Settings snapshot and the matching document
  from one coherent repository snapshot, then validates, resolves, and applies it before publishing
  an installed durable base. With no saved identity, the built-in fallback is the durable base.
- An unavailable identity, missing or unreadable document, invalid theme, or application failure
  publishes the complete built-in fallback and retains typed failure provenance for the feature's
  localized Retry presentation. It never fabricates an installed identity for the fallback.
- Repository refresh keeps the last coherent snapshot and durable base until the complete new
  snapshot and any affected active document are resolved and applicable. A failed refresh cannot
  partially update installed order, editor inputs, durable base, or current preview.
- Structural Beryl-home failure retires the theme runtime's repository service, Settings handle,
  mutation gates, workers, pages, and preview arbiter with their exact home generation. None is
  adopted by a replacement service.
- Same-home recovery constructs a fresh theme runtime from the unpublished fresh home, typed-state,
  repository, and app-service candidates behind the startup fence. Old cursors, reconciliation
  descriptors, subscriptions, and preview state do not cross that fence.
- Existing windows may retain the last immutable appearance for coherent presentation while the
  fresh service is unavailable, but it is not storage or mutation authority. The fresh runtime
  publishes only a newly proven installed base or the complete built-in fallback.

## Tool And Worker Boundaries

- Schema reads, validation, preview, Stop Preview, install, update, Save As, and active-choice
  staging cross bounded typed request/response brokers. Turn workers receive structured outcomes
  and never hold repository mutation authority, Settings drafts, GPUI handles, or window adapters.
- Active-choice tools may stage only the exact scalar through the Settings feature's existing draft
  authority and never invoke Apply/OK. Repository tools use the same typed commands, parser,
  validation, and reconciliation as GUI-initiated operations.
- Dynamic-tool Save As may create a new installed document from a validated candidate. In-place
  Save or update is accepted only when the request names an existing installed theme plus its exact
  repository generation and document revision; an unbound candidate has no in-place Save target.
- The runtime exposes no access to Codex authentication or configuration, Syndic history, runtime
  and root records, image assets, unrelated settings, or raw Beryl-home storage.

## Bounds And Diagnostics

- Repository page caches, source pages, staged-file buffers, parser state, mutation workers,
  reconciliation workers, coalesced filesystem notifications, live-edit rereads, preview
  preparation, publication attempts, window adapters, and tool brokers each have an explicit
  configured item, byte, or concurrency bound and release capacity after completion, cancellation,
  supersession, reconciliation, window closure, or service retirement.
- At most one mutation executes for an overlapping repository scope and at most one reconciliation
  owns its descriptor. Preview arbitration retains only the current preview, the latest pending
  sequence, and bounded preparation state rather than an unbounded request history.
- Parsing, file I/O, resolution, hashing, serialization, and staged publication run away from the
  GPUI thread. Window adapters receive only finite resolved appearances and bounded presentation
  facts.
- Content-free diagnostics expose home and repository generation presence, bounded page and worker
  counts, filesystem-notification coalescing and overflow counts, mutation and reconciliation
  outcomes, preview source kind and sequence, appearance generation, window-set epoch, adapter
  counts, stale-result rejection counts, and publication failure class. They never expose theme
  names, document text, concrete user values, paths, tool arguments, or editor drafts.
