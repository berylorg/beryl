# Thread Catalog Currentness

## Stored Current Flags Do Not Establish Query Readiness

On 2026-10-08 source-backed frozen-query readiness invalidated the assumption that existing
CatalogCurrentScan plus retained snapshots would suffice for the exhaustive Thread Switcher.
Ordinary Session removal deletes claims without Catalog invalidation; live event admission
advances canonical HistorySummary without compact Syndic summary or State Catalog effects. A
stored Current row can therefore disagree with its authoritative sources.

Neither a stable Catalog revision nor checking only compact-summary revision fixes this problem.
The named-thread acquisition repair helper is not an exhaustive production maintenance owner.
Catalog-only scans also cannot discover Syndic threads with missing rows. Do not omit those rows,
turn stale data into an empty result or run history-title fallback implicitly while opening a flyout.

The correction is separately accepted source-coupled publication/invalidation, exhaustive compact
source readiness and bounded rebuild custody before frozen reader admission. Existing current
single-domain commands cannot silently acquire extra participants; establish the typed atomic
cross-domain capability and writer-time exact Catalog invalidation first. See
[source-backed readiness](../audits/frozen-thread-catalog-readiness.md).
