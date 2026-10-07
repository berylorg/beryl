# Goals

Define Beryl's internal backend runtime system for launching, connecting to, supervising, and recovering one pinned Codex App Server release per configured runtime.

Preserve exact runtime, root, process, Syndic-thread, CAS-thread, turn, authentication, policy, sandbox, and protocol boundaries while rebuilding failed runtime services from durable authority.

## Non-goals

- Defining user-visible recovery copy, disabled states, or runtime/root workflows.
- Bundling, installing, replacing, or modifying Codex.
- Exposing operator-managed or unauthenticated app-server listeners.
- Treating backend thread enumeration or historical reads as Beryl catalog or transcript authority.
- Dynamically probing backend capabilities or destructive experimental methods.
- Retaining or adopting failed connection/service internals after Beryl-home recovery.
- Providing hard-stop escalation or managing CAS process memory.

# Decisions

## Runtime Ownership

- Beryl integrates with CAS as an out-of-process client and owns every app-server process it launches.
- `beryl-app` owns process-wide runtime interest, launch and retirement orchestration, and the one
  single-flight same-home recovery supervisor. `beryl-backend` owns construction, process-tree
  supervision, bounded lifecycle termination, and disposal of each managed app-server process.
- Runtime/root admission also uses the existing exact `wsl.exe` launch and Linux process-tree
  supervision boundary for fixed filesystem observations. `beryl-app` owns selected-path and
  environment admission; `beryl-backend` owns construction, bounded observation and joined disposal
  of these temporary helpers. The helper accepts only executable, directory or user-home
  observation in one exact distribution; it exposes no arbitrary command execution, CAS session,
  release proof or durable authority. Canonical paths are bounded typed facts, not launch admission.
- Filesystem observations run off the GUI and storage writer. Cancellation, timeout, malformed
  output and failure retain exact process/reader cleanup custody until joined disposal. Admission
  cannot publish a runtime/root or repeat a probe while the previous helper remains unsettled.
  Helper cleanup cannot affect an existing runtime process or its accepted work. A missing utility
  yields failure without installation, alternate distribution or guessed path/home substitution.
- One configured runtime is identified by one canonical Codex App Server launch executable path plus its Host or exact WSL distribution. Runtime identity is not inferred from `PATH` or from an environment label alone.
- Runtime admission is complete only when one production foreground session proves all four
  release-admission parts: opaque provenance from the exact Beryl-managed launch; an initialize
  response leading user-agent product token matching exactly `beryl/0.146.0`; the immutable foreground
  profile selected before the first byte and initialized with every required notification enabled;
  and exactly one effective `config/read` on that same initialized session. The product is
  `<client_name>/<codex_version>`: Beryl supplies the name and CAS supplies its compiled package
  version, independently of caller `clientInfo.version`. The full TUI is not required for compatibility.
- That sole admission `config/read` must prove
  `features.multi_agent_v2.enabled = true` and
  `features.multi_agent_v2.expose_spawn_agent_model_overrides = true`, with both dotted origins
  exactly `sessionFlags`. Missing, false, malformed, superseded, differently sourced, or detached
  facts fail closed, and no partial runtime record is committed.
- Admission and launch do not send capability probes, create synthetic threads, or issue destructive requests.
- Backend availability is tracked per configured executable runtime. Failure disables backend-required operations only for threads bound to that runtime and never erases or rebinds durable Beryl state.

## Launch And Listener Security

- Every runtime retains one explicit executable launch form: standalone Codex App Server or Codex CLI. Host launch executes the exact configured path; WSL launch uses the exact configured distribution, working directory, and runtime-native executable path. Standalone launch passes server arguments directly; CLI launch prepends exactly one `app-server` subcommand. Both use the same authenticated foreground release admission and supervision. No filename inference, alternate-form retry or TUI interaction is permitted.
- The admitted launch form follows the exact runtime through registration, ordinary launch, restoration and same-home recovery. Managed-launch provenance includes the form, so detached proof for another form cannot authorize registration or execution. Canonical executable identity remains unique irrespective of form; selecting an already registered path resolves its existing record and retained form.
- Every managed launch applies the exact pinned configuration required by Beryl as one atomic configuration override. Configuration mismatch makes the runtime unavailable; Beryl does not probe around it.
- A managed app-server listens only on a Beryl-selected authenticated loopback WebSocket endpoint.
- Beryl creates one high-entropy token per launch, stores it only in memory and a per-run local
  token file, and uses it for the handshake. `beryl-backend` removes the file and clears retained
  token material on every failed spawn, launch or admission failure, cancellation, normal or
  abnormal process exit, and disposal path; cleanup is idempotent and joined before managed-process
  disposal completes.
- Executable composition supplies one immutable canonical host temporary-directory root for
  launch token files, outside the Beryl home. Host runtimes use that exact host path. WSL runtimes
  use the drive-backed `/mnt/<drive>/...` projection into the exact selected distribution;
  UNC, device and otherwise unmappable paths make that launch unavailable without another
  directory, transport or mapping fallback. The backend's token-hash and authenticated admission
  checks still apply. The directory configuration retains no runtime catalog or home generation;
  each launch derives its runtime-native path from the current validated runtime record. Backend
  ownership of individual random token files and their cleanup remains unchanged.
- The managed-process owner mints production connectors tied to the exact process, runtime, executable, mode, and working directory. Caller-supplied endpoints, bearer values, labels, or detached reports cannot manufacture admission authority.
- CAS alone applies working-directory-dependent instructions, skills, sandbox, configuration, and policy. Beryl neither reads nor emulates them.

## Native WSL Supervision Privileges And Proof

- Native WSL supervision uses a Beryl-owned Linux companion through the exact configured
  distribution's `wsl.exe` root launch. Elevated execution is limited to Beryl-owned supervision
  mechanics; it does not select root as the CAS workload account.
- The workload preserves the normal launch account's UID, primary and supplementary groups,
  home, execution root and required environment. Credentials are established before executing
  the selected CLI, shell/profile code or fixed filesystem observation. The root bootstrap's
  environment cannot substitute for the normal account's facts.
- The companion owns a private PID namespace without a new user namespace, with a dedicated init
  and a private mount namespace containing namespace-correct proc. Mount propagation is private;
  this lifetime boundary supplies no new filesystem/network restriction or CAS policy engine.
- The supervisor retains a pidfd for the original namespace init before workload execution is
  released. Destructive disposal uses that original capability, never a subsequently resolved
  numeric PID or process-group ID. Exact init reaping proves removal of namespace members;
  successful signalling, deadline expiry or Windows-wrapper exit does not supply that proof.
- Privileged companion code belongs to the trusted Beryl release. Runtime launch does not compile,
  download or install it, accept an arbitrary companion path, or expose a privileged command
  listener. Missing or untrusted artifacts cannot authorize elevated execution.
- Namespace closure proves only its Linux membership. It cannot establish closure of Windows
  interoperability processes or Linux work created outside the namespace by a service. A Windows
  job proves only its own membership. Neither proof alone or in combination manufactures ownership
  of work created by shared services. For WSL, managed cleanup covers CAS and every member of its
  original private Linux namespace, the explicitly owned companion roles and the exact Windows
  launchers. Service-created Windows applications and Linux processes outside that namespace are
  external work and may outlive retirement. Their survival does not make this owned boundary's
  proven disposal incomplete. Host process-tree ownership is unchanged.
- Supervision does not disable the selected environment's interoperability or terminate a shared
  WSL service or distribution to obtain cleanup. Unproved disposal within the owned boundary
  retains its original owner and replacement fence; no outside process is adopted by enumeration.

## Native WSL Composition And Distribution

- [`beryl-wsl-supervisor`](../../../crates/beryl-wsl-supervisor/doc/design.md) owns the Linux
  companion and its bounded control codec. `beryl-backend` owns launch/protocol orchestration,
  original cleanup custody and filesystem observations; `beryl` supplies the immutable artifact
  descriptor. Application admission, runtime interest, CAS policy and durable authority stay with
  their existing owners.
- The supported companion artifact is a static non-PIE `x86_64-unknown-linux-musl` executable, named
  `beryl-wsl-supervisor-linux-x86_64` beside the Windows desktop executable. The desktop build
  embeds the exact artifact SHA-256 and protocol version; composition accepts only that sibling
  artifact in the trusted Beryl release directory. It retains an opened host file that denies
  write/delete sharing while launches can use the artifact, checks the embedded digest, and uses
  its canonical drive-backed path in the exact distribution. Untrusted replacement, missing or
  mismatched bytes, unsupported architecture or mapping failure makes WSL unavailable. There is
  no runtime build/download/install or arbitrary runtime-selected companion path.
- Source builds without the companion descriptor can run Host runtimes and expose WSL as
  unavailable; they cannot qualify WSL acceptance. Build the Linux artifact before a bundled
  desktop build. Release identity, digest generation and artifact pairing are build-owned Rust
  work, not a mutable runtime manifest. The trusted Operator account and release directory are
  the artifact trust boundary; this is not protection against a malicious trusted account.
- Native qualification uses the selected static Linux artifact and integration-test binaries in
  the exact WSL2 distribution. Tests must prove identity/environment preservation, namespace
  teardown, all companion/launcher/reader joins and unrelated-process preservation. Windows-only
  simulations or successful cross-compilation cannot replace native lifecycle evidence.

## Backend Lifecycle

- `beryl-backend` supervises each managed Host process tree or the WSL owned boundary above and explicitly terminates it
  when `beryl-app` retirement orchestration releases the final runtime requirement or final
  shutdown reaches managed-runtime disposal after the CAS-live graceful execution and durable
  window/session barriers. Starting shutdown is not authority to terminate still-unsettled turns.
- Bounded process-shutdown escalation belongs only to managed-runtime lifecycle disposal. It is not
  turn control, a hard stop, terminal-history evidence, or authority to terminate a selected turn.
- One Beryl process uses at most one active managed app-server process per configured runtime.
- Backend process lifetime is independent from client-connection lifetime. Dropping one connection does not stop a process still needed by another window or operation.
- Runtime interest is process-wide and shared by the `beryl-app` runtime orchestrator. Releasing
  one window's interest does not stop a runtime still required by another window or an already
  required operation. After the final runtime interest disappears and no required operation
  remains, that orchestrator retires its app-owned drivers, ingesters, brokers, routers,
  schedulers, projections, queues, and workers, then directs `beryl-backend` to retire its client
  and session internals, managed process tree, listener, token material, queues, and workers.
- The `beryl-app` runtime orchestrator owns one opaque `runtime activity period` identity for each
  continuously usable published runtime service and supplies it to every matching Syndic activity
  projection mutation. Thread switching, turn completion, and later turns retain that identity.
  Managed-process restart, runtime teardown or replacement, and same-home service replacement end
  it before old-period facts can publish. An unpublished candidate's fresh identity becomes current
  only in the atomic replacement publication; late facts for an ended period are rejected.
- Runtime activity identity has a process-owned lifetime fence and one lazily enrolled durable
  period token. The process-local readiness counter is never cast into durable identity. The first
  exact thread enrollment for a runtime allocates its token from that command's committed home
  revision; the writer stores the same value in the thread's existing Activity work-period field.
  The runtime owner serializes this first enrollment and retains only one token and at most one
  unresolved enrollment custody object. Concurrent demand waits through existing bounded runtime
  demand; it cannot allocate competing tokens or retain a runtime-wide thread map.
- A token becomes usable only after exact committed classification. A known noncommit permits a
  new preparation against a fresh revision; an indeterminate result retains the original witness
  and blocks token publication until reconciliation. Retirement revokes the lifetime fence before
  draining enrollment and producer work; it never transfers a token or unresolved old-generation
  authority into a replacement runtime. Canonical recovery may still settle original custody.
- Runtime readiness and view warm-up need no Activity enrollment command. Canonical input may be
  accepted before a runtime exists. Enrollment occurs only for exact admitted producer work before
  its first current-period activity publication, and does not itself launch CAS or dispatch a turn.
  Subsequent turns and other threads on that runtime reuse the proven token. The immutable thread
  runtime/root binding prevents a thread from switching between concurrent runtime periods.
- A runtime may end after enrollment commits but before provider dispatch. After its original
  enrollment custody settles, the replacement preserves the same proven-undispatched pending turn
  and explicitly replaces that source's retired Activity period using exact head and pending-dispatch
  evidence. It allocates or reuses only its own proven token. It cannot reuse the ended attempt's
  token, manufacture terminal history, or treat activated provenance without a CAS-turn id as
  nondispatch. The storage package owns bounded authentication and atomic period replacement.
- Foreground turns and bounded background operations use separate connections when sharing one would delay foreground streaming or terminal handling.
- Every connection is created with its fixed parser, queue, payload, page, and concurrency bounds before reading its first byte. A request-only connection cannot later become a foreground capture connection.
- Status and model lists remain cursor-paged and revision-bound; the `beryl-app` runtime
  orchestrator does not aggregate a complete backend inventory.

## Progressive Warm-Up

- Opening Beryl and restoring conversation shells does not launch CAS.
- Warm-up begins only for unique runtimes required by currently open selected threads or exact
  admitted process-owned work. Required work includes queued promotion admitted by the scheduler,
  pending request handling, compaction, continuation, and terminal capture/convergence independent
  of GUI selection. Catalog membership alone never warms a runtime.
- The `beryl-app` runtime orchestrator coalesces concurrent interest in the same runtime and fans
  the result to interested windows.
- Cancelling one interest does not cancel launch while another interest remains.
- Cancelling the final interest permits the same orderly zero-interest retirement once no required
  operation remains; an in-flight required operation is not abandoned merely to reach zero interest.
- Launch, exact-release admission, retry, and shutdown run off the GUI thread with bounded request and process timeouts.

## Neutral Maintenance Roots

- Maintenance operations that must avoid project instructions use an empty runtime-local directory reserved for that managed runtime.
- Host and WSL directories are keyed by non-secret Beryl-home and runtime identity, contain no durable conversation authority, and are recreated or validated as empty before use.
- Beryl never falls back to a project root when neutral-root preparation fails.

## Pinned Protocol Boundary

- The exact supported release contract includes normal foreground notifications, thread start and steering, exact soft interruption, context compaction, native continuation and fork, dynamic tools, generated-image `savedPath`, and the bounded historical-repair adapter defined by `doc/systems/cas-live-syndic-transcript/design.md`.
- Generated-schema and pinned-release source evidence for these boundaries is recorded under
  `doc/memory/topic/codex-app-server/`. Runtime admission combines that semantic proof with the exact
  initialize version and required effective configuration facts; it does not use capability,
  model-list, private-steering, user-target, synthetic-target, or diagnostic-text probes.
- CAS-native collaboration owns subagent creation and lifecycle. Its native `spawn_agent` tool
  exposes optional `model` and `reasoning_effort` selection to the orchestrating model. Each
  explicit value precedes its configured subagent default. If neither resolves, the child keeps the
  parent profile. Reasoning alone applies to the parent model; a selected model without resolved
  reasoning uses that model's catalog default; and a resolved pair is validated together. Context
  selection through `fork_turns` is independent of profile selection, including when full parent
  history seeds the child. Beryl does not require a child to use its parent's profile, register an
  imitation spawning tool, or maintain a parallel child-agent registry.
- Admission rejects an effective configuration known from the pinned contract to disable native
  subagent model or reasoning selection. It never discovers those inputs by running a probe turn.
- CAS thread lists and ordinary historical turn reads are not catalog or transcript surfaces. Only the repair adapter may perform the exact bounded terminal-turn read authorized by the CAS-live system.
- Hosted and standalone media producers are admitted only when the pinned release contract names their exact normalized form. Parser tolerance for an unsolicited item does not admit a producer.
- Unsupported ordinary operations are unavailable for that pinned release. Beryl does not negotiate experimental fallbacks or invoke user-thread methods merely to test them.

## Exact Soft Interruption

- Active-turn interruption accepts one of two non-interchangeable typed authorization families. Both
  bind the exact already loaded foreground session, CAS thread and turn, runtime/process generation,
  loaded-session generation, and sole foreground driver.
- Durable authorization additionally carries the exact admitted stop operation and sole attempt.
  Its admission, join, approval, and convergence ordering is defined by
  `doc/systems/cas-live-syndic-transcript/design.md`.
- Volatile pre-admission authorization is eligible only after exact proof that durable admission
  failed before reaching a writer or returned `NotCommitted`. `Committed`, `Indeterminate`, and
  any state in which durable stop authority may exist are ineligible.
- Volatile authorization is process-local and single-use on that same existing authenticated
  foreground target and driver. A detached, replacement, resumed, request-only, or newly selected
  session cannot consume it. The driver cancels the exact target's process-local continuation intent
  before consuming the authorization or dispatching `turn/interrupt`.
- Volatile authorization supplies no durable operation, join, retry, restart recovery, durable
  success, or terminal claim. Matching request acceptance remains nonterminal; the ordered live
  stream or authoritative target loss determines convergence.
- The foreground driver serializes an interruption authorized by either family with provider polling,
  approval responses, target closure, and terminal handoff. Each closed request-outcome family
  distinguishes matching acceptance, pinned rejection, proven local nondispatch, and completion
  unknown after possible dispatch without making the families interchangeable.
- The backend boundary never retries interruption. A lost or replaced connection cannot recreate
  either dispatch authority.
- Hard stop, diagnostic hard stop, child/subagent termination, command-process termination, coarse
  background-terminal cleanup, and process shutdown as turn control are unsupported and are never
  probed or invoked.

## Store And Connection Recovery

- These are ordinary returned-error recovery rules. An application panic follows
  [fatal crash reporting](../crash-reporting/design.md), not runtime convergence or replay. Existing
  OS-owned managed-process lifetime containment remains responsible for parent-death cleanup.

- A failed Beryl-home generation fences new runtime commands that require durable publication.
- During the bounded outage interval, foreground capture may retain only the hard-limited process-local facts defined by the CAS-live system. The runtime layer adds no second buffer or durable authority.
- Fresh-service recovery runs in this order: fence new durable commands; close and dispose the failed
  service; reopen the same home as an unpublished `reopening` candidate with a fresh writer and
  candidate-only handles; construct a fresh backend/app service and fresh connections; converge
  durable pending, stop, compaction, and repair obligations behind the recovery publication fence;
  attach the supervisor; atomically publish the complete replacement as the newer healthy
  generation; and only then reacquire CAS projections from durable Syndic binding authority.
- Every connection, driver, broker, router, projection registration, loaded-session registration,
  lease, candidate, scheduler, and worker derived from the failed generation is closed and disposed.
  No such object, stable core, service epoch, or quarantined connection crosses the publication cut.
- Failure or cancellation during candidate construction, durable convergence, or supervisor
  attachment publishes none of that candidate. `beryl-app` disposes the complete unpublished
  candidate by joining and releasing its app-owned drivers, ingesters, brokers, routers,
  schedulers, projections, queues, and workers. `beryl-backend` separately joins and releases the
  candidate's backend client and session internals, managed process tree, listener, token material,
  queues, and workers before recovery reports failure.
- Before any broker, connection, or service releases a home-store command outcome, it synchronously
  transfers any `Indeterminate` custody value to the owning home's reconciliation registry as
  required by `doc/systems/beryl-home-storage/design.md`. Once the registry owns that exact scope,
  broker or service cancellation, failure, retirement, candidate disposal, and managed app-server
  process exit cannot retract or drop it.
- Any in-flight non-idempotent request remains classified from its last exact durable and transport evidence. Recovery does not resend it merely because a fresh connection exists.
- Backend process replacement likewise creates fresh connection and projection authority; it cannot inherit interruption, steering, repair-response, or capture authority from the old process.

### Same-Home Recovery Composition

- The process-wide app supervisor outlives replaceable service graphs. It owns one exact configured
  home recovery attempt, one retry deadline and the publication slot. Concurrent failure notices
  coalesce for the same failed generation; stale notices and completions cannot replace a newer
  graph. Retry uses the home-store delay sequence and cannot overlap disposal or candidate work.
- Retirement separates runtime disposal from storage recovery custody. Every old graph component
  fences admission and joins its workers before reporting its exact generation retired. Only
  storage-package recovery custody survives in the failed home owner; reopening preserves its
  lock and reconciliation registry while disposing and reconstructing backend storage as required
  by the storage contract. No old graph or connection accompanies that owner into reopening. Terminal
  application close remains distinct and creates no recovery handoff.
- The outer composition owner requires retirement of the complete old graph before consuming that
  failed home owner through `HomeStore::recover_same_home`. It never releases and reacquires the
  home lock, opens another path, or falls back to initial creation. Unproven component retirement
  blocks reopening; the failed home remains under conservative storage custody.
- Reopening yields the storage package's unpublished `HomeRecoveryCandidate`. Fresh typed domain
  handles and service factories use only that candidate's generation and explicit recovery access.
  Initial and replacement construction share service implementations and convergence rules, while
  their owned candidate capabilities remain distinct. No adapter admits an old healthy handle.
- The complete fresh graph, durable convergence result and supervisor attachment are prepared
  before publication. Attachment binds the exact attempt and candidate generation without opening
  admission or installing another supervisor. Publication verifies that the slot still owns that
  attempt, consumes the storage publication capability and installs the complete graph in one
  serialized outer transition. No fallible constructor or attachment follows storage publication;
  only release of prepared work and fresh projection acquisition follows visibility.
- Cancellation before publication fences and joins the candidate before its recovery candidate is
  aborted back to failed storage custody. Failed reopening returns the original failed owner for
  the next bounded retry. Candidate writes retain their actual committed or indeterminate outcome;
  disposal never rolls them back or treats them as authorization to repeat external work. A
  shutdown racing publication is serialized by the same owner: it either cancels the unpublished
  attempt or closes the newly published complete graph.
- Verification proves old-worker disposal before reopen, retained reconciliation and lock custody,
  exact candidate identity, construction/convergence/attachment failure, cancellation, stale attempt
  rejection and publication races. Component acceptance does not substitute for complete-graph
  publication or running-window recovery evidence.

### Interrupted Exit During Same-Home Recovery

- The process supervisor retains the reported-failed Exit's identity and bounded immutable session
  evidence outside replaceable graphs. That evidence records the exact configured home, intended
  complete session/window revisions and placements, known command outcome and original failure.
  It conveys no old-generation read, write, receipt or service authority. Pending reconciliation
  stays owned by the home-store registry and follows its exact-handle recovery protocol.
- Reporting failure latches cancellation of that Exit. A later exact-new resolution preserves the
  successful durable outcome but cannot resume shutdown. Before reporting failure, healthy exact
  reconciliation may still satisfy an active Exit's ordinary readiness contract.
- Replacement preserves known outcomes and reconciles only eligible pending outcomes. Proven
  noncommit needs no inverse session write. Proven committed Exit state requires a separate
  revision-checked session transition back to Running before live session mutation resumes.
  This transition preserves committed placements, window records, selections and paired claims;
  it is not startup restoration and does not undo the original write.
- Fresh candidate handles validate the retained evidence against the same home's exact complete
  session/window state before preparing that transition. Prior-generation receipts are historical
  evidence only. Changed membership, revisions or placements, collision, unsupported successor or
  unproven outcome refuse resumption rather than adopting newer facts. No failed-generation handle,
  editor adapter, service lease or execution capability crosses the replacement publication cut.
- The resume command has its own retained ordinary command outcome. Noncommit keeps dependent
  interaction fenced; another automatic recovery attempt may prepare it only after proving that
  noncommit and freshly validating the unchanged source. Indeterminate resume is reconciled before
  any repeat. Proven commit is preserved through later failure and is never repeated; fresh
  validation must prove its exact resulting Running state. Terminal uncertainty remains unavailable.
- Candidate convergence includes this session settlement before whole-graph publication. Fresh
  graph bindings attach to the preserved native windows and resident presentation; their old
  generation resources must first retire. Only complete fresh bindings, healthy session settlement
  and exact draft/work cleanup permit atomic interaction-gate release and completion of the
  cancelled request. No intermediate step grants native destruction or quit authority.
- Verify committed and noncommitted original outcomes, pending and terminal uncertainty, stale
  evidence, failed/ambiguous resume, duplicate and stale completions, preservation of windows and
  claims, and absence of automatic Exit after recovery. Independently review this replacement and
  persistence boundary.

### Interrupted Ordinary Close During Same-Home Recovery

- A nonfinal native-destruction failure while the original home remains Healthy uses the separate
  [app-owned healthy restoration path](../../../crates/beryl-app/doc/design-shell-lifecycle.md#nonfinal-native-close-recovery).
  State supplies ordinary authenticated access to the same exact removed-member transition.
  No runtime service replacement or all-work barrier is required. If that home subsequently fails,
  the process owner transfers original removal and restoration outcomes plus protected resident
  custody into this replacement protocol; an already committed restoration is validated, never
  repeated. Home failure must be real, and exact native-survival settlement remains a prerequisite
  for restoring the cancelled close's member.
- A reported-failed ordinary close remains cancelled. Its process owner retains one bounded,
  immutable State-owned removal evidence value and the original exact command outcome outside
  replaceable graphs. It preserves the native window, resident content and process window
  reservation. The evidence contains no old service handle or execution authority.
- A pending removal follows home-store exact reconciliation. Proven noncommit requires validation
  of the unchanged original membership and claim, with no inverse write. Proven commit requires
  a separate State-owned recovery transition that reinstates the exact removed member and paired
  claim. Absence alone, collision, an unproven result or changed membership grants no restoration
  authority. The owner validates the same configured home and the original outcome with fresh
  candidate handles before preparing the transition.
- That transition preserves original window identity, selection, placement and target, advances
  window and claim revisions, and preserves the claim generation and active state. It never
  restores an old revision, selects another thread or overwrites a conflicting record or claim.
  The surviving set and fallback must still equal the exact post-removal facts. State owns the
  precise source/result qualification and fixed reconciliation closure.
- The restoration has independent ordinary outcome custody. Proven commit is validated and never
  repeated, noncommit permits a later automatic recovery attempt only after fresh exact source
  validation, and indeterminate outcome must reconcile before another write. Unsupported successor
  state or terminal uncertainty retains the window read-only and keeps dependent actions unavailable.
- Session settlement precedes whole-graph publication and fresh resident binding. Renewed bindings
  use the restored claim revision; no pre-removal claim capability is revived. The process owner
  releases interaction only after complete fresh bindings, draft/work settlement and healthy exact
  durable membership. It then completes the cancelled close request; only fresh activation may
  close the window. Nonfinal close itself never enters the all-work execution barrier.
- Verify noncommit, commit with later failure, exact-new reconciliation, failed or ambiguous
  restoration, stale and duplicate delivery, conflicting membership/claims, and preservation of
  the same surviving native window. Final close uses the same recovery rule for its empty
  post-removal set. Independently review the persistence and lifecycle integration.
- Failed-home recovery captures every preserved resident through a separate failed-generation
  retirement boundary, including residents not captured before the failure and an unsaved
  candidate whose flush failed. It fences input, drains admitted work and retains bounded exact
  candidate/root/history, prior durable selector and original publication outcome custody.
  Ordinary healthy-close admission and clean-checkpoint retirement are not substitutes.
- After old graph retirement, fresh same-home candidate access authenticates this custody and
  settles original publication outcomes before any new save. State membership/claim settlement
  precedes dependent resident reconstruction. Syndic owns candidate provenance and publication;
  Asset qualifications remain typed and atomic. Already-saved facts require validation only;
  proven noncommit of an unsaved checkpoint permits separately retained candidate publication.
  Indeterminate, conflicting or unsupported facts retain custody and interaction fencing.
- Reconstruction preserves the same protected native editor and its captured input/history
  state. Successful save alone grants no graph publication or interaction release. Complete
  fresh graph bindings, exact saved-checkpoint authentication and all resident/work settlement
  still precede coherent reopening. This route creates no new editor session, adopts no unrelated
  unpublished state and does not relax clean interrupted-Exit validators.

## Protocol Ownership

- Authentication, agent execution, configuration, skills, MCP, tools, subagents, sandboxing, approvals, and provider policy remain backend-owned.
- Captured and repaired conversation history becomes Syndic-owned only through the CAS-live publication boundary.
- Turn-stream inactivity is not backend failure. Active streams may remain quiet until terminal evidence, protocol failure, transport disconnect, or backend exit.
- Timeouts apply to bounded requests. They never infer that a quiet turn is complete or that a non-idempotent request did not dispatch.

# Engineering Rigor

Profile: `production-application/v2`

Modifiers:

- `privileged-access/v1`
- `external-side-effects/v2`

The Operator-selected Codex executable and OS account are trusted. Detached endpoints, tokens,
reports, and malformed protocol inputs cannot establish authority.
