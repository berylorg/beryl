# Goals

Provide the app-neutral fixed-capacity pages, channels, range streaming, and backpressure
primitives used by Beryl process services.

Keep known large-data choke points bounded and observable without imposing one accounting model on
unrelated services or limiting the logical size of streamed content.

## Non-goals

- Owning provider, storage, application, GUI, renderer, media, or feature semantics.
- Selecting product limits for consumers or constructing a global resource runtime.
- Owning operation cancellation tokens, semantic cancellation policy, or service lifecycle.
- Providing typed resource currencies, universal allocation capabilities, reservations, or exact
  process-memory accounting.
- Requiring every allocation or returned value to carry an admission charge.
- Treating pages, buffers, or channel messages as protocol or durable records.
- Providing a whole-value compatibility buffer, raw spool, unbounded queue, or independent local
  governor.

# Decisions

## Public Boundary

- Consumers configure each page pool or channel with a finite capacity appropriate to their own
  payload and concurrency contract. Configuration rejects zero, unrepresentable, or
  multiplication-overflow capacities before allocating package-owned fixed backing.
- Fixed-capacity pages expose explicit valid lengths and never grow implicitly. Reuse clears stale
  valid-length state and prevents prior content from becoming part of a later message.
- `PagePool` owns a fixed number of equal-capacity pages and returns one transferable page handle at
  a time. When no page is available, it returns a typed unavailable result immediately; the owning
  service decides whether to retry, apply backpressure, or cancel its operation.
- These primitives do not expose `ResourceRuntime`, `ResourceCapability`, `Reservation`,
  `CompoundReservation`, resource currencies, or process-wide owner accounting.

## Fixed Channels And Backpressure

- `FixedChannel<T>` has a finite message capacity and never grows. Full and closed results preserve
  the caller's exact message so the owning service can retry, cancel, coalesce, or fail explicitly.
- Immediate and deadline-bounded operations let the owning service interleave its own cancellation
  checks. Dropping an endpoint wakes the opposite endpoint, and each service selects a finite wait
  interval appropriate to its cancellation-responsiveness contract.
- Timeout, full, empty, and closed results remain structural channel outcomes. The owning service
  maps its own cancellation token to its semantic cancellation result. Timed sends distinguish
  timeout from an immediate full result and preserve the caller's exact message in either case.
- Channels bound message count, not arbitrary heap content hidden inside `T`; consumers must use a
  bounded payload type or page/range handoff at large-data boundaries.

## Bounded Range Source And Sink Contract

- A source page carries immutable stream identity, exact logical offset, checked next offset,
  explicit valid byte length, and terminal state. A nonterminal empty page is rejected because it
  cannot make progress.
- `BoundedSource` fills a caller-supplied fixed page and `BoundedSink` consumes pages without
  concatenating the logical stream. `StreamCursor` rejects identity drift, overlap, gaps,
  post-terminal pages, and logical offset overflow independently of domain-specific parsing.
- Range sources and sinks support finite absolute-range requests so callers can page durable,
  filesystem, parser, and transport data without materializing the complete value.
- `ReplayableSource` is a separate contract carrying exact stream identity and source revision.
  Implementing the ordinary source trait alone never claims that a live observation can be replayed.
- Source and sink implementations may retain and check an owning service's cancellation token and
  return that service's typed error. This package does not define a common cancellation token or
  reinterpret a partial transfer as successful completion.

## Telemetry And Failure

- Content-free page-pool and channel observers expose configured capacities, current and high-water
  occupancy, lease, send, receive, wait, timeout, full, and exhaustion counts, and endpoint state.
- Telemetry does not retain pages or messages and is not a process-memory total, policy authority,
  or durable record.
- Observers do not count source or sink bytes or service cancellation. The service that owns those
  operations owns their progress and cancellation diagnostics.
- Capacity, channel, page, range, and overflow failures are typed. Consumer-defined failures,
  including cancellation, remain consumer-owned. No package error contains user content.
- Internal synchronization executes no caller callback while locked.

## Dependency Boundary

- This package depends only on the Rust standard library and the workspace error-derivation crate.
- It does not depend on `beryl-model`, storage packages, `beryl-backend`, `beryl-app`, GPUI, Tokio,
  Fjall, or app-server protocol types.
