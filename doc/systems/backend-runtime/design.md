# Goals

Define Beryl's internal backend runtime system for launching, connecting to, supervising, and recovering one pinned `codex app-server` release per configured runtime.

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
- One configured runtime is identified by one canonical Codex CLI executable path plus its Host or exact WSL distribution. Runtime identity is not inferred from `PATH` or from an environment label alone.
- Runtime admission is complete only when one production foreground session proves all four
  release-admission parts: opaque provenance from the exact Beryl-managed launch; an initialize
  response user-agent product token matching exactly `codex-cli 0.146.0`; the immutable foreground
  profile selected before the first byte and initialized with every required notification enabled;
  and exactly one effective `config/read` on that same initialized session.
- That sole admission `config/read` must prove
  `features.multi_agent_v2.enabled = true` and
  `features.multi_agent_v2.expose_spawn_agent_model_overrides = true`, with both dotted origins
  exactly `sessionFlags`. Missing, false, malformed, superseded, differently sourced, or detached
  facts fail closed, and no partial runtime record is committed.
- Admission and launch do not send capability probes, create synthetic threads, or issue destructive requests.
- Backend availability is tracked per configured executable runtime. Failure disables backend-required operations only for threads bound to that runtime and never erases or rebinds durable Beryl state.

## Launch And Listener Security

- Host launch executes the exact configured CLI path with `app-server`; WSL launch uses the exact configured distribution, working directory, and runtime-native executable path.
- Every managed launch applies the exact pinned configuration required by Beryl as one atomic configuration override. Configuration mismatch makes the runtime unavailable; Beryl does not probe around it.
- A managed app-server listens only on a Beryl-selected authenticated loopback WebSocket endpoint.
- Beryl creates one high-entropy token per launch, stores it only in memory and a per-run local
  token file, and uses it for the handshake. `beryl-backend` removes the file and clears retained
  token material on every failed spawn, launch or admission failure, cancellation, normal or
  abnormal process exit, and disposal path; cleanup is idempotent and joined before managed-process
  disposal completes.
- The managed-process owner mints production connectors tied to the exact process, runtime, executable, mode, and working directory. Caller-supplied endpoints, bearer values, labels, or detached reports cannot manufacture admission authority.
- CAS alone applies working-directory-dependent instructions, skills, sandbox, configuration, and policy. Beryl neither reads nor emulates them.

## Backend Lifecycle

- `beryl-backend` supervises every launched Host or WSL process tree and explicitly terminates it
  when `beryl-app` retirement orchestration releases the final runtime requirement or final
  shutdown begins.
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
- Foreground turns and bounded background operations use separate connections when sharing one would delay foreground streaming or terminal handling.
- Every connection is created with its fixed parser, queue, payload, page, and concurrency bounds before reading its first byte. A request-only connection cannot later become a foreground capture connection.
- Status and model lists remain cursor-paged and revision-bound; the `beryl-app` runtime
  orchestrator does not aggregate a complete backend inventory.

## Progressive Warm-Up

- Opening Beryl and restoring conversation shells does not launch CAS.
- Warm-up begins only for unique runtimes required by currently open selected threads. Catalog membership alone never warms a runtime.
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

## Protocol Ownership

- Authentication, agent execution, configuration, skills, MCP, tools, subagents, sandboxing, approvals, and provider policy remain backend-owned.
- Captured and repaired conversation history becomes Syndic-owned only through the CAS-live publication boundary.
- Turn-stream inactivity is not backend failure. Active streams may remain quiet until terminal evidence, protocol failure, transport disconnect, or backend exit.
- Timeouts apply to bounded requests. They never infer that a quiet turn is complete or that a non-idempotent request did not dispatch.
