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

## Bulk Invalidation And Eligibility Authority

The next source-backed review established that rewriting every dependent Catalog pair during
Session restoration cannot fit the existing command envelope: restoration handles up to 512 claim
records, while each admitted Catalog copy can be 256 KiB and the encoded-value budget is 64 MiB.
Adding physical invalidation indiscriminately would also reject valid source writes when a derived
row is absent. Exact committed source revision advancement or deletion already persists projection
disagreement, provided consumers authenticate canonical sources and complete coverage before use.
The owning storage and package contracts now distinguish that invalidation from physical row
markers and forbid treating Current flags as sufficient source agreement.

Both New Thread paths filtered cached claim/scope facts before live source checks. A claim removed
by restoration or ordinary close could therefore leave a cached claimed row that suppresses valid
reuse. Election now targets canonical source discovery and live claim authentication before
filtering; missing or stale projections must not define the eligible population. Additional-window
reuse also used strict pristine cleanup authority, which incorrectly excludes a never-submitted
thread after typing and removal. Reuse requires eligible-empty authority; strict pristine remains
the exact created-fallback deletion boundary. Coherent background catalog publication and frozen
queries retain separate acceptance gates.

Missing-row reuse exposed a further origin error: State classified an initial Catalog revision as
fallback creation, although rebuilding a first projection for an existing thread also starts at
that revision. That rejected genuine reused acquisition during natural replay, native adoption and
abandonment. State now authenticates only its own facts and leaves source origin unqualified; the
composing owner supplies positively authenticated origin before abandonment classification. An
additional-window elected reuse source must also be distinct from its reserved fallback thread and
draft identities, so a request identity collision cannot later turn reuse into fallback deletion.

Runtime qualification found the same invalid initial-revision assumption in Catalog claim release.
A reused canonical thread with a previously missing projection legitimately receives its first
Catalog revision during acquisition. Its exact acquired and native-prepared outcomes were valid,
but release rejected that revision and retained cleanup custody. Release now checks the exact
Current paired row, runtime/root and full active claim with its source revision; row revision does
not supply creation provenance. The separate strict created-fallback deletion gate remains intact.
