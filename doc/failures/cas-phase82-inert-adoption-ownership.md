# CAS Phase 82 Inert Adoption Ownership

## Scope

Retained-connection service-epoch adoption after a finished persistent Beryl-home failure cut.

## Invalidated Approaches

The first Phase 82 implementation had several boundaries that appeared inert but did not own the
complete terminal state:

- unpublished replacement construction reused ordinary service startup and therefore performed a
  recovered pending read and a Syndic revision read;
- router observations retained the process-fact owner, so router lifetime still influenced stable
  connection retirement;
- disabling a driver relied on ordinary stop and destructor shutdown, which either emitted a
  WebSocket close during implicit drop or stranded an immortal thread with no explicit reclaim
  path;
- poisoned forwarding-hub recovery changed only the inert marker and could leave the detached
  endpoint and its ingester outside the returned failure owner;
- provider-broker build errors extracted only the worker permit and dropped page, channel, sink,
  control, receiver-side ingester, and startup-gate resources reached before a spawn failure; and
- adoption checked for late publication before moving ownership, leaving a window in which a late
  owner could publish between validation and commit. The first correction closed that window but
  returned a cloneable escrow reference and had no consuming post-commit retirement witness; and
- the first forwarding-order fixture injected `thread/closed` after persistent-failure shutdown,
  when the backend reader could no longer select any new wire observation, so its timeout proved
  only an impossible test ordering rather than the adoption barrier; and
- the second fixture expected failure quarantine and adoption to progress while that selected close
  was still paused, but persistent-failure quiescence itself waits for the backend reader and driver
  observation to settle, before any finished quarantine can exist; and
- the third fixture sent an after-adoption wire close even though successful Phase 82 adoption is
  intentionally unpublished and keeps the stable driver parked until later publication; and
- the driver treated a poisoned forwarding-hub lookup as a loop exit followed by unconditional
  backend shutdown. Because its execution guard held the same adoption-slot mutex needed by inert
  conversion, the driver could emit a WebSocket close before `Disabled` became visible.

## Evidence

The ordinary service constructor performs startup recovery before workers become available. A
closed startup gate cannot undo reads that already happened during construction.

`ManagedBackendSession::shutdown` and its destructor perform transport close work. Waking a
disabled driver through the ordinary stop flag therefore violates the implicit-drop rule, while a
driver that can never be woken also makes the error's documented explicit disposition false.

A poisoned mutex still contains its protected endpoint. Recovering only a boolean state leaves the
old ingester reachable and can drop the forwarding sink outside the inert authority. Similarly,
fixed-capacity broker preparation acquires meaningful resources before thread spawn; returning only
the worker is not complete partial-construction ownership.

Late cut publications use the coordinator and adoption escrow as two ordered locks. An `is_empty`
observation followed by a separate ownership move is not a linearization point. Even after an
atomic adoption commit, old sources may publish into the escrow until later recovery proves their
retirement, so successful adoption alone is not publication authority.

A before-cut ordered observation must be selected while the old reader and ingester are still
live. A deterministic pause then proves persistent-failure completion remains pending until that
old observation fully settles. Only after quiescence may a finished quarantine reach the adoption
barrier. The proof must not bypass or weaken that earlier frontier merely to manufacture overlap
with a later hub-lock attempt.

An adopted-but-unpublished service cannot consume a new backend frame. Phase 82 can prove its
post-cut forwarding selection by invoking the exact stable hub path used by the already-bound sink;
live wire resumption belongs to later publication and must remain behind the closed startup fence.

An adoption-slot execution guard cannot coexist with a fallthrough shutdown on coordination loss.
The losing inert conversion is blocked on that guard, so the driver itself must move the backend
and worker admission into a non-executable quiesced state before releasing it. Stop notification or
implicit Drop is not shutdown authority.

## Course Correction

Unpublished construction now has an explicit dormant startup state. It performs no recovered read,
storage read, durable convergence, scheduler dequeue, or worker activation. Ordinary construction
retains its separate ready path.

The stable connection wrapper uniquely owns process-fact retirement; routers retain only read-only
process observations. Inert failure disables each capacity-one driver slot and parks its backend
session indefinitely for implicit drop. The sole explicit consuming disposition changes that exact
slot to terminal stop, releases retained admissions, intentionally shuts down the backend outside
authority locks, and joins the driver.

Poison recovery atomically marks the forwarding hub inert and takes the exact endpoint, then
cancels its ingester after releasing the hub lock. The failure result retains that endpoint in
preallocated storage. Provider-broker preparation uses staged owning errors, including a launch
escrow that preserves the receiver-side ingester when OS thread spawn fails.

The one-time driver startup drain now runs as the first adoption-slot-guarded cycle. Hub or epoch
coordination loss changes that guarded cycle to stable quiescence. Exact-cut inert conversion folds
quiesced and every other preterminal slot state into `Disabled`; only explicit disposition or a
typed proven ordinary-lifecycle exit may authorize backend shutdown. There is no unconditional
driver shutdown epilogue.

Adoption commit holds coordinator and escrow locks through its allocation-free ownership move.
Success retains a non-cloneable adoption fence. After old-source retirement, Phase 84 must consume
that fence in one second coordinator-plus-escrow validation and receive a one-shot retirement
witness. Retirement-first late owners remain terminally escrowed; publisher-first invalidates the
fence. Process publication can consume only the witness and occurs outside coordinator locks.

The forwarding-order proof selects and pauses `thread/closed` against the still-live old epoch,
starts persistent-failure progression, and proves the cut cannot finish while that same hub barrier
remains held. After release, the old close settles, failure quiescence and recovery finish, the test
observes the exact adoption hub-lock cut, and later closes must select only the replacement router.
Those post-cut closes enter the stable hub directly while the driver remains parked; Phase 84 owns
the separate proof that publication resumes backend input.

## Required Proof

Tests must prove dormant construction preserves a subsequently armed read fault; stable process
retirement is independent of router observations; implicit inert drop is bounded and sends no
backend frame; explicit disposition releases and joins the parked driver; poisoned hubs retain and
dispose the exact endpoint; every broker build stage owns all reached resources; late authority
before commit returns one inert owner; clean retirement yields one witness; and late authority
after commit prevents witness issuance while startup remains closed. A deterministic guard-held hub
poison race must quiesce before exact-cut disable, and disabling before the first driver cycle must
prevent startup reconciliation from touching the backend.
