# Running Threads Mounting Readiness

Source assessment at `589f2573957d9585f800b4e575fa8c9574c8f1ec`, 2026-10-03. This is bounded
implementation evidence, not target-state authority. No production source or test changes belong
to this diagnosis, and no native Beryl window was launched.

## Controlling Contracts

The [conversation-thread behavior](../../features/conversation-threads/design.md#running-threads)
owns collection membership and explicit activation. Its [GUI composition](../../features/conversation-threads/gui.md#running-threads-command)
owns the toolbar command and collection-only picker. The [CAS-live system](../../systems/cas-live-syndic-transcript/design.md#request-custody-and-running-thread-projection)
owns execution custody and bounded projection; [Notifications](../../features/notifications/design.md#main-conversation-notices)
owns retained attention and destination routing. The app's
[shell ownership](../../../crates/beryl-app/doc/design-shell-lifecycle.md#process-service-graph-and-windows),
[catalog and claims](../../../crates/beryl-app/doc/design-catalog-and-composer.md#catalog-and-claims)
and [feature adapters](../../../crates/beryl-app/doc/design-feature-adapters.md) supply package boundaries.
Their effective rigor requires production behavior and review of lifecycle, persistence and
external effects; source inspection alone accepts only this diagnosis.

## Command And Picker Consumers

[`MainWindowShellRoot::render`](../../../crates/beryl-app/src/main_window/shell/host/root.rs)
currently mounts New Window and Exit in `main-window-toolbar`, selected status controls, notices
and the composer. Its transcript region is an empty container. It mounts neither Running threads
nor a Thread Switcher. The other conversation toolbar contributions remain separate rework work;
their absence must not be mistaken for an existing picker adapter to extend.

The [thread-root picker specification](../../gui/widgets/thread-root-picker/spec.md) already
defines the collection-only immediate-selection variant, stable row identity, focus return,
fixed-height virtualization, bounded page requests and failure states. No corresponding picker
implementation was found in the current crate source inventory. The mount needs that canonical
widget's collection-only behavior, feature-specific row data and a shell overlay consumer.
It must realize only visible rows plus at most four overscan rows on each side, retain logical
count separately from resident rows, and expose the specified content-free diagnostics.
Implementing this variant does not authorize mounting runtime/root management or the ordinary
Thread Switcher. Search must query the source collection, never scan an accumulated GUI row vector.

The command must remain inspectable at zero count, have stable geometry as count changes, and
respect existing startup, ordinary-close and Exit gates. Arrival or refresh must never activate
a row or a window. No source currently establishes these Running threads consumers.

## Process Source And Its Missing Presentation Join

[`PublishedAppServices`](../../../crates/beryl-app/src/app_services/published.rs) exposes the
current CAS service, scheduled sessions, State and shared attention pool. Those are the actual
graph-owned sources; the GUI must receive a generation-qualified narrow adapter rather than
retain the whole graph or execution custody.

[`ProcessWorkInventory`](../../../crates/beryl-app/src/cas_projection/service/process_work.rs)
already merges typed durable work, checked-out sessions, connection targets/requests, control
work and attention. It returns exact thread identity, canonical title, execution binding,
last activity, work facts and exact attention records. Its page validates revisions before and
after collection, rejects foreign cursors and cancellation, and returns total logical count.
[`types.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/types.rs) caps each
page at 256 records and 65,536 bytes; ordering is descending activity time with exact thread ID
as tie-breaker. [`selection.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/selection.rs)
keeps a bounded sorted prefix, including typed single-row byte-limit failure.

[`required.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/required.rs)
qualifies durable, session, connection and control revisions with exact home and service ownership.
[`live.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/live.rs) maps preparing,
executing, pending, terminal settlement, requests, stopping, compaction and continuation facts.
[`durable.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/durable.rs)
merges non-idle gates and accepted ready/next inputs through bounded source pages. Available idle
sessions alone contribute no live fact; retained attention can keep an otherwise completed row.
The existing Exit count in
[`shutdown_observation.rs`](../../../crates/beryl-app/src/cas_projection/service/process_work/shutdown_observation.rs)
uses shutdown obligations and flight facts, excludes the attention collection, and is not the
Running threads toolbar count.

The inventory has no production caller outside its definition. Its existing contract has no search
argument, logical-position page access, State occupancy join or window publication adapter. These
are implementation gaps. A bounded source adapter must supply revision-bound searchable pages,
logical positions for Home/End and scrollbar navigation, exact runtime/root display metadata,
current/open-elsewhere/unviewed facts and a coherent count/attention summary. It must use source-owned
catalog search semantics and qualified State reads; it cannot reinterpret metadata or collect the
whole catalog. Count and query membership must remain distinguishable, so search does not silently
change the process toolbar count. The accepted catalog/page and window limits bound resident pages
and subscriptions. Output page bounds do not by themselves prove bounded scan scratch state:
qualification must also inspect the live-fact merge and every metadata/occupancy join.

Reads and source scans belong off GPUI. The mounted adapter must fence completion by original graph,
home/service generation, collection/query revision, request identity and window lifetime. Initial
failure must show failure rather than false emptiness; page failure must preserve coherent rows,
count, focus and scroll. Replacement, close and cancellation must settle pending requests and reject
late completions. Existing Retry publication fencing in
[`window_services.rs`](../../../crates/beryl-app/src/app_services/window_services.rs) is a useful
ownership example, not an existing Running threads capability.

## Exact Activation And Draft Custody

[`SessionState::thread_claim_catalog_source`](../../../crates/beryl-state/src/session/catalog_source.rs)
reads both reverse claim copies or proves absence. `SessionState::replace_claim` in
[`session.rs`](../../../crates/beryl-state/src/session.rs) supplies the typed atomic claim operation.
The app currently has no production `replace_claim` caller. Row display is not a reservation:
activation must revalidate exact occupancy and selected predecessor before taking effects.

For a current row, activation keeps the current selection and acknowledges only its displayed
attention token after successful exact validation. For an open-elsewhere row, the existing
[`PublishedMainWindowRestoreSet`](../../../crates/beryl-app/src/main_window/restoration/native.rs)
retains the bounded published shell set and exact handles; the
[`RunningProcessOwner`](../../../crates/beryl-app/src/running_owner/ordinary_commands.rs)
already owns created/closed-window publication. There is no Running threads reveal command.
The new command must match the current paired claim to that exact surviving shell before explicit
reveal, without creating a window, replacing the invoking selection or transferring custody.
A disappeared or changed destination cannot fall through to a different activation.

For an unviewed row,
[`MainWindowComposerSlot`](../../../crates/beryl-app/src/main_window/composer_slot.rs) already has
unpublished target activation, predecessor/source checks, ordinary `ThreadSwitch` flush tickets,
reconciliation, widget release and final publication.
[`MainWindowConversationComposerMount`](../../../crates/beryl-app/src/main_window/conversation_composer_mount.rs)
wraps those operations with editor fencing, autosave suspension/resumption and failed-target
retirement. These are callable composer primitives; they do not perform the State claim/session
transaction or attach a live conversation presentation. No production picker drives them.

The mount needs one exact activation owner joining ordinary flush, durable claim/session settlement
and view publication. Failed or indeterminate flush must retain the invoking view, its candidate,
history, caret and selection; the target's execution must remain running. Pending publication and
ambiguous durable outcome retain ordinary reconciliation custody. Cancellation, duplicate activation,
target occupancy races and home replacement must settle through those owners. A successful switch
releases view/editor/subscription ownership only, never the process execution session.

Managed lifetime fixtures in
[`execution_lifetime.rs`](../../../crates/beryl-app/tests/runtime_session_preparation/execution_lifetime.rs)
and [`compaction_lifetime.rs`](../../../crates/beryl-app/tests/runtime_session_preparation/compaction_lifetime.rs)
prove process work survives view-interest detach and immediate reacquisition of the same readiness.
They do not establish a mounted live presentation attachment. The production mount must obtain
bounded presentation subscriptions to the existing exact live thread without execution checkout,
another CAS load/projection, input replay, new turn or wait for idle. The empty shell transcript
container and those runtime-interest fixtures cannot be used as proof that this consumer exists.
The attachment's bounded current presentation must use the single canonical transcript host under
the [transcript shell boundary](../../systems/transcript-presentation/shell-boundary.md), with exact
activation seeds, bounded prepared snapshots and ordinary demand retirement. It cannot introduce a
separate live presentation model. This attachment is necessary supporting work; complete transcript
feature mounting remains its broader rework boundary.

## Lifecycle Attention Consumers

[`ProcessLifecycleAttentionPool`](../../../crates/beryl-app/src/lifecycle_attention.rs) is already
graph-owned, count-bounded and token-qualified. `acknowledge` checks exact pool and token identity,
removes only that record and advances the work revision; it cannot acknowledge a successor token.
[`work.rs`](../../../crates/beryl-app/src/lifecycle_attention/work.rs) provides owner-bound revisions
and compact snapshots. Lifecycle/terminal publishers already receive the shared pool through
process preparation. Records are compact fixed facts, not approval content or transcript data.

There is no production pool-acknowledgement caller, lifecycle notice destination router or picker
attention consumer. The mounted notice infrastructure in
[`shell/notices.rs`](../../../crates/beryl-app/src/main_window/shell/notices.rs) has exact window/home
ingress and arbiter tokens, but does not bind lifecycle pool records to those tokens. Pool capacity
omission exists; admission/priority/replacement and byte accounting must be checked against the
Notifications contract when projecting actual notice content. Existing pool presence is not full
attention mounting acceptance.

The mount must keep one shared process record, remove its prior presentation before offering it to
the current viewing window or smallest surviving WindowId, and bind exact notice-close and successful
row activation to that displayed record. Failure or stale activation must not acknowledge it.
Window absence, detach and reassignment preserve the record; arrival never changes focus. A completed
attention row retains neither execution lease nor hidden view. No durable acknowledgement or restart
reconstruction is authorized. Generation replacement must reject old callbacks and cannot import
an old pool's tokens into a new pool.

## Derived Implementation Boundary And Acceptance

The controlling docs decide the relevant ownership, activation, failure and bounded rendering
semantics. No material target-contract contradiction was identified. The missing consumers are
bounded implementation work, with no basis to introduce a replacement architecture or compatibility
shell. The next boundary is the mounted Running threads command/count and collection-only picker,
including exact activation and process-owned attention routing necessary to make its rows usable.
Source adapters, canonical widget realization, live presentation attachment and ordinary claim/flush
joining belong to that one visible behavior, rather than a series of helper-only acceptances.

Acceptance must drive the actual shell command and row callbacks with two windows and an unviewed
running thread: zero count, recent-first/search pages, current-row acknowledgement, exact existing
window reveal, and ordinary flush-before-attachment. Qualify failed/indeterminate flush, occupancy
race, terminal-history retention/removal, attention-only rows, stale acknowledgement, destination
change, query/page failure, cancellation, duplicate activation and generation replacement.
Observe unchanged live connection/turn identity, exact request/input custody and no new dispatch,
CAS load or capture subscriber. Qualify bounded resident/realized rows across large logical counts,
Home/End, keyboard focus through reorder/removal, scrollbar paging and content-free diagnostics.
Test actual disposal of owned subscriptions and workers. Run affected storage/State/app checks as
changes require, and independently review lifecycle, authority joins and external-effect exclusion.

Independent completion review passed on 2026-10-03 against the cited source and controlling
contracts, including the final canonical transcript-host clarification. Root validation checked
the evidence links and documentation diff. No blocking finding or unresolved material design
choice remained. This accepts the readiness evidence and derived implementation boundary;
production mounting and its behavioral qualification remain pending.
