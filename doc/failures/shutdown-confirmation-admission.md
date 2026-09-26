# Shutdown Confirmation Admission Lock Ordering

## Invalidated Composition

On 2026-09-26, source inspection during ordinary close/Exit composition showed that the accepted
read-only shutdown observation cannot simply be revalidated inside
`ProcessAdmissionGate::fence_if_quiescent`. The conditional fence holds the process-admission mutex
through its callback. Existing work-revision readers acquire locks above that mutex in dispatch
paths. This finding concerns the proposed integration; no such callback has been mounted.

## Decisive Evidence

- `crates/beryl-app/src/process_admission.rs`: `fence_if_quiescent` holds `inner` across validation;
  `ProcessExecutionPermit::commit` acquires the same mutex.
- `crates/beryl-app/src/cas_projection/connection/router/command.rs`:
  `authorize_turn_start` holds the router state lock while invoking
  `LiveCommandPermit::commit_execution_if_current`.
- `crates/beryl-app/src/cas_projection/persistent_failure/gate/permit.rs`:
  `commit_execution_if_current` holds the live-command gate through acquisition of process
  admission. The dispatch lock order is router, command gate, process admission.
- `crates/beryl-app/src/cas_projection/service/process_work/required.rs`:
  `validate_required_work_revision` calls `validate_connection_work_revision`.
- `crates/beryl-app/src/cas_projection/service/work_facts.rs`: connection revision validation
  locks the connection registry, then `read_work_stamp` checks the live-command gate and reads
  each connection's work stamp.
- `crates/beryl-app/src/cas_projection/connection/router/work_facts.rs`: `work_stamp` locks router
  state. Thus validation under process admission would acquire both command and router locks in
  the reverse direction. A dispatch worker holding either while awaiting process admission can
  prevent the shutdown callback from returning, leaving both workers blocked.

Context-compaction and active-steering dispatch use the same execution admission relationship.
The source lock cycle is sufficient to reject this composition; no deliberate hanging test was
run. Existing observation and conditional-fence tests do not exercise this proposed combination.

Independent concurrency review confirmed both cycles. It additionally checked
`persistent_failure/gate/authorizer.rs`: `authorize` holds the live-command gate while obtaining
`process.execution_permit()`, which takes process admission without increasing reservations.
Therefore the conditional fence's zero-reservation check cannot exclude this conflict. A source
search confirmed that the conditional fence has no mounted caller. No production changes or
new Cargo verification were made for this source-only assessment.

## Recommended Correction And Remaining Boundary

Keep observation outside process admission. Introduce a dedicated admission-safe validation path
whose checks under the admission lock never wait for a lock that dispatch can hold while seeking
admission. Nonblocking exact revision and provenance checks can refuse busy or changed sources
without fencing or changing execution authority. Audit every nested source, including health,
durable revision, sessions, connections, controls and cleanup custody; changing only the router
read is insufficient. Do not substitute a speculative fence followed by reopening on cancellation.

The Operator clarified that bounded technical corrections proceed autonomously; this finding
does not require another approval. Derive each prerequisite and lock-order evidence from the existing
[window lifecycle contract](../../crates/beryl-app/doc/design-shell-lifecycle.md#window-detachment-and-process-shutdown)
and [main-window behavior](../features/main-windows/design.md#ordinary-window-close).
Verification must cover contention with actual dispatch lock order, stale/foreign evidence,
unchanged permits after refusal, and work appearing before the idle admission cut. Native
confirmation, restore modes and process-owner mounting remain separate unfinished work.

The durable-read prerequisite was accepted on 2026-09-26: home-store observed coherent election
validates exact observation/store identity and unchanged mutation interval, then holds nonblocking
mutation, reconciliation and health guards through the caller's publication. Observation precedes
the durable reads; returning successfully does not retain proof for a later publication. All 27
focused tests passed as run `399f44a3-f217-49d1-baac-d8afffed4616`, together with the default package
check and independent concurrency/integrity review. Runtime-source validation and complete
shutdown admission remain unimplemented and are not accepted by this home-store evidence.
