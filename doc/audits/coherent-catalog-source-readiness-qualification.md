# Coherent Catalog Source Readiness Qualification

## Boundary

This boundary establishes a graph-owned background coordinator that publishes an exhaustively
authenticated compact Catalog source from one retained Home snapshot. Frozen query admission and
visible Thread Switcher mounting remain separate boundaries. The owning contracts are the
[Home storage system](../systems/beryl-home-storage/design.md),
[Home domain API](../../crates/beryl-home-store/doc/design-domain-api.md#generation-owned-frozen-reads),
[Syndic compact history storage](../../crates/syndic-storage/doc/design-history-storage.md),
[State Catalog](../../crates/beryl-state/doc/design-jobs-catalog.md) and
[app catalog composition](../../crates/beryl-app/doc/design-catalog-and-composer.md#published-frozen-catalog-reader).

Qualification is in progress; this record does not establish completion acceptance.

## Implementation

Home owns a finite retained-read registry inside each database generation. Opaque capabilities
contain identities and revisions; independent admission shares the exact internal snapshot while
ordinary cloning adds no retention. Typed points, revisions and bounded pages use ordinary owner,
codec, envelope and health checks. Release refuses new requests and drains admitted requests through
confirmation; retirement closes all slots before database disposal. Closing slots remain charged
until their snapshot and request ownership is gone.

Syndic authenticates compact Thread, Execution, Attributes, HistorySummary and CatalogSummary
records against one frozen capture. Exact earned title facts require no history traversal. Missing
or outdated derived summaries produce separate background preparation work, with exact old-summary
or proven-absence guards. State exposes frozen primary/recency coverage and full paired claims and
runtime/root sources. Structural identity, copy and orphan disagreement is a typed failure.

The coordinator retains one attempt, one coalesced refresh and one published source. It scans
canonical threads and both Catalog indexes in bounded pages, keeps only counters and one repair
identity, and publishes only complete source agreement. Repair captures release before ordinary
source-owned preparation; their audited revision remains the command fence. A fresh capture
certifies settled repair. Continuous invalidation may remain unready and grants no progress claim.
Consumers independently retain the exact certified snapshot under the publication lock, so
coordinator supersession releases only its own slot.

Separate State rebuild preserves full old paired-row/absence guards, immutable runtime/root
identity, advancing Catalog revision and atomic copy replacement. Recreated summary or Session
claim local revisions may restart; runtime/root regressions and ordinary publication's monotonic
guards remain unchanged. App composition joins full source validators in the same Home command.

One fixed observer fanout retains the existing scheduler wake and adds the coordinator wake without
another Home registration. Recovery retains the existing publication gate; actual initial
startup needs a separate Catalog fence through whole native restore-set publication.
Shutdown and failed-home retirement join coordinator work before Home disposal. Original repair
noncommit, committed receipt and installed indeterminate handle remain distinct. Fresh-candidate
retained-process settlement reconciles the original handle and refuses collision/failure without
reconstructing receipt authority; recovery publication requires settled custody.

## Evidence So Far

- Development focused Home lifecycle tests passed 9/9
  (`65baf075-3f15-4fdc-b94d-cda3f326d79c`, 0.622 seconds).
- Development frozen State/Syndic authentication and absent-summary tests passed 22/22 across
  three binaries, 15.737 seconds. Development coordinator, guarded State rebuild and existing
  projection tests passed 15/15 (`c6328251-01dd-475a-a39a-fa5ddd88caed`, 12.775 seconds).
- Canonical complete Home/State regression passed 572/572 across the full requested targets:
  316 Home and 256 State cases, `b50b101c-5483-44c1-bdd1-6ce49dd81d46`, 180.049 seconds.
  Independent review subsequently found the closing-slot admission accounting gap documented in
  [the retained-read failure lesson](../failures/frozen-read-retention.md). The registry and its
  lifecycle test were corrected; all other domain inputs remained unchanged.
- The complete corrected canonical frozen-read binary passed 10/10
  (`eef7302f-abf3-42f3-bca7-647b0b916bdf`, 0.668 seconds), including the actual held-request
  saturation control. Broad domain evidence plus this correction is reused; it is not one fresh
  complete run against the correction.
- Canonical app/State/Syndic all-target checks passed in 2 minutes 23 seconds before that narrow
  registry accounting correction. Canonical app runtime and remaining source qualification are
  pending, with exact input correspondence retained in bounded task evidence.
- Canonical Syndic source, summary absence/pair, pristine eligibility/outcome, discovery and
  thread-property regressions passed 52/52 across eight binaries
  (`ed05197d-f1ac-4f4d-9cc2-8b3571ef99b5`, 97.507 seconds; build 27.79 seconds).
- The final corrected canonical Home all-target check passed in 16.92 seconds.
- The canonical broad app diagnostic (`43405051-3fb5-488e-b75d-7002367f1bc4`) was interrupted
  after 198 completed cases: 169 passed, five failed and 24 native cases aborted at watchdogs.
  Its 520-case selection did not complete and is not accepted broad regression evidence. Older
  fixture corrections are pending reruns. A focused startup diagnostic then proved competing
  Catalog maintenance causes `restore draft or coherent source revision changed` before Running
  handoff. The [startup maintenance lesson](../failures/catalog-startup-maintenance.md) records
  the required narrow fence correction; actual native qualification remains pending.
- The corrected fence focused packet passed nine lifecycle cases, three mutation-fanout cases,
  and selected native final-close. The ordinary native recovery case required termination and
  two new coordinator fixtures needed correction; this 24-case packet is not accepted as a
  complete regression run. A subsequent diagnostic passed all ten corrected coordinator cases,
  including legitimate restore/activation claim revision advancement and fresh-candidate proof
  after a known failed-Home noncommit. Ordinary native qualification also exposed a worker stack
  overflow during dirty editing, reproduced with all tracked source restored to accepted baseline
  `1c7b8b5a`. First-chance CDB identifies staging preparation and its typed participant wrapper
  retaining approximately 465 KiB above nested decoding. The private staging and failed-resident
  custody prerequisite subsequently passed 96 distinct canonical cases, current checks and review;
  see [qualification](native-editing-recovery-stack-qualification.md). This source-readiness phase
  resumes on that accepted correction. Earlier recovery-stage
  breadcrumbs did not establish that the coordinator caused the overflow; see the
  [native staging failure evidence](../failures/ordinary-close-recovery.md#native-staging-preparation-regression).

## Review And Remaining Gates

Independent owning-contract authoring review cleared exact-snapshot independent retention,
guarded local-scalar restart and original failed-home repair settlement. Completion integrity review
is active. Mounted producer witness coverage reuses the accepted
[source-authenticated acquisition investigation](source-authenticated-thread-acquisition-qualification.md#source-readiness);
an unchanged Catalog Current flag alone is never readiness evidence. Scope-wide and title setters
that remain unmounted do not gain acceptance from this boundary.

Final runtime correction evidence, scoped formatting, canonical source correspondence, complete
independent review and accepted tracker/plan updates remain required before completion.
