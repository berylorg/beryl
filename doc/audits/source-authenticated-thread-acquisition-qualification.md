# Source-Authenticated Thread Acquisition Qualification

## Boundary

This qualification covers canonical-source election for ordinary same-window and additional-window
New Thread acquisition. It does not accept the coherent published catalog source coordinator,
frozen query service or Thread Switcher mount. The owning contracts are the
[storage system](../systems/beryl-home-storage/design.md#thread-claims-and-empty-thread-acquisition),
[app catalog boundary](../../crates/beryl-app/doc/design-catalog-and-composer.md#catalog-and-claims)
and [Syndic eligible-empty boundary](../../crates/syndic-storage/doc/design-history-storage.md#empty-thread-reuse-eligibility).

## Source Readiness

Independent source review established that committed canonical witnesses cover the mounted
Catalog-affecting producer families. Syndic Thread, Attributes and History revisions cover input
admission/promotion, draft publication, live capture/convergence, stop, compaction, incomplete
recovery and discussion archive changes. Exact optional full Session claims authenticate both
reverse copies or absence. Runtime/root source records authenticate their facts independently.
Canonical user-input title content is sealed and distinct from provider-owned content freezing.
An unchanged compact-summary revision or Catalog Current flag alone is insufficient evidence.

Bulk Session restoration reserves up to 512 claims per claim family. Maximum-sized dependent
Catalog pairs cannot be copied into its existing 64-MiB encoded-value command envelope. Universal
physical markers would also reject source writes when a rebuildable row is absent. The owning
contracts therefore recognize exact committed source-witness disagreement as durable invalidation,
with mandatory source authentication and completeness before consumer use. The accepted physical
marker remains a bounded optional operation with its original missing-row rejection.

Review found two concrete acquisition defects: cached claimed rows could exclude a thread after
its actual claim was deleted; additional-window reuse applied strict fallback-cleanup eligibility
to a thread made empty after unsubmitted typing and removal. The corrected contract requires
canonical thread discovery, live occupancy and eligible-empty authority for both entrances.

Independent architecture and plan authoring review accepted the bounded correction. Natural-state
additional-window reconciliation may authenticate the exact persisted acquisition fingerprint and
fresh eligible closure of that selected thread; this remains separate from the original command
receipt and retained outcome audit. Strict original created-fallback deletion stays unchanged.

## Implemented Acquisition Boundary

Both production acquisition paths now walk canonical Syndic thread pages before authenticating
live Session occupancy, immutable execution, eligible-empty source closure and State jobs. Election
retains one bounded page and the best deterministic oldest candidate, including threads whose
Catalog projection is absent or stale. The selected compact-summary and complete Catalog successor
join the original claim command with source validations; unrelated stale rows require no repair.

Additional-window reuse retains an opaque original eligible-target outcome covering its complete
source closure and sole planned compact-summary successor. Fresh-generation inspection can qualify
that exact outcome without replay or deletion authority. State facts leave creation origin
unqualified until the composing owner positively authenticates it. Natural replay uses the exact
persisted State fingerprint and fresh selected source; reserved fallback thread/draft collisions
reject rather than invent creation provenance. Reused abandonment validates its source and releases
only the exact claim; original created-fallback deletion retains strict pristine authority.

## Verification

The new Syndic canonical discovery surface has explicit item and encoded-byte ceilings, full
identity-range coverage, exclusive continuation and key/value identity checks. Its focused
nextest run `d076b531-ed83-46e0-8238-3f59e710b8c1` passed all four cases in 3.732 seconds:
empty complete source, identity ordering through both range endpoints, bounded oversized requests
and insufficient-byte refusal. The dev all-target Syndic check passed in 51.51 seconds.

The focused Syndic discovery, eligible outcome, pristine and summary-pair run passed 26 cases
(`0445a0a9-c422-4e7d-b219-159ec67ed51e`). A broad State run passed 239 of 240 cases before an old
no-scan fixture was corrected to qualify its positively known created origin; all 22 affected
State acquisition/abandonment cases then passed (`5582a4ca-6bce-4a62-b404-f239edcb7fa3`).

The first complete affected app run passed 134 of 141 cases. Its seven failures traced to the
shared edit fixture's unnecessary marker seal, an old fixture assuming creation instead of canonical
reuse, and a real release restriction incorrectly deriving origin from initial Catalog revision.
After correction, all fourteen new acquisition/native preparation cases and the corrected recovery
case passed (`33d6f29d-11aa-4932-b861-f60761ce4582`, 15 cases in 19.107 seconds). The release correction
preserves exact Current paired rows, scope, full active claim and claim revision checks.

The canonical all-target check for app, State and Syndic passed in 2 minutes 23 seconds, with actual
compiler paths from the canonical checkout. The published TextInput and Settings widget revisions
and the canonical lockfile remained unchanged; no ignored local dependency patches were used.
The complete affected app run passed all 141 cases without exclusions
(`5583ac20-2946-4f74-a723-6e83fc416bbe`, 233.587 seconds).

Canonical State qualification covered all 240 cases. The broad run
`1b782121-5aab-4ffc-865c-0e3fd8eb69bf` passed 239 cases; its remaining old fixture still expected the
removed initial-revision release restriction. Only that test was corrected to prove initial-row
release preserves its unclaimed successor, partial cleanup remains a collision, and strict reused
row deletion rejects. The complete affected abandonment binary then passed all eight cases
(`71bde028-ee75-4342-a823-8e9c6303d929`, 3.540 seconds). Production inputs were unchanged between
those runs; this is reused broad evidence plus focused correction evidence, not one clean 240-case run.

Independent completion review cleared the source, claim, cancellation, original outcome/receipt,
natural replay and cleanup boundaries, including the runtime corrections. Thirty exact source/test
paths have final SHA-256 correspondence between the working tree and canonical checkout. Two new
Rust modules had mixed line endings normalized during staged review; their non-whitespace source
was unchanged. Scoped formatting with child traversal disabled and staged whitespace checks passed. The canonical five-target
Syndic run passed all 26 cases without exclusions (`ebf337e4-9ec4-41d3-a5bc-1c99dd2a2b95`,
32.409 seconds), including bounded discovery, exact original outcomes, generation/home fencing,
writer races, validation-only reuse and strict original fallback cleanup. Completion review accepted
this acquisition boundary on 2026-10-08 with 407 distinct canonical cases covered as described above.

## Evidence Limits

This boundary does not qualify frozen queries, full background producer readiness or the visible
Thread Switcher. Native editor/startup tests exercise their existing worker preparation and cleanup
routes; this acquisition qualification does not establish an interactive Windows session or live
CAS behavior. No dependency, manifest, schema, supported storage budget or product policy changed.
