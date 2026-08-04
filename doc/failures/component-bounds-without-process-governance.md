# Exact Process-Wide Memory Governance

## Scope

The Beryl-owned path from CAS transport through Syndic storage, application projections,
transcript layout, media decoding, and GPUI presentation.

## Invalidated Approach

Treat every content-dependent allocation as requiring exact pre-allocation admission through one
process-wide resource runtime, including dependency-private buffers, ordinary paths and objects,
structural allocation slots, and every returned value.

## Evidence

- Earlier work found real unbounded risks: whole provider events, clone-heavy transcript snapshots,
  unenforced queues, unrestricted layout records, media buffers, complete catalogs, and caches that
  could multiply across windows.
- The exact-accounting correction went beyond those risks. It required plumbing capabilities and
  non-cloneable leases through nearly every package and treating normal dependency bookkeeping as a
  public accounting contract.
- Fjall Phase 41 exposed the limiting case: an honest total required formulas for `lsm-tree`
  topology, pinned blocks, cursor readers, temporary workspaces, allocator-dependent structures,
  and retained path capacity.
- `PathBuf` and general allocator capacity cannot be represented as exact portable requested bytes
  without redesigning otherwise ordinary dependency internals.
- CAS 0.144.1 still materializes some request structures inside its own process, so exact
  Beryl-side accounting could not prove an end-to-end process total even after the additional
  complexity.
- The Operator explicitly selected reasonable best-effort controls for the data paths most likely
  to consume substantial memory rather than space-station-grade accounting for every bit.

## Why It Failed

The approach conflated two different requirements:

- practical enforcement at bulk-data, expansion, queue, cache, concurrency, and renderer
  boundaries; and
- exact proof of every allocation made by Beryl and its dependencies.

The first materially improves product reliability. The second imposed large API and implementation
cost, unstable dependency coupling, and misleading precision without eliminating external-process
or allocator uncertainty.

## Course Correction

- Keep hard limits and stress coverage for CAS ingress, Beryl-to-CAS projection, Syndic and
  Beryl-home reads, transcript working sets, media decode/upload, renderer layout, queues, caches,
  workers, and window multiplication.
- Use paging, streaming, chunking, virtualization, backpressure, eviction, and explicit operation
  limits where they address a concrete risk.
- Use subsystem-local item, byte, page, cache, and concurrency limits. Do not require one universal
  governor, structural-slot currency, exact dependency residency quote, or charge attached to every
  object.
- Preserve exact canonical content or fail explicitly; only declared diagnostic or optional
  projections may truncate or degrade.
- Treat process and renderer memory counters as observational evidence rather than an exact
  reconciliation ledger.

## Affected Authority

- `doc/design.md`
- `doc/systems/bounded-resource-dataflow/design.md`
- `doc/systems/beryl-home-storage/design.md`
- CAS-live, Syndic-history, transcript-presentation, image-asset, feature, and package boundaries
  that previously consumed the universal resource runtime
- `doc/rework/beryl-home/REWORK.md`
- `doc/plan.md`

## Remaining Risks

- Relaxing exact accounting must not reintroduce the concrete whole-provider-event,
  whole-transcript, unbounded-channel, unlimited-cache, decode-bomb, or renderer-retention paths
  that motivated the earlier work.
- Generous operational defaults need realistic stress evidence and may require later tuning.
- A dependency allocation should receive targeted architectural work if measurements show it is a
  material product problem; it does not need a speculative public formula beforehand.
