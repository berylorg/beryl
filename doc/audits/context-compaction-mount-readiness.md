# Boundary

Read-only qualification on 2026-10-10 against the accepted model/reasoning mount at `b86f6932`.
This record establishes prerequisites; it does not accept context or manual-compaction mounting.
The [status-line feature](../features/status-line/design.md#context-and-rate-limit-cell),
[GUI composition](../features/status-line/gui.md#status-operation-menus),
[CAS-live system](../systems/cas-live-syndic-transcript/design.md) and
[App live control](../../crates/beryl-app/doc/design-live-control.md) remain authority.

# Context Source

The native `main_window/shell/host/status_controls/render.rs` still renders `Context Unknown`.
Backend's `turn/metadata.rs` exports an older `ThreadTokenUsage` value; its existence is not proof
of a production CAS-live source. The provider machine's `classifier.rs` explicitly classifies
`thread/tokenUsage/updated` and `account/rateLimits/updated` as discarded unavailable compact
controls. Searches of Backend source and App's `cas_projection` found no other usage/quota route.

Backend's authoritative [provider-stream supplement](../../crates/beryl-backend/doc/design-provider-stream.md#polling-and-repair-observation)
already requires exact fixed-width usage and bounded active-model quota normalization. The
[pinned producer investigation](../memory/github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/context-usage-and-quota-observations.md)
qualifies the usage notification's exact route and serialized order. A complete implementation
must add typed incremental ingress and source-ordered App handling before publishing any value.
The older serde type and legacy Shell cache are not adapters for that work.

Quota metering IDs do not establish a model mapping. The feature already permits unavailable
matches and independently omitted windows. No mapping, label inference or merged account bucket
is needed to keep that contract. Exact active-model interest, window classification and malformed
counter handling need an explicit bounded public contract before implementing normalization.
No latest-usage read response was qualified; absent authentic observations remain Unknown.

# Compaction Source

`cas_projection/service/commands.rs::compact_thread` delegates to the production process
coordinator. `context_compaction/coordinator.rs::compact_thread` authorizes a live command, reads
typed storage eligibility, joins only an exact existing local operation or admits a candidate,
then waits for its shared outcome off GPUI.

`coordinator/admission.rs::projection_for` finds exactly one registered existing foreground lease
for the candidate's runtime and CAS/Syndic thread. Duplicate exact leases fail. It rereads the
current binding and checks revision, execution runtime, CAS thread and represented prefix before
creating a loaded projection. `admit_manual` reserves execution custody and commits storage's
typed current admission before dispatch; it does not create a detached connector or resume a
missing session. Runtime Ready alone therefore cannot establish eligibility.

The public request in `coordinator/model.rs` carries thread and timeout policy, but no opaque
selected-window/claim/idle origin. Calling it after a GUI-only precheck would not prove that the
original popup's selection remained current at durable admission. A mounting capability must
capture that authority, revalidate it at the effect boundary and preserve storage's atomic
precedence against accepted input, repair and other operations.

`coordinator/settlement.rs::complete_local` publishes the shared outcome and removes the local
entry. `StillRunning` is a waiting outcome, not disposal or terminal success. A window needs a
bounded consumer-owned exact outcome handle that survives this removal and receives late
settlement. Its disposal must not cancel admitted execution or retain the service graph. The
existing weak exact-stop worker, operation origins and presentation budgets establish reusable
patterns, but do not by themselves constitute a manual-compaction worker contract.

# Verification And Next Boundary

Independent semantic completion review found no blocking readiness issue. Root and reviewer read
exact coordinator/admission/settlement bodies, classifier mappings, metadata exports and
controlling authority. Existing tests include `compaction_policy/process_admission.rs`,
`context_compaction/work_facts.rs` and `context_compaction/window_close.rs`; they cover process
fencing, custody after local removal and lifecycle close behavior, rather than a mounted manual
command. No source or manifest changed during this qualification, so no new Cargo run was needed.
The accepted [model mount evidence](model-selection-mount-qualification.md) remains reusable only
for unchanged widget/publication mechanisms, not as acceptance evidence for this missing behavior.

The next boundary is a bounded owning-contract update for transient exact context observations
and manual-compaction GUI authority/feedback. It must settle numeric/window domains, active-model
interest, retained-state limits, selection and incarnation fences, atomic admission and late-result
ownership, including sparse quota-update retention/invalidation, before production implementation.
Subsequent context-status and manual-compaction
mounting have distinct observable outcomes and require separate acceptance boundaries, including
real provider ingress, stale-window and successor races, no history mutation for status,
timeout/late settlement and teardown. No product contradiction or requirement for a migration
adapter was found. Phase-744 evidence remains retained; this investigation created no GUI or
managed runtime process, Home, Cargo execution or upstream checkout.
