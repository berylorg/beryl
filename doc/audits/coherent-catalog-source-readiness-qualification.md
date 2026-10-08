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

Completion review accepted this boundary on 2026-10-08.

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

## Verification

- Canonical Home/State regression passed 572/572: 316 Home and 256 State cases,
  `b50b101c-5483-44c1-bdd1-6ce49dd81d46`, 180.049 seconds. After the closing-slot accounting
  correction, the complete frozen-read binary passed 10/10
  (`eef7302f-abf3-42f3-bca7-647b0b916bdf`, 0.668 seconds). The subsequent direct ordinary-read
  correction again passed all ten frozen-read cases
  (`21753726-bb69-4807-a749-74afbb861e97`, 0.635 seconds). This explicitly reuses broad domain
  evidence plus narrow corrected coverage; it is not one fresh complete run against every correction.
- Canonical Syndic source, summary absence/pair, pristine eligibility/outcome, discovery and thread
  properties passed 52/52 across eight binaries
  (`ed05197d-f1ac-4f4d-9cc2-8b3571ef99b5`, 97.507 seconds). Private publication-custody correction
  passed publication, evidence, checkpoint, history and adoption regressions 74/74 across six
  binaries (`052efb46-126f-4e0d-8f28-9d232344de63`, 180.700 seconds).
- The final affected App packet passed 95/95 across two binaries
  (`5ee6ce11-46a4-49d5-8797-0c48799563ab`, 325.767 seconds). It includes all ten coordinator,
  nine source-lifecycle and three observer-fanout controls, ordinary native close/Exit and
  confirmation, failed-Home recovery, retained resident retries, original session outcomes,
  stop-worker generation refusal, incomplete-cut cleanup and existing Catalog projection controls.
- Both previously intermittent native confirmation cases also passed against the same frozen
  artifact (`dd0aa6d3-0c53-4e09-919a-d52a93c93519`, 2/2, 5.824 seconds).
- Current canonical App/Home/State/Syndic all-target checks passed in 3 minutes 03 seconds, with
  the App test-fault feature, locked offline resolution and one build job. All 81 frozen Rust
  inputs match root/canonical SHA-256 values and pass scoped formatting with child traversal disabled.

The complete earlier App diagnostic `ab6fb7ad-720b-4b69-b33b-dc3ba876b182` ran 555 cases:
517 passed and 38 failed. It remains diagnostic evidence. Independent comparison confirms every
one of those 38 distinct failed cases has a passing entry in the final 95-case packet. Earlier
interrupted and partial packets likewise do not become passing broad regressions. Their decisive
lessons are retained in [startup maintenance](../failures/catalog-startup-maintenance.md),
[frozen-read retention](../failures/frozen-read-retention.md) and
[ordinary close integration](../failures/ordinary-close-recovery.md#catalog-source-integration-regression-findings).

## Completion Review And Limits

Independent review cleared retained-read admission/charging/drainage, complete source and paired
index authentication, guarded local-scalar restart, exact-snapshot independent retention,
prepublication startup fencing, original failed-Home repair settlement, observer fanout and the
bounded private native frame corrections. The separately accepted baseline native staging and
failed-resident prerequisite is recorded in its [qualification](native-editing-recovery-stack-qualification.md).

The newly seeded unviewed-thread fixture exposed legitimate background Catalog repair competing
with close work observation. Existing visible notice evidence proved `a home mutation is in progress`.
Busy remains terminal actual Exit delivery under the owning contract. Independently reviewed setup
now verifies the exact seeded row on a certified source at the current quiescent Home revision,
releases every admitted read before handling results, and waits only for source NotReady or typed
coherence Busy. Catalog remains active. The original dialog deadline, coalescing, cancellation,
waiting-work assertions and fresh activation behavior remain. Temporary callback instrumentation
was removed to exact baseline; useful bounded timeout diagnostics remain.

Mounted producer witness coverage reuses the accepted
[source-authenticated acquisition investigation](source-authenticated-thread-acquisition-qualification.md#source-readiness).
An unchanged Current flag alone never certifies readiness. Unmounted scope-wide/title setters,
State query collections, App query/page capability, ordinary selection recovery and visible Thread
Switcher mounting retain their separate gates. Isolated older observation/resident-page fixtures
that explicitly stop Catalog qualify those narrower contracts; active coordinator integration is
established by the final source-lifecycle and native families.

Verification used stable LLVM, disabled normal debug/incremental compilation and process-local
32-MiB Rust test-thread stacks. Production Windows worker stacks, dependency pins, manifests and
lockfile are unchanged. No task-owned native child remains. Failed diagnostic fixture homes with
exact logged ownership were reclaimed through the required cleanup tool; ambiguous OS temporary
state and the shared target cache were left alone. Bounded canonical logs, input ledgers and
first-chance traces remain in `.tmp/coherent-catalog-source-evidence` as qualification evidence.
