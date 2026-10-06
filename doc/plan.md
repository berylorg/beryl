# Scope

Operator selected continued use of unmodified CAS on 2026-09-23, authorized necessary buffering
for its payload-before-route ordering, and requested resumption and completion of the Beryl-home
rework. The whole-rework implementation hold is released by that explicit direction, subject to
ordinary phase design-readiness and acceptance gates. CAS replacement is deferred until this
rework is fully finished; the completed [research decision package](memory/topic/responses-agent-runtime/architecture-decision.md)
is retained as evidence, not current implementation scope. Its proposed subscription runtime,
tool host, model, authentication, recovery and integration selections do not change the CAS target.

The [root design](design.md), [bounded-resource system](systems/bounded-resource-dataflow/design.md),
[CAS-live system](systems/cas-live-syndic-transcript/design.md#durable-store-outage-buffer) and
[app capture contract](../crates/beryl-app/doc/design-live-capture.md) now permit necessary typed
pre-route retention. Healthy storage keeps bounded-page Syndic staging; outage capture uses
bounded unpublished assembly and existing qualified retention, with explicit loss attribution,
closed durable admission and full disposal before replacement. No CAS producer change or further
replacement-runtime investigation is a prerequisite. The existing release admission remains in
force; the 0.154.0 ordering investigation does not upgrade the pinned 0.146.0 provider contract.

Production marker admission and canonical widget publication have passed acceptance with the LLVM, one-job,
no-normal-debug and nonincremental settings under the root
[technology decisions](design.md#implementation-technology). The Operator authorizes pushing projects and owned forks as needed for
this work, including the accepted widget revision and Beryl changes. Preserve required
checks and independent review while using conditional delegation and bounded evidence.

Keep GUI thread switching clean without overengineering. The two authorized non-GUI corrections,
marker admission and healthy scheduler-conflict handling, are accepted. The Operator's instruction to
continue implementation resumes process-provider composition and the remaining phases below.
The controlling contracts are [conversation threads](features/conversation-threads/design.md),
[backend recovery](features/backend-runtime-recovery/design.md), and the
[app package](../crates/beryl-app/doc/design.md). Complete process composition, Running threads,
final-window shutdown and other broader background-work requirements remain separate rework work.

The Operator authorizes continuous implementation until a blocker requires attention, with a
commit after each accepted phase. Temporary-directory deletion and obsolete-directory cleanup
remain authorized; verify exact targets and preserve unrelated or concurrent work. The active
Beryl-home architectural replacement remains tracked by [REWORK.md](rework/beryl-home/REWORK.md).
The former window-owned stop/wait plan is superseded. Previously accepted exact-stop,
continuation-cancellation, draft-flush and session primitives are reusable evidence, not authority
for stopping a background thread when a nonfinal view closes.

Operator clarified that bounded technical prerequisites are to be planned and implemented
autonomously. Escalate only a material architectural change, impending scope explosion, significant
risk, or a concrete decision needing Operator input; a discovered prerequisite alone is not a stop.
On 2026-09-26 the Operator reaffirmed that lock-order corrections and other bounded technical
prerequisites must proceed autonomously; only architectural or product-scope issues warrant a pause.

Continue in the current conversation thread; do not request new-thread handoffs.

At every phase boundary, explain any blocker directly to the Operator and suggest concrete next
steps. Evidence links supplement that explanation rather than replacing it.

Apply the [simplification audit](audits/code-simplification/report.md) selectively within each
owning acceptance boundary. It is evidence, not authority or a second plan; preserve its baseline
estimates and record actual dispositions only for accepted selected findings. Keep bounded supporting simplifications within their behavioral acceptance boundary; separate
distinct outcomes or material scope growth before work begins. Keep
source names behavior-based and follow the canonical single GPUI graph. No compatibility shell,
universal resource governor, or compile-only substitute fulfills target behavior.

The final shell mounts every declared slot and feature contribution using accepted target
services and widgets. Leave deferred contributions visibly absent or unavailable until their
bounded implementation is accepted. Startup, restoration, Exit/close mounting, catalog,
transcript, Running threads, attention, approval-policy reconciliation, Settings, repair, recovery,
branch, assets and bootstrap remain explicit rework checkpoints. Preserve their separate gates.

The Operator selected [fatal crash reporting](features/crash-reporting/design.md) with a separate
reporter process and immediate failed-application termination, explicitly requiring restrained
complexity. The [system boundary](systems/crash-reporting/design.md) supersedes the proposed
in-process panic-recovery correction. Establish its bounded components before returning to CAS
regression reconciliation; final production mounting remains dependent on executable bootstrap.

The Operator authorizes defining and reconstructing target executable bootstrap. Establish the
private home candidate and its typed consumers, explicit recovery access and prepared service
composition before restore-set and native process-entry integration. Preserve each separate
acceptance boundary and the intentional removal gaps; complete registration alone does not accept
the service graph or visible startup.

# Phase 465: Implement The Private Exact Terminal Repair Adapter (pending)

Only after new exact evidence proves the route and its implementation prerequisites are ready, implement the
bounded one-request adapter with exact target/capability admission, streaming closed-item output
and typed failure. Verify no successor race, cursor follow, retry, alternate history route or
partial publication; keep durable dispatch custody and runtime mounting separate. Remaining
recovery and branch components continue to feed bounded phases from the rework tracker before 423.

Blocked on 2026-09-16: exact source does not satisfy the complete semantic item and identity proof
for Beryl's supported thread population. The existing design requires unavailable repair. Resume
only after separately authorized prerequisites prove the route; do not substitute a history
fallback or silently select a new history mode. The Operator approved proceeding with unavailable
repair and explicit-incomplete recovery; this conditional boundary does not block phases 467–470.

# Phase 732: Establish Runtime And Root Creation Mounting Readiness (finished)

Accepted source-backed mapping of the missing main-shell runtime/root consumers, available native
picker and managed admission APIs, and the complete first-runtime atomicity prerequisite.
Independent factual/planning review passed; no production or native GUI qualification is claimed.
See [readiness evidence](audits/runtime-root-creation-readiness.md). Admission, setup mounting and
ordinary New Thread confirmation remain the separate boundaries below.

# Phase 733: Implement Atomic Runtime And Root Admission (wip)

Implement the app-owned selected-path admission capability for the future New Thread controller,
derived from [runtime/root authority](features/conversation-threads/design.md#runtime-and-root-configuration),
[backend release admission](systems/backend-runtime/design.md#runtime-ownership) and
[atomic onboarding](systems/beryl-home-storage/design.md#thread-claims-and-empty-thread-acquisition).
Validate exact Host/WSL executable/directory and home facts off the GUI and storage writer; prove
the managed foreground release/profile/configuration before registry admission. Resolve canonical
duplicates without partial rows. Compose the first runtime, home root, Syndic thread/draft, catalog,
existing threadless window claim and session fallback in one revision-checked HomeCommand; later
runtime/root admission preserves selection. Retain original reconciliation and process/session/token
cleanup custody through cancellation, ambiguous outcomes and retirement. Verify injected bounded
filesystem/backend seams, duplicate/stale/foreign refusals, commit/noncommit/Unavailable and complete
closure/disposal; require focused checks and independent consequential review. No native GUI launch.

Active milestone: establish the selected-path service and the single-command first-runtime
transition for the existing threadless window. The future New Thread controller is its consumer;
completion requires qualified durable outcomes and joined validation cleanup, before GUI mounting.

The Operator resolved the WSL helper ownership prerequisite on 2026-10-06: continue through the
existing supervised `wsl.exe` path. Backend/system authority now specifies the fixed filesystem
observation helper, with app-owned path admission. Resume the unaccepted draft under that contract;
see [failure evidence](failures/runtime-root-admission.md). No implementation acceptance is claimed.

Blocked on 2026-10-06: the existing supervisor signals a saved numeric Linux process-group ID;
after the original group exits, delayed disposal can target a reused unrelated group. A proposed
original-leader request/ack protocol avoids that signal hazard, but leader exit and shell
`kill -0` do not establish trustworthy whole-group termination under permission failures.
No qualifying ownership-safe mechanism has been established. Preserve the existing `wsl.exe`
launch path; resolve the backend-owned supervision proof and its supported environment before
resuming implementation. A narrow native Linux companion is the recommended design investigation,
with explicit build/deployment and kernel requirements. The Operator approved this investigation
on 2026-10-07; no production companion is built or installed.

On 2026-10-07 the Operator approved the root supervisor/bootstrap with CAS retaining its normal
account. The selected privilege and namespace proof boundary is now in
[system authority](systems/backend-runtime/design.md#native-wsl-supervision-privileges-and-proof).
Namespace creation probes and source-backed Linux teardown feasibility remain evidence only;
see [supervisor investigation](memory/topic/wsl-process-supervision/namespace-supervisor-feasibility.md).

Still blocked: [cross-OS ownership investigation](memory/topic/wsl-process-supervision/interop-ownership-boundary.md)
shows that namespace closure plus a Windows job does not prove disposal of work created by the
WSL service outside those boundaries. For example, a managed Linux command can invoke Windows
`wsl.exe`, which starts a new Linux process outside the original namespace. Root-bootstrap approval
does not authorize dropping this part of the current lifecycle guarantee. The bounded proposal is
to own CAS and Linux descendants in its original namespace, plus the exact Windows launcher/job
members, while leaving service-created interop work outside the cleanup guarantee and preserving
interoperability. This narrower contract needs Operator selection; otherwise a qualified cross-OS
ownership mechanism is required. Never kill shared WSL services or silently disable interoperability.

Artifact build/deployment and bounded proof transport still require owning decisions and checks
after that envelope is resolved. No companion is built, installed or qualified. Do not restore the
archived draft or create an implementation prerequisite phase before architecture readiness.

The full unaccepted draft is retained at `.tmp/runtime-root-admission-resumed-draft` under a 2 MiB
limit; production source and manifests are restored. Atomic/validation/owner and selected backend
test subsets passed before later unqualified corrections. Outer consuming close/recovery custody,
token clearing and its terminal retry consumer still require complete verification and review.
See the [failure record](failures/runtime-root-admission.md#exact-linux-group-ownership) for precise
evidence, retained draft ownership and prerequisites. This phase has no acceptance claim.

# Phase 734: Mount Runtime And Root Setup And First Conversation Activation (pending)

Consume accepted admission through the main-shell New Thread secondary segment and shared bounded
thread/root picker. Mount Add runtime/Add root, coherent runtime/root pages, exact pending/error and
reconciliation presentation, and attach the first selected conversation to the existing initial
window. Qualify cancellation, duplicate suppression, retained scope/selection/focus, retirement and
same-shell first-runtime publication. Preserve existing Running threads and lifecycle behavior.

# Phase 735: Mount New Thread Root Confirmation (pending)

Complete ordinary New Thread scope/search, pending root selection and Confirm through accepted
pristine reuse/create and existing-window activation. Verify exact remembered-target updates,
cancellation/noncommit preservation, reconciliation and bounded picker behavior.
