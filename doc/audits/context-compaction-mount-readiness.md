# Boundary

Initial read-only qualification on 2026-10-10 used the accepted model/reasoning mount at `b86f6932`.
The source/prerequisite sections below preserve that readiness snapshot. The later context-status
closure records its separate mounted acceptance; manual compaction remains a distinct boundary.
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

# Contract Closure

The owning-contract boundary passed independent semantic review on 2026-10-10. Backend defines
fixed-size exact usage, numeric domains, separately qualified model/metering association,
300/10,080-minute normalization windows and complete sparse quota observations. The pinned
producer has no qualified model association, including when identifier strings coincide; its
quota remains unavailable. CAS-live owns bounded agreeing-projection interest election and exact
observation retirement. Proven ordinary terminal revision advancement on the same loaded session
preserves source provenance; merely reading a newer binding cannot rebind an observation.

App and CAS-live define weak original-selection/claim/idle admission and independent bounded
feedback, with terminal settlement winning timeout and late outcome surviving local removal.
Notifications owns the shared protected stop/compaction FIFO and reserved visible contribution;
ordinary queue saturation cannot discard required command feedback. Window disposal releases
presentation without cancelling admitted execution. Source feasibility review included
`ordinary/execute/capture_loop.rs::finish_proven_terminal` and its checked-next binding transition.

Review corrections resolved namespace coincidence, multi-projection interest ownership,
Notifications retention and normal revision continuity. Scoped `git diff --check` passed and
rag-rat discovery/reconciliation reached Current with zero failed or blocked chunks. This was a
documentation-only boundary: no production Rust/manifest edits, Cargo run or GUI launch occurred.
Context status and manual command production mounting remain separately unaccepted work.

# Context Status Mounting Closure

The bounded ordered context path is implemented on 2026-10-10. Backend incrementally validates
exact usage routes/counters and known quota window shapes. Unsupported numeric domains replace
prior observations with unavailable values; missing/nonpositive context windows render Unknown.
The pinned producer supplies no qualified model/metering association, so account quota remains
unavailable and quota windows are omitted even when names coincide. Unrelated account facts are
streamed away. No account read or history/start effect is introduced by status observation.

CAS-live retains one exact observation per registered projection and bounded agreeing-model
interest per connection. Proven checked-next ordinary terminal advancement preserves original
usage provenance; unproven drift, replacement and retirement invalidate it. The existing coherent
status worker carries only a weak connection and exact loaded/session/binding/observation stamp to
the final GUI apply. Final stamp election uses nonblocking locks and fails closed to Unknown;
monotonic observation revisions reject identical-value ABA.

Review corrected quota schema validation, late publication provenance and the pinned outgoing
notification timestamp. The bounded optional timestamp tail also qualifies the necessary ordinary
start/status/terminal controls while preserving their owned payload checks. The
[pinned producer note](../memory/github.com/openai/codex/commit/e363b08c9175ac1cbe5893615dd2cb9ddf95043b/context-usage-and-quota-observations.md)
preserves this transport evidence. The first native fixture aborted on nested root drawing;
separate native window drawing and root reads corrected the fixture.

Final retained receipts verify 128 distinct affected cases under the root one-job LLVM build policy:

- Backend `incoming_json_ingress`: 110/110, run `ff914391-41cf-4b1d-90a0-0f8a4b30e839`.
  Includes every-split timestamp-bearing usage/quota/control/terminal parsing and fatal owned shapes.
- App `context_observation` and `mounted_exact_status_controls`: 17/17, run
  `f98be985-bb02-45db-b4a4-6204364f2d24`. Includes continuity/election/retirement and actual final
  GUI publication rejecting held identical-value ABA, selection loss and expired publication.
- Windows `context_status_native`: 1/1, run `9fbfdfa3-f053-4ab5-9991-8204d88f1d4e`.
  A real Application mounts and publishes the production shell, requests minimization and presents
  native frames. Real websocket start/status/usage/terminal ingress renders the production 180-pixel
  cell as 75%, Unknown and 87%; an actual terminal execution preserves 87% with `ok`, and exact
  connection retirement renders Unknown. The production cell's prepaint receipt verifies its
  native bounds and value, with explicit window/server/service cleanup.
- Default production `cargo check -p beryl-backend -p beryl-app --lib --locked --offline` passed
  in 13.90 seconds. Scoped formatting and `git diff --check` passed. No manifest changed.

All 42 Rust inputs matched the reviewed fingerprint receipt
`D244201F443F624043D23C8CE073A33D070F79E261D189FDB442A1FC8F959265`.
Independent whole-boundary completion review accepted phase 747 with no blocking findings.
Temporary receipts were consumed and the exact owned `.aipm/tasks/context-status/` root reclaimed
after absolute-path, reparse and process checks. All three native runner PIDs
and their test processes are absent. The initial abort did not print its Home; the bounded read-only
creation-time check found no remaining candidate, and no uncertain temporary directory was deleted.
No selected simplification-audit finding required a separate change in this boundary.
