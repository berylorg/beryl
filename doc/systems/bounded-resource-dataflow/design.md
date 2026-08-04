# Goals

Prevent externally supplied, durable, decoded, rendered, or queued data from causing runaway
memory, GPU, or background-work growth in the parts of Beryl that handle substantial payloads.

Keep the CAS-to-Syndic-to-Beryl-to-renderer path responsive through practical paging, streaming,
virtualization, cache budgets, queue limits, and concurrency limits.

Allow exact large logical content to remain durable and usable without requiring exact accounting
of every allocation made by Beryl or its dependencies.

## Non-goals

- Proving that total process memory is independent of every logical input size.
- Accounting for allocator metadata, ordinary structs, paths, handles, dependency bookkeeping, or
  every temporary allocation.
- Requiring one process-wide resource governor, typed allocation capabilities, structural-slot
  currencies, or a lease attached to every returned value.
- Requiring every allocation to be predicted or rejected before it occurs.
- Hiding an unavoidable whole-value allocation inside CAS, GPUI, the operating system, or another
  dependency.
- Treating physical transport, storage, parser, or renderer chunks as semantic product records.

# Decisions

## Risk-Based Scope

- Hard resource rules apply at boundaries with a moderate or high chance of large amplification or
  accumulation:
  - CAS ingress and normalization before Syndic capture;
  - Beryl-to-CAS request and recovery-context projection;
  - Syndic and Beryl-home durable reads into application memory;
  - transcript and editor working sets passed toward GPUI;
  - image, media, decompression, parsing, shaping, and GPU upload;
  - queues, caches, concurrent workers, windows, and retained pages whose count can grow.
- Small control records, ordinary object graphs, path storage, fixed metadata, allocator overhead,
  and dependency-private bookkeeping do not require exact byte accounting.
- A boundary may use a hard byte limit, item limit, page limit, concurrency limit, cache budget, or
  a combination. It does not need a universal resource currency when a local bound controls the
  actual risk.
- Byte estimates may use logical payload length or owned buffer capacity when that value is cheap
  and useful. They are operational estimates, not claims about allocator or operating-system RSS.

## Enforcement Model

- When a trustworthy size is available before a large whole-value allocation, Beryl checks the
  applicable product or subsystem limit first.
- When size is not known up front, a risk-bearing boundary uses incremental parsing, bounded reads,
  paging, or streaming when the dependency provides a practical seam.
- When an external API necessarily materializes a whole value, Beryl places a documented generous
  limit on the input or operation where feasible and treats the dependency allocation as an
  accepted limitation. It does not reconstruct dependency-private allocation formulas.
- Queues and worker sets have explicit count capacities. Full queues apply backpressure, coalesce
  replaceable work, evict optional work, or return a typed unavailable result according to the
  owning semantic contract.
- Caches have coarse byte or item budgets and observable eviction. Several subsystem budgets may
  coexist; no cache or package becomes universal process-memory authority.
- Canonical conversation or user content is never silently truncated to satisfy a resource limit.
  A boundary either preserves exact bytes through bounded pieces or reports an explicit failure or
  unavailable state. Diagnostic excerpts and other declared non-authoritative projections may be
  truncated with an explicit truncation fact.

## Shared Streaming Primitives

- `beryl-stream` owns reusable fixed-capacity byte pages, bounded channels, range-source and
  range-sink contracts, and compact identity and offset progress helpers.
- These primitives prevent accidental unbounded buffers at known choke points. They do not own a
  universal process resource runtime or require typed admission capabilities for unrelated work.
- The backend, app, storage, or presentation service that owns an operation also owns its semantic
  cancellation token, cancellation checks, maximum wait interval, typed cancellation outcome, and
  cancellation diagnostics. Shared channel operations provide immediate or deadline-bounded
  results and endpoint-close wakeups so that service-owned cancellation can remain responsive.
- A timeout, unavailable page, or closed endpoint is not silently reclassified as cancellation.
  Source and sink implementations may retain their owning service's cancellation token and return
  that service's error without adding a package-wide cancellation protocol.
- Page sizes are selected per boundary. A storage page, JSON fragment, WebSocket frame, text
  layout chunk, and image tile need not share one size.
- A physical page may be transferred or shared where useful, but ordinary bounded results may also
  be owned values. Non-cloneable resource leases are not a system-wide requirement.

## CAS To Syndic

- Provider-capable foreground connections parse CAS JSON incrementally for size-unbounded text,
  tool arguments, and other payloads that Beryl must retain. Compact controls use generous
  representable-field limits.
- The normalized ingress queue is count-bounded. Large canonical text moves to Syndic staging in
  bounded ordered fragments instead of accumulating as one backend or app-owned string.
- Syndic publishes canonical events and projection records only after their exact ordering and
  provenance are established. Physical fragment boundaries do not change event identity.
- Unsupported or malformed controls, oversized required compact fields, capture failure, and
  connection loss retain their existing typed lifecycle meaning. Resource pressure does not
  manufacture successful capture or a terminal provider fact.
- Unknown or unused provider data may be structurally discarded. Required canonical content is
  preserved exactly or the affected capture becomes explicitly incomplete or failed.

## Beryl To CAS

- Submitted drafts, recovery history, steering input, and tool responses use range-backed or
  incrementally encoded sources where the targeted CAS contract permits it.
- Outbound queues remain count-bounded and do not retain duplicate whole request payloads.
- A CAS release that internally materializes a request remains an upstream limitation. Beryl
  limits the logical operation when necessary and avoids adding its own proportional copies, but
  it does not claim to account for CAS process memory.
- Dispatch identity, possible-dispatch outcomes, cancellation, retry eligibility, and lineage
  semantics remain exact regardless of how request bytes are paged.

## Syndic And Home Storage To Beryl

- Large durable text and resources remain chunked or sidecar-backed. Product-level logical content
  is not limited to one Fjall value.
- Store point reads and cursor pages require practical stored-byte, decoded-byte, item, and range
  limits. Callers request one bounded page and decide whether to continue.
- Callers and caches may retain only a configured number or coarse byte budget of result pages.
  They do not accumulate a complete durable history, catalog, or resource merely because each page
  is individually bounded.
- Exhaustive validation and recovery scan in bounded pages with compact invariant state. Algorithms
  that need cross-record state use ordered traversal, durable indexes, or staged durable proof
  rather than a whole-domain in-memory set.
- The owned Fjall stack provides:
  - encoded and decoded block and separated-value read ceilings;
  - merge-source and retained-topology sanity ceilings;
  - configured cache and memtable budgets;
  - metadata-first point and cursor reads;
  - bounded record and encoded-byte batch limits;
  - stable lifecycle, error, health, durability, and recovery behavior.
- Fjall and `lsm-tree` do not expose exact whole-database residency totals, structural-slot totals,
  path-capacity formulas, or per-operation allocation quotes for Beryl. Beryl does not reserve
  dependency-private memory before configuration or storage access.

## Syndic To Transcript Presentation

- Transcript activation and scrolling request revision-bound pages around the selected viewport.
  The host retains a configured visible working set plus modest overscan and evicts obsolete pages.
- Live text deltas use bounded queues and bounded transient coalescing. Durable reconciliation
  prevents the live suffix from becoming a second permanent copy of the response.
- Activity, catalog, lineage, model, and history collections remain paged when their logical count
  can grow substantially. Virtualization bounds rendered rows; page/cache policy bounds the
  application working set.
- Selection, menus, focus, and navigation may pin only a small configured number of records or
  resource ranges. If the relevant data is evicted, the owning feature follows its documented
  unavailable or close behavior.

## Beryl To Renderer

- GPUI receives only the realized transcript frame, bounded overscan, and explicitly retained
  nested-widget data. Scrolling does not accumulate one element, measurement, or shaped line per
  visited record.
- Large text records are divided into stable presentation ranges before Markdown parsing, syntax
  work, line layout, or `SharedString` construction would create an unreasonable whole-record
  working set.
- Markdown, code, tables, logs, and accessibility projections have practical per-record or
  per-window work limits. Optional decoration may degrade explicitly while canonical durable text
  remains available.
- Render and prepaint paths consume prepared presentation data. They do not perform blocking
  storage, backend, filesystem, image decode, or full-history work.

## Media, Files, And Clipboard

- Image and media headers are inspected before expensive decode when possible. Dimension, pixel,
  frame, decoded-byte, upload, and texture-cache limits prevent decompression bombs and accidental
  full-resolution presentation.
- Original media remains file-, sidecar-, or storage-backed. Transcript and preview surfaces use
  bounded thumbnails, tiles, or an explicit unavailable state rather than decoding an arbitrary
  original merely to downscale it.
- CPU media caches and GPU texture caches use coarse process-level budgets with eviction and
  high-water diagnostics. Shared immutable resources may be reused across windows.
- File hashing, save, export, and upload stream when practical. Clipboard operations that require
  one contiguous platform value use an explicit product limit; larger content uses a streaming
  alternative when the feature provides one or reports an explicit limit.

## Editors And User-Created Collections

- Drafts may be durably chunked and presented through a range-backed editor so very large content
  need not be duplicated across storage, app state, undo, and rendering.
- Undo, history, autosave staging, image markers, and navigation histories use practical count or
  byte limits. Limits protect the working set without redefining the durable logical content.
- Window, popup, preview, background-session, and worker counts have explicit generous caps where
  unbounded creation could multiply substantial state. Ordinary small UI objects do not require a
  memory reservation protocol.

## Failure And Recovery

- Resource-limit, malformed-data, transport, storage, stale-revision, cancellation, and
  unknown-dispatch outcomes remain distinct where they affect correctness.
- A denied speculative load leaves durable authority unchanged. Optional preload or decoration may
  be dropped under pressure.
- If a correctness-sensitive operation exceeds a known limit before dispatch or commit, it fails
  explicitly. After possible external dispatch, resource failure preserves the existing
  unknown-outcome rules.
- Restart reconstructs bounded current working sets from durable identities. Cache contents and
  process-memory estimates are never durable authority.

## Diagnostics And Verification

- Diagnostics expose configured page, item, byte, queue, cache, concurrency, pixel, and texture
  limits where useful, plus current counts, approximate owned bytes, high-water marks, denials,
  evictions, and cancellations.
- Primitive page-pool and channel diagnostics report their own capacity, occupancy, waits,
  timeouts, full or exhaustion results, traffic, and endpoint state. The service that owns semantic
  cancellation reports cancellation observations and outcomes.
- OS process memory and renderer-resource counters are observational evidence, not exact
  reconciliation against every Beryl allocation.
- Stress tests focus on the major risk paths:
  - large and fragmented CAS provider payloads captured into Syndic;
  - large drafts and recovery contexts sent toward CAS;
  - repeated and deep Syndic history reads into Beryl;
  - long transcript scrolling and live-delta reconciliation;
  - large or adversarial image/media inputs;
  - full queues, cache churn, many windows, and bounded worker concurrency.
- Tests verify that these paths remain within their configured working-set intent, apply
  backpressure or explicit failure, release reusable capacity, and do not grow without bound when
  operations repeat. They do not require exact allocator-byte equality.
- Reviews and source scans target unbounded channels, whole-history collections, obvious
  clone-heavy bulk payloads, unrestricted decode/layout expansion, and caches without eviction at
  the named risk boundaries. They do not reject every `Vec`, `String`, `PathBuf`, iterator, or
  dependency allocation.

## Cross-System Invariants

- Paging or streaming must cover the complete Beryl-owned portion of a named large-data path; a
  bounded final stage does not excuse an obvious whole-value copy immediately upstream.
- Practical bounds are enforced, not merely documented.
- Physical chunking never changes semantic identity, ordering, provenance, or transactional
  publication.
- No subsystem budget is durable authority, and no diagnostic observation authorizes mutation.
- Exact dependency memory accounting is unnecessary unless later evidence identifies a concrete
  dependency allocation as a material product problem.
