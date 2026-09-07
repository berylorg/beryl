# Purpose

Complete the Operator-authorized simplification audit of Beryl's live source and tests before
selecting implementation work. Examine individual implementations, their subsystem relationships,
and the whole codebase. Investigate established crates.io dependencies where they could replace
substantial custom code.

This directory contains observational audit evidence and recommendations. It does not change
product, system, package, GUI, persistence, or implementation-plan authority. A recommendation that
requires a contract change remains a proposal until the owning authority is updated.

# Baseline And Scope

- Baseline commit: `e6172f7c49bebef78d51831e621e70c8ac0d6a07`.
- Audit authorization: Operator conversation on 2026-09-06, following the preliminary size audit.
- Primary scope: every tracked live first-party source and test file in this checkout, including
  source-local tests, test support, build code, and live source outside workspace crates if present.
- Archived replacement snapshots and the two historical Codex CLI 0.144.1 rework probes are
  excluded explicitly in `excluded.tsv`; the live product contract targets Codex CLI 0.146.0.
- Generated source, if present, is identified explicitly; its generation boundary and necessity
  must be assessed rather than silently counting generated lines as removable handwritten code.
- External dependencies and sibling or ignored dependency forks are assessed at their consumed
  boundaries and relevant replacement surfaces. Their complete upstream source is not part of the
  first-party file-coverage claim. Inventory metadata records these boundaries.
- Product source, tests, manifests, and target design remain unchanged during this audit. The
  earlier paused implementation work does not resume as a side effect.

`inventory.json` records the discovered scope and counts. `coverage.tsv` binds each included file
to its baseline Git blob. An initial pending row is an inventory entry, not evidence of review.

# Artifacts

- [Completed whole-codebase report](report.md): conclusions, priorities, net estimates, dependency
  decisions, contract alternatives, verification requirements and limits.
- [Canonical estimates](estimates.tsv): 250 unique decisions after final overlap reconciliation;
  use this file for totals rather than adding historical cohort or preliminary estimates.
- [Findings register](findings.md): shared proposals, prior evidence, and synthesis decisions.
- `coverage.tsv`: one row per included file, with baseline identity, size, kind, review status,
  review depth, finding IDs, and concise evidence.
- `excluded.tsv`: excluded tracked source paths and explicit reasons.
- `inventory.json`: baseline, inclusion rules, counts, and dependency boundaries.
- `reviews/`: bounded review reports with per-file coverage and structured findings. The root
  auditor validates and integrates these into the main ledger and findings register.
- Dependency investigations are retained under `doc/memory/crates.io/<crate>/<version>/` or the
  applicable exploration-memory source path, with links from the relevant finding.

# Coverage Rules

Coverage status is `pending`, `in_progress`, `partial`, `audited`, `blocked`, or `stale`.
Review depth is `none`, `declarations_only`, `partial`, or `full_bodies`.

Mark a source or test file `audited` only after examining all its implementation bodies, relevant
type and macro definitions, conditional code, and the relationships needed to assess its purpose.
For declaration-only files, record that depth and examine every declaration. Symbol inventories,
filename searches, representative sampling, prior module-level reviews, and generated summaries
do not establish complete coverage. A partial review records what remains unread or unresolved.

Each audited row references findings or gives a concise reason why no simplification was identified.
An audited result is evidence about this baseline; it does not certify that the file is irreducible.
Changes to an audited source blob make its result stale until reassessed. Complete coverage requires
reconciling the ledger against the exact inventory, with no missing, duplicate, pending, partial,
blocked, or stale included paths.

Source and tests are examined together where useful, but every physical file is counted once.
Test review distinguishes construction and assertion plumbing from scenario behavior, fault timing,
and meaningful intermediate-state checks. Existing test names and source-string assertions do not
create design authority.

# Finding Rules

One finding may reference many files. Record the responsibility or behavior being simplified, the
complete-body evidence, a concrete smaller alternative, contracts retained or requiring change,
verification needed, confidence, and a conservative net reduction estimate where supportable.
Subtract replacement code, adapters, and additional verification from gross deletion estimates.
Do not add overlapping or mutually exclusive findings.

Separate finding disposition (`proposed`, `retained`, `deferred`, `rejected`, `implemented`,
`verified`) from evidence maturity (`preliminary`, `file_review`, `subsystem_review`,
`whole_codebase_review`). A completed implementation entry later records its commit and actual
verification. A retained or rejected entry records why a tempting cut would lose a real guarantee
or add more complexity than it removes.

# Dependency Evaluation

For a substantial replacement candidate, establish the exact crate version and inspect current
official registry, documentation, and upstream evidence. Record relevant enabled features, license,
MSRV and target support, maintenance and real adoption evidence, known relevant advisories or
limitations, and the Beryl call sites and contracts being replaced. Download counts alone do not
establish that a dependency is battle-tested.

Compare actual semantic fit: streaming and fragmentation, allocation and residency, error and
cancellation behavior, concurrency and ownership, persistence and crash behavior, platform support,
and any other guarantees material to that candidate. Account for wrappers, format changes,
integration cost, transitive dependencies, and runtime consequences. Distinguish a compatible
replacement from one requiring an explicit product or architecture decision.

Research does not authorize installing software, changing dependencies, or rewriting implementation.
Negative and inconclusive investigations are retained when they prevent repeated costly research.

# Audit Sequence And Completion

1. Establish the exact inventory and baseline-bound ledger.
2. Complete bounded file reviews until every included file is audited.
3. Review each completed subsystem for unnecessary layers, representations, state machines,
   duplicated ownership, test structure, and viable dependency replacements.
4. After complete file coverage, synthesize across all subsystems. Merge overlapping findings,
   reconsider local recommendations in light of larger replacements, and identify abstractions
   whose sole purpose is supporting other removable abstractions.
5. Publish a prioritized final proposal with coverage evidence, conservative savings, contract
   decisions, replacement costs, verification boundaries, and remaining uncertainty.
6. Derive implementation work from accepted design decisions in the existing authoritative planning
   system. Preserve this audit as the baseline record and update finding dispositions as work lands.

Current state: complete file review was verified on 2026-09-07. All 1,910 included files and 578,865
lines are accepted: 1,847 full-body reviews and 63 declaration-only reviews. The ledger and 341
explicit exclusions exactly cover all 2,251 tracked source candidates, with no missing, duplicate,
nonaudited, invalid-depth or empty-evidence rows. Baseline identities and unchanged product source,
tests, manifests and implementation plan were checked. The
[saved pause checkpoint](PAUSED.md) is historical; the Operator resumed and authorized completion.

Subsystem and whole-codebase synthesis are complete. The [final report](report.md) reconciles all
three subsystem reviews and resolves every preliminary lead. Current-contract estimates total
20,925–30,474 net lines, including 1,660–2,460 dependent on a response-parser prototype. Excluding
that proposal gives 19,265–28,014. Contract alternatives, correctness corrections and existing
required rework are separate. These are reviewed proposals, not implemented changes or Operator
approval. Product builds/tests were not run for this observational audit.
