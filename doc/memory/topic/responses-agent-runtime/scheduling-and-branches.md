# Reason For Investigation

Determine what replaces CAS collaboration and native branch operations while preserving Beryl's
durable branch discussion and process-owned background work. This assessment uses existing local
authority and retained source evidence; no live parallel inference was performed.

# Outcome

A child agent can be another local conversation/context plus the same direct request/tool loop.
No subscription-side thread-spawn endpoint is needed for that candidate architecture. Scheduling,
permissions, mailboxes, waits, quota arbitration and recovery nevertheless become Beryl work.
This is architectural reasoning from direct explicit-context acceptance, not demonstrated
multi-agent service behavior or approval to replace the current CAS-native contract.

Three concepts must stay distinct: model-created subagents, Operator-visible branch discussions,
and background execution of an ordinary thread. They have different context, admission, result
and restart rules even if they share a bounded executor.

## Subagent Ownership Proposal

A child has an exact local identity, immutable parent/fork provenance, execution root, selected
model/effort, instruction/tool-policy snapshot, context revision and task mailbox. Reserve its
capacity before publishing successful creation. A rejected or indeterminate reservation must not
start an untracked child request. Local model/effort metadata can populate activity directly;
CAS nickname lookup and provider-ID discovery can disappear.

History selection and model selection are independent responsibilities. Preserve the required
ability to choose explicit child model/effort rather than forcing parent inheritance. Validate
against the actual catalog and chosen compatibility envelope. A different model may require a
different request dialect; copying opaque parent context across it is not proven safe.
Fresh-task, bounded-history and full-history seeds need exact boundaries, especially across
compaction and incomplete tool calls. Share immutable resource references where valid; do not
duplicate every ancestor's payload in memory merely to construct a child.

Tool permissions cannot be broadened by a model-supplied parent ID or task text. Determine child
capabilities from host policy and inherited execution authority. Subagents sharing a filesystem
also share effects: isolated conversation context is not a worktree or filesystem sandbox.
Concurrent edits need deliberate coordination or conflict reporting. A read-only task instruction
is not an OS-enforced read-only capability.

Messages and follow-ups require exact recipient, origin, sequence and bounded payload custody.
Sending to a running child should enqueue at an eligible point, not mutate already-sent request
bytes. Starting an idle child is a distinct admission operation. A wait observes local state and
must release its worker while blocked; it must not hold a model-request permit that prevents the
awaited child from running. Bound notification fan-out, retained messages and final-answer
delivery independently of the number of historical children.

At crash, durable accepted tasks and finished results can remain queryable, but uncertain active
effects cannot be blindly restarted. Orphan policy needs an explicit choice: retain interrupted
children for inspection and later explicit continuation, or allow only narrower proven-safe
resumption. Do not reconstruct work from a parent transcript saying that an agent was started.
Completion notification is not proof that the parent consumed the result.

## Concurrency And Quota

Use separately bounded admission for model requests, local tools, child tasks and maintenance.
A stalled network request must not prevent stop or terminal-result processing. Avoid nested waits
that consume every permit. Fairness should prevent title jobs or compaction from starving accepted
interactive work, while allowing hidden threads to make progress without a window claim.

Personal Pro limits are shared across these jobs and other clients on the account. Internal
reservation cannot promise remaining remote quota. Honor explicit service limits and represent
unknown accounting; do not multiply retries across every child on the same capacity error.
No direct experiment has established a safe maximum concurrent request count, a reservation API,
or identical limits across models. Choose conservative concurrency and validate later with bounded
real tasks rather than stress testing the account.

## Branch Discussion And Parent Handoff

Keep exact selected assistant text, finalized source identity/range/revision, digest and context
envelope. Branch creation remains one durable operation with no model request; later activation
must not repeat creation. A parent switching paths does not rewrite the retained source envelope.
Direct context ownership can remove native CAS fork/injection mechanics, but it cannot weaken
these provenance and admission guarantees.

Resolution still derives parent, child and job identity from the correlated app-tool request,
never from model arguments. Queued future-turn input defers admission without creating a job.
Successful admission atomically creates the intent/job and closes the discussion's composer gate.
One exact job admits at most one parent input; restart advances an already durable step rather
than appending another turn. Busy parent work waits without redirecting to a new thread.

If parent request delivery is uncertain, retain delivery-unknown custody and prohibit automatic
replay. Uncertainty alone does not release the discussion gate while the parent may still execute.
Current authority converges the parent to incomplete and the handoff to terminal failure only
after proven execution-session loss; a direct runtime must define its equivalent exact authority-
loss boundary. Parent success permits the atomic job-success/archive commit; archive publishes
only after that commit's `SyncAll` barrier.
Failure keeps the discussion unarchived and its evidence intact. Later
fresh resolution is distinct from retrying the failed effect. Existing bounded live-job scanning,
reconciliation slots and durable admission structures remain useful; the actual branch coordinator
is still a pre-existing implementation gap.

## Windows, Stop And Lifecycle

Execution belongs to the Beryl process/home, not the selected window. Switching or closing a
nonfinal view does not cancel ordinary work, queued input, compaction or eligible lifecycle
continuation. Running-thread visibility must derive from exact local occupancy, including hidden
threads and branch jobs. A window claim is presentation ownership, not a scheduler lock.

Final close/Exit freeze new dispatch and settle owned work with their distinct restore behavior.
Stop remains an exact operation and must fence later admissions. Current product authority
provides soft stop and explicitly excludes hard-stop/child-termination controls; ownership of
local child cleanup does not authorize adding those controls. Handler-specific cancellation and
unknown remote outcomes need the new execution contract, not labels implying remote termination.

Unaccepted lifecycle continuation intent stays process-local. Stop, shutdown or accepted user
input can consume it. A restart may recover an already accepted continuation as ordinary work,
but cannot create another from a recorded yield. Compaction completion alone does not authorize
resuming either a parent or child.

## Decision And Verification Boundaries

The replacement must explicitly adopt a subagent compatibility envelope and reconcile the
current prohibition on an imitation spawn tool/parallel child registry. That prohibition belongs
to the CAS architecture, not evidence that a Beryl-owned runtime is impossible. Do not leave both
CAS and Beryl claiming the same children in a clean replacement.

Local deterministic verification should cover capacity exhaustion, nested waits, duplicate
messages/results, missing recipients, stop versus follow-up, parent exit, crash after child
creation, sibling file conflicts, compaction/fork provenance, queued-input versus resolution,
parent delivery uncertainty, archive publication and final-window shutdown. A later small live
two-child task can test transport/profile selection if it changes a decision; no concurrency
service claim is inferred from current single-request probes.

# Sources

- Local authority inspected 2026-09-17: `doc/systems/backend-runtime/design.md` Pinned Protocol
  Boundary and stop exclusions; `doc/features/activity-panel/design.md`; `doc/features/main-windows/design.md`;
  `doc/systems/branch-discussion-handoff/design.md`; `doc/features/lifecycle-yield/design.md`;
  `doc/features/conversation-threads/design.md` Replacement Editing and Running threads.
- [Retained CAS 0.146.0 collaboration investigation](../codex-app-server/subagent-model-selection-0.146.0.md)
  establishes the historical model/effort/fork distinction and source pin; it is not a claim about
  current direct subscription behavior or complete current Codex collaboration internals.
- [Direct continuation](direct-stream-continuation.md), [execution/recovery](durable-execution-and-recovery.md),
  [context assessment](context-and-compaction.md) and [responsibility inventory](cas-responsibility-inventory.md).
- Ownership and scheduling choices above are proposals derived from retained requirements,
  explicitly awaiting architecture selection and local implementation verification.
