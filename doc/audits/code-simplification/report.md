# Completed Simplification Audit

Completed on 2026-09-07 against `e6172f7c49bebef78d51831e621e70c8ac0d6a07`.
This is an observational proposal for the Operator. It does not amend design authority or
`doc/plan.md`, resume paused implementation, or approve a dependency change.

Later accepted corrections are recorded in [implementation outcomes](implementation.md), separately
from the frozen baseline findings, dispositions and estimates below.

The complete review supports **20,925–30,474 estimated net lines of reduction under current
contracts**. Of that, **1,660–2,460 lines depend on a response-parser prototype**. Excluding that
proposal gives **19,265–28,014 lines**. These are estimates after replacement, caller adaptation
and additional verification allowances, not measured implementation results. They include test
construction and support. They do not represent production-only savings or a performance forecast.

The best-supported cuts remove repeated fixture construction, duplicate process-local payloads,
redundant deterministic derivation and avoidable continuation plumbing. Several larger-looking
layers protect distinct durability, boundedness or ownership requirements. The examined external
crates do not establish a compatible wholesale replacement for those layers. Substantially larger
cuts require an explicit simpler product or persistence contract and a priced replacement design.

# Coverage And Evidence

All **1,910 included files, containing 578,865 physical lines**, were reviewed before subsystem
synthesis: 1,847 full-body reviews and 63 declaration-only reviews. The 341 explicit exclusions
and included ledger exactly cover all 2,251 tracked Rust/PowerShell candidates. No generated live
source was found. Archived replacement code and historical probes are excluded by recorded reason.
External and owned dependency forks were inspected at relevant consumed boundaries; their complete
upstream source is outside this first-party coverage claim.

- Application: 685 files, 206,936 lines; [subsystem synthesis](reviews/subsystem-app.json).
- Syndic: 782 files, 250,804 lines; [subsystem synthesis](reviews/subsystem-syndic.json).
- Foundations and entry: 443 files, 121,125 lines; [subsystem synthesis](reviews/subsystem-foundations.json).
  This includes model 16 files, stream 6, home-store 99, state 125, backend 196 and entry 1.

The [coverage ledger](coverage.tsv) records each baseline Git blob, size, review depth, finding IDs
and evidence. [Inventory](inventory.json) records scope and classifications; [exclusions](excluded.tsv)
record excluded paths. Physical classification is 353,487 source lines and 225,378 test/support
lines. Source-local tests make that classification unsuitable for a production/test savings split.

All 24 file-review ledgers were reconciled against the final ledger with zero changes required.
Each included baseline blob matched, and live product source remained unchanged. Exact-set checks
found no missing, duplicate, nonaudited, invalid-depth or empty-evidence rows. Subsystem synthesis
then reconsidered ownership chains, cross-crate callers, overlap, alternatives and replacement cost.
Root review checked the decision records and targeted consequential interconnections, including the
new ordinary-edit caller reuse and the distinct no-op versus active-marker removal phases.

The [canonical estimates](estimates.tsv) contain 250 unique decisions, including every preliminary
lead resolved once. There are 179 current-contract entries: 178 have positive estimates and one is
a zero-line cleanup. Every one of the 246 subsystem estimate rows matches its structured decision's
bucket and amounts. The duplicated cross-subsystem PRE-004 lead is merged, and missing preliminary
cross-references are added at zero. Historical cohort estimates remain evidence; they are not added
to the canonical totals. Here `proposed` means reviewed as a proposal, not Operator approval.

# Net Estimates And Accounting

- Application: **7,345–10,286** net lines.
- Foundations: **5,718–8,805**, including the prototype-dependent BJ-003 response cursor.
  Without BJ-003: **4,058–6,345**.
- Syndic: **7,862–11,383**.
- Combined: **20,925–30,474**, approximately 3.6–5.3% of the reviewed physical lines.

These ranges describe one compatible set of current-contract proposals. Helpers, adapters,
preserved exceptions and new verification reduce gross deletion; corrective work earns zero
savings. They are neither statistical confidence intervals nor a commitment that every helper
will remain worthwhile after implementation. Reject a helper if its final implementation is larger
or obscures the contract. No global source/test allocation or unmeasured subsystem deletion is
invented to inflate the result.

Important overlap decisions are already applied:

- SP-003 combines provider compiler and typed mutation savings once: 180–255. Actual durable
  staging and exact completion verification remain. Projection findings do not count it again.
- DH-008 pays for the common ordinary-edit fixture. DP-007 owns different callers and staging;
  SYN-SYN-001 adds only two omitted piece-tree/range-source callers, net 20–40. DT-006 retains its
  separate marker/publication helpers.
- PRE-004 becomes App's remaining home helper, 12–18, and Syndic's incremental helper, 260–330.
  Foundations already use tempfile. Syndic excludes DT-006's label-protection home and pays for
  feature isolation, path adapters, cleanup retry and lifetime corrections. No global helper count
  is added on top.
- Foundation BR-004 and MS-003 include their App caller changes. ST-005's Syndic command spelling
  adaptations add no Syndic savings. The receiving subsystem does not recount those changes.
- BJ-003 excludes BJ-004's unsupported-wire-shape correction, deducting 140 from both endpoints
  of the earlier parser estimate. HS-004 earns zero because error provenance needs correction.
  ST-004 deducts the area associated with false concurrency/digest evidence.
- AX-015 is revised to 1,640–1,725 after retaining distinct retirement preconditions and paying
  for nine target mounts. AI-013's 415 unmounted fixture lines are a separate deletion. AS-009
  pays for missing behavioral replacement coverage; AX-018 adds evidence, not another estimate.
- Schema, Copy Detail and whole-document TOML alternatives below are excluded from this total.
  Existing required rework, including tree/service ownership, earns no new reduction credit.

# Whole-Codebase Conclusions

## Remove Representations That Only Feed Other Representations

Several subsystems maintain a payload, a parallel field-copy structure, a provisional value and
then a reconstructed final value. Syndic records, build progress, settlement effects and App
projection outcomes offer concrete instances. Prefer one private payload with explicit validated
metadata or a borrowed already-owned effect. This removes conversion and reconstruction layers
as a consequence of choosing the right owner. It does not merge independently durable heads,
receipts, source/target admission or separately timed validation.

The same approach applies to state theme execution: one complete pending outcome can replace
temporary optional fields and repeated semantic execution setup. Home-store's physical staging
helper has a different responsibility and remains below that state-level owner. A cross-package
transaction framework would absorb meaningful distinctions and is not justified by this evidence.

## Simplify Control Flow At Its Actual Execution Boundary

Compact backend responses already finish inside a synchronous driver. BJ-003 proposes a bounded
pull cursor over the existing incremental recognizer, removing stored response continuation states,
forwarding and drain plumbing that exist to support those states. The price is 1,660–2,460 net
lines, with a prototype required before committing to the rewrite. It must prove classified-prefix
handoff, produced bytes on failing input, fragmented UTF-8, bounded scratch, final-input draining
and terminal poison. Provider events, direct page sinks and their backpressure remain independent.

Smaller current-contract cuts share compact control grammar, page mechanics, response waiting,
pure successor derivation and repeated App projection handling. They preserve request expectations,
method-specific echo timing, first-byte dispatch, exact rejected payloads and shutdown order.
For example, AI-009 may share a ten-field execution payload; the worker and flight must remain
outside it so destruction still proceeds session/tools, then worker, then flight.

Durable-job transitions are another closed family: one action enum can replace seven repetitive
Rust command implementations while retaining the transition matrix, revisions and live index.
This changes a source API; no authority requires the seven old struct names. Ordinary source API
adaptation is not automatically a product-contract decision.

## Keep Distinct Authorities Even When Their Shapes Resemble Each Other

Syndic's sequence tree answers positional questions, its identity index proves marker occurrence,
and its marker-order tree commits order/count/labels. They have different selectors, authenticated
aggregates and bounded continuation duties. Share exact packing or pure calculations where shown;
an ordinary resident collection does not erase those obligations.

Staging, build progress, settlement and publication likewise own authored input custody, bounded
path-copy continuation, one logical outcome and selection of a captured candidate/history pair.
History witnesses support direct-root undo and lineage retention. Materialization plans content
identity, writes bounded output and exposes it only through atomic sealing. These are substantive
responsibilities under the current editor contract.

Across the workspace, storage acquisition, writer admission, point-read closure checks and physical
scrub establish different facts at different times. Share pure formulas and narrowly bounded
transport while preserving acquisition order, meters, mutable-anchor observations and domain errors.
DT-003 specifically cannot simply delegate between root finalizers: normalization/read-error order
differs. Extract the post-normalization summary or preserve each caller's prelude.

Home-store must still distinguish NotCommitted, Committed and Indeterminate, perform all-domain
preflight, preserve prepare/contribute order and publish manifests last. App mount/service/widget
boundaries retain their own locks, admission, GUI lifetime and custody. One generic success/failure
or common owner would discard guarantees that local line-count comparisons miss.

## Delete Dead Construction Before Sharing Live Fixtures

This is the clearest large starting point. App AX-015/AI-013 offer 2,055–2,140 net lines from
duplicate fixture trees and unmounted helper modules, preserving distinct live setup preconditions.
Syndic SC-009 offers 780–1,020 by deleting provider-record construction that both consumers always
filter out. The real provider seed, active route facts and temporary store supplying retained empty
draft/history references remain. Removing this dead producer chain also removes its feeder helpers.

Then share behavior-specific fixture construction. App's remaining support package is 2,287–3,583;
Syndic's complete fixture package is 3,746–5,128, already including SC-009 and its incremental home
and ordinary-edit helpers. State ST-004 contributes 570–970. These are subsets of the totals,
not additional savings. Preserve scenario entry points, independent expected bytes/digests, exact
IDs/revisions, intermediate ownership assertions and fault insertion timing. A universal fixture
object or production-derived corruption oracle would reduce evidence quality.

# Dependency Replacement Findings

The synthesis consolidates 22 versioned assessment entries. Exact versions, features, licenses,
declared MSRV/platform limits, maintenance/adoption evidence, wrapper cost and advisory limitations
are in the linked investigations and the
[foundation dependency matrix](reviews/subsystem-foundations.json). These are concrete candidate
screens, not a claim to have exhausted crates.io or cleared every dependency graph of advisories.
No dependency was installed or changed, and no candidate was build-validated during this audit.

Existing dependency reuse is the strongest result:

- [thiserror 2.0.18](../../memory/crates.io/thiserror/2.0.18/manual-error-simplification.md): use for
  conventional errors while preserving exact text, From behavior, source topology and redaction.
  Some errors deliberately have no source; derive defaults must not silently alter that.
- [tempfile 3.27.0](../../memory/crates.io/tempfile/3.27.0/audit-test-homes.md): use at the audited
  custom test homes. Exclusive creation is useful; default Drop ignores cleanup errors and does not
  reproduce existing retry policy. Destroy stores/leases before directories and retain explicit
  retry where required. This is not a durable publication primitive.
- [Serde 1.0.228 and serde_json 1.0.149 outbound reuse](../../memory/crates.io/serde_json/1.0.149/streamed-string-serialization.md):
  retain streamed serialization and use fixed wire derives where their field/null policy matches.
  Resolved serde_json is 1.0.149; 1.0.151 below is a separately screened candidate, not the lockfile.
- [TOML 0.9.12+spec-1.1.0](../../memory/crates.io/toml/0.9.12+spec-1.1.0/capped-theme-document.md):
  a plausible theme-document replacement only after the bounded whole-document grammar/residency
  decision. Its savings replace the current-parser helper estimate.

The larger drop-in candidates did not demonstrate the required semantic fit:

- [serde_json 1.0.151](../../memory/crates.io/serde_json/1.0.151/bounded-ingress-replacement.md) and
  [struson 0.7.2](../../memory/crates.io/struson/0.7.2/bounded-ingress-replacement.md): ordinary
  deserialization/stream readers do not replace fragmented, externally driven, bounded ingress and
  direct-page backpressure. Capped ancillary documents would need separate contract choices.
- [fast-float2 0.2.4](../../memory/crates.io/fast-float2/0.2.4/incremental-number-replacement.md):
  contiguous numeric conversion does not remove the incremental grammar, bounds and adapters.
- [tungstenite 0.30.0](../../memory/crates.io/tungstenite/0.30.0/incremental-canonical-ingress.md):
  no compatible replacement for canonical incremental ingress was established. Existing 0.29 test
  helper use remains appropriate within its different boundary.
- [url 2.5.8](../../memory/crates.io/url/2.5.8/incremental-provider-locators.md): resident URL parsing
  does not replace the streamed provider locator contract at a demonstrated net benefit.
- [Ropey 1.6.1](../../memory/crates.io/ropey/1.6.1/durable-piece-tree-screen.md) and
  [im 15.1.0](../../memory/crates.io/im/15.1.0/durable-admission-index-screen.md): resident data
  structures do not supply authenticated external nodes, durable continuation or admission custody.
  They become relevant only under a changed editor/storage envelope; im's archived maintenance
  status is an additional limitation.
- [pulldown-cmark 0.13.4](../../memory/crates.io/pulldown-cmark/0.13.4/durable-projection-parser-screen.md):
  parsing a resident document does not replace resumable durable projection with the required bounds.
- [Postcard 1.1.3](../../memory/crates.io/postcard/1.1.3/syndic-record-value-codecs.md): a candidate
  for a newly authorized record-value format. Ordered keys, hashes, validation, collection caps and
  exact-end checks remain. No V7-compatible or net whole-codec saving is claimed.
- [bincode 3.0.0](../../memory/crates.io/bincode/3.0.0/provider-codec-screen.md): rejected at the
  release/maintenance gate; the examined release intentionally does not build.
- [notify 8.2.0](../../memory/crates.io/notify/8.2.0/bounded-theme-watcher.md): its Windows worker
  lifecycle does not establish Beryl's joined retirement and bounded hint behavior. The retained
  lifecycle/fallback adapter prevents treating scanner deletion as net savings.
- [process-wrap 10.0.0 and command-group 5.0.1](../../memory/crates.io/process-wrap/10.0.0/backend-process-ownership.md):
  the inspected public configuration does not provide the required kernel kill-on-close ownership.
  WSL cleanup and grace waits also remain. A fork and ownership adapter are unjustified for the small
  replaceable host-process wrapper.
- [ICU normalizer/casemap 2.3.0](../../memory/crates.io/icu_normalizer/2.3.0/exact-catalog-casefold.md):
  component normalization/case mapping is not proof of the required Unicode 17 NFKC_CF plus NFC
  sequence, including ignorables and bounded expansion. No priced equivalent replacement is established.
- [Syntect 5.3.0](../../memory/crates.io/syntect/5.3.0/bounded-code-panel-highlighting.md): grammar,
  contiguous-line/state residency and style mapping differ. The existing adapter's gross size is
  not net savings; revised highlighting behavior and an overlong-line policy would be needed.
- [Moka 0.12.16](../../memory/crates.io/moka/0.12.16/bounded-app-cache-screen.md): eventual cache
  capacity does not replace three independent budgets, admission tickets, worker accounting and
  exact owner retirement. Adding the required outer coordinator yields no demonstrated reduction.
- [lru 0.18.3](../../memory/crates.io/lru/0.18.3/app-pruning-screen.md): two small pruning loops do
  not justify a dependency plus policy adapters. The screen records the relevant advisory fixed in
  0.18.2; that version-specific fact is not a security clearance for future adoption.

# Alternatives Requiring Changed Authority

These are separate from current-contract savings. They need concrete choices before a replacement
can be designed and verified; elapsed audit work does not imply Operator acceptance.

- **DT-004, 60–85 net lines:** remove only `DraftPieceBuildFrontierV1::Removing`, whose cursor
  advances through a marker-free range without changing the tree. Planning can proceed to Applying
  under a revised continuation/schema contract. The distinct active-marker Removing phase remains.
  This supersedes preliminary 95–130 and local 40–100 estimates. Runtime benefit depends on removed
  range size and has not been benchmarked.
- **AW-003, 330–430:** replace notification partial selection with Copy Detail. This replaces
  AW-004's 8–12; it does not add to it. Wrapping, scrolling, shaping, continuity and stale/inert
  fences remain. The notification feature and widget contract must choose this behavior.
- **ST-014, 350–550:** allow capped whole-document TOML parsing with explicit grammar, memory and
  concurrent-document budgets. This replaces ST-011's 70–110. Physical theme publication, freshness,
  reference acquisition and manifest streaming remain independently required.
- **HS-003, 150–260:** replace inaccessible sealed collision fact archives with a compact closed
  marker. This changes required diagnostic retention and needs that decision first.
- **PRE-015, unmeasured:** a bounded resident editor and snapshot undo could remove whole durable
  editing responsibilities. First choose maximum logical draft size and oversize behavior, restart
  survival of unsaved edits/history, retention, directed selection, marker identity/order/assets,
  autosave/publication selection and ambiguous-outcome custody. Price the new persistence and UI
  integration before claiming any tree/history/materializer lines. This overlaps local reductions
  in the affected subsystem and any new Postcard record format.

# Correctness And Verification Findings With Zero Savings

These are static findings or evidence gaps, not runtime reproductions. Preserve them as prerequisites
when affected simplifications are selected; corrective additions must not be described as savings.

- AP-007/AM-007 describe the same same-host request high-water reset. AP-007 owns the finding once.
- AX-006 loses continuation on commandless readiness scans. AX-019 fails to dispose of the exact
  local compaction owner after definitive nondispatch; committed/indeterminate custody must remain.
- DP-010's head/head/closure acquisition can classify a valid concurrent advance as corruption.
  Bracket closure acquisition with mutable-anchor observations under the existing read contract.
- DH-011 accepts some partial historical successor fields for noncommit because it compares
  `committed == all_three_present`. Commit requires all three, noncommit none. Normal constructors
  currently emit all-or-none; malformed-record acceptance is the issue.
- HS-004 includes physical read errors classified through a raw-box fallback. Fix provenance
  explicitly. BJ-004 accepts extra reasoning-effort string/map shapes outside the pinned producer
  contract; its removal is corrective rather than discretionary simplification.
- AM-008 and SA-011 identify duplicate test module wiring. AW-005's append fixture is already
  truncated, and its command selector uses the wrong generation counter, weakening its evidence.
- FND-SYN-002 identifies serialized lazy spawn/join masquerading as concurrency and a digest
  comparison whose content/digest inputs are ignored. Keep real independent byte/digest oracles and
  introduce actual contention before relying on those tests.
- APP-SYN-002 and FND-SYN-001 identify aggregate fixtures declaring a directory before its live
  store. Syndic has the same lifetime pattern in its helper consolidation scope. Rust field drop
  order requires deliberate store-before-directory ownership; tempfile alone does not fix it.

Existing paused rework is separate: AP-008's service registry, AX-020's partial worker, SS-010's
86-versus-90-family/input-gate authority gap and EN-001's explicit entry `compile_error!` remain
required work. The baseline is not certified as a complete runnable application. Audit completion
does not substitute for implementation or change the active plan.

# Recommended Implementation Sequence

This is prioritization for future plan derivation, not a second implementation plan. The current
authoritative plan and owning engineering-rigor contracts control actual work.

1. Reconcile affected correctness/test evidence and active rework. Begin with dead fixture trees,
   unused modules and always-discarded producer chains, checking the current include/call graph.
   Retain complete scenario inventory and compare retained fixture records before factoring setup.
2. Apply exact local codec reuse and conventional error derives. Codec deletion determines which
   error surfaces still exist, so do that before pricing derives on the same area. Verify independent
   golden bytes, EOF behavior, exact diagnostics and source topology.
3. Choose private payload ownership, then simplify construction/callers. Preserve durable record
   boundaries, hash preimages, proof correlation, preflight ordering and rejected-payload custody.
   Update cross-crate callers within the one estimate that owns them.
4. Prototype BJ-003 before choosing the response rewrite. If it passes the complete fragmentation,
   extents, final-input and poison corpus, proceed under backend authority. If it fails the required
   mechanics, stop and re-evaluate the proposal; do not quietly weaken the parser contract.
5. Share bounded query/control/projection mechanics and behavior-specific fixture support. Verify
   drift versus stable corruption, read/byte ceilings, first-byte dispatch, sink backpressure,
   cancellation and terminal owner release at each changed boundary. Test default and test-faults
   configurations where applicable, and Windows cleanup with stores actually retired first.
6. Evaluate optional product/schema choices before investing in overlapping local work. A chosen
   editor, TOML or notification replacement needs updated owning authority and a new measured cost.

The audit executed no product builds, test suites, GUI runs or dependency installation. Verification
performed here concerns coverage, static source relationships, versioned dependency evidence,
decision reconciliation and arithmetic. Implementation must run the applicable focused checks and
the required independent/adversarial review for consequential persistence and custody changes.
At audit completion, source, tests, manifests, design authority and the implementation plan were unchanged.
