# Shell And Lifecycle

This supplement is normative only for its bounded beryl-app shell and lifecycle role and is
governed by [design.md](design.md). It does not independently declare engineering rigor.

## Process Service Graph And Windows

- One ordinary process service graph is constructed only after the configured Beryl home validates.
  A busy home creates only its compact surface, and an unreadable home creates no ordinary
  conversation window from unvalidated state.
- The graph composes typed home domains, health and readiness publishers, runtime/root services,
  bounded catalog and activity services, settings and theme services, durable-job coordinators,
  and explicitly bounded worker pools. Shared versioned facts never retain or mutate GPUI views.
- `beryl-app` is the sole GPUI shell-composition boundary described by
  [GUI integration](../../../doc/gui/integration.md). Feature controllers mount only into declared
  windows and slots.
- Each main window owns one controller, `WindowId`, exact selected-thread claim, bounded navigation
  and transient interaction state, composer host, transcript host, and presentation projections.
  No main-window controller or GPUI entity is shared between windows.
- Construction checks the stable process main-window count before allocating the controller or OS
  window. The [main-windows feature](../../../doc/features/main-windows/design.md) owns capacity and
  visible creation, close, Exit, and restore behavior; the app creates no overflow or hidden
  deferred controller.
- Settings, busy-home, and home-failure windows are distinct top-level controllers and never receive
  main-window claims or restore records.

## Startup And Activation

- Startup accepts only the validated minimal session plus each restored window's selected thread,
  current draft, visible range, compact editor frontier, transcript seed, and placement facts. It
  retains bounded seeds and never preloads a complete draft or history.
- Later activation prepares one revision-consistent target bundle containing claim, durable draft
  selector, fresh candidate-session head, composer and transcript seeds, title, lineage,
  runtime/root, and window-local projection facts.
- Each window owns at most one unpublished target composer. The prior coherent selection remains
  authoritative until promotion fences and flushes it and publishes the complete target bundle.
- Cancellation, preparation failure, source drift, stale completion, rejection, supersession, or
  disposal releases the unpublished target and preserves the complete prior bundle. An unmutated
  fresh target is abandoned only through the typed Syndic boundary; a mutated or ambiguous target
  stays in its ordinary operation or reconciliation custody.
- Startup and activation presentation follow the
  [conversation-threads feature](../../../doc/features/conversation-threads/design.md); this
  package owns only typed preparation, fencing, and publication.

## Typed Home Integration

- The package never opens Fjall, reads raw keyspaces, or constructs storage encodings. It receives
  typed domain handles, repositories, revisions, and command outcomes.
- Service configuration supplies a nonzero minimum capture reserve to `beryl-home-store` and
  receives the home-store-owned opaque turn-start space requirement after dependency validation.
  The app neither knows nor recreates the dependency's fixed budget arithmetic.
- Direct submission and accepted-input promotion pass that same opaque requirement intact to the
  typed free-space query. Callers cannot provide a path-specific or pre-aggregated total.
- Correctness-sensitive success is published only after the typed home command's required durability
  barrier. Health transition invalidates mutation authority by generation while windows may retain
  only last coherent inert presentation.

## Same-Home Replacement Contribution

- `beryl-app` contributes one complete unpublished app service graph to the process-wide same-home
  replacement. Before candidate construction, the old graph fences admission and disposes its
  connections, brokers, routers, schedulers, projections, leases, workers, custody, subscriptions,
  and service-local handles; none transfers.
- Candidate construction, durable convergence, and supervisor attachment remain unpublished. The
  app exposes the candidate only through outer atomic whole-stack publication.
- Failure before publication disposes the candidate and publishes no authority. After publication,
  schedulers and projection consumers establish fresh authority from durable typed facts under the
  new generation.
