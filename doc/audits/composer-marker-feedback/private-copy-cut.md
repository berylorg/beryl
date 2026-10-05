# Private Composer Copy And Cut Qualification

Status: accepted after independent semantic review, 2026-10-05. This is evidence for root
plan phase 729, not acceptance of mounted paste or the complete large-draft clipboard workflow.

## Controlling Boundary

The [composer](../../features/composer/design.md#image-clipboard-semantics),
[private source system](../../systems/image-assets/design.md#private-composer-clipboard-sources),
[app ownership](../../../crates/beryl-app/doc/design-catalog-and-composer.md#clipboard-operation-ownership)
and [Syndic readiness](../../../crates/syndic-storage/doc/design-draft-storage.md#marker-readiness-inputs)
control the implementation. The selected composer must publish one compact source only after
complete checked text/metadata acknowledgement, and cut must use one exact ordinary mutation.
Candidate/cut authentication and foreign label allocation remain Syndic-owned.

The process graph owns the current home-generation handle. Composer configuration and recovery
adapters carry that same handle; graph retirement permanently revokes it. Source selectors alone
grant no paste. Checked snapshot acquisition, external clipboard replacement observation, complete
fallback correlation and captured private admission remain phase 730 qualification.

## Verification Scope

The canonical checkout excludes ignored local Cargo overrides and uses the four published pins
accepted by [native boundary qualification](checked-native-clipboard.md). Commands use one Cargo
job, LLVM linking, no normal debug information, nonincremental compilation, the locked graph and
serial nextest. The Operator's working clipboard is excluded; no Beryl GUI is launched.

Focused source and lifecycle tests, canonical app/executable checks and independent semantic
review are required before acceptance. Bounded local logs live under
`.tmp/private-composer-clipboard-evidence`; they supplement this durable evidence.

## Development Corrections

The initial canonical Syndic test run built successfully, then rejected authenticated foreign
candidate evidence because the existing durable evidence validator allowed only preserving
groups. The correction permits Syndic-derived allocation with the same candidate/cut selector
bytes and validates the exact group label/asset tail. Dedicated codec cases cover both candidate
and cut forms and malformed or mismatched groups. No durable format change follows from this
correction. The corrected Syndic suite passed all 49 cases.

Committed-begin replay fixtures initially used a helper requiring a newly committed contribution.
Exact equal replay instead returns Syndic's empty contribution, then ordinary staging outcome
reconciliation selects the committed target without advancing the domain revision. Both candidate
and cut replay cases now verify those exact outcomes after source expiry.

Self-review found that a stale page completion could leave its preparation lease occupied. The
correction releases only the exact captured clipboard operation, never a newer scan or source.
A mounted case holds a page, adopts a new origin candidate, releases the old completion, then
proves zero clipboard writes and successful acquisition by another mounted composer.

The cut Undo fixture initially seeded markers without authenticated label protection. It now
uses production Asset publication and mounted evidence/readiness/staging, verifies predecessor
protection, and proves exact predecessor restoration and token expiry with one actual Undo.
See the [fixture failure lesson](../../failures/composer-history-fixture-protection.md).

## Accepted Results

The final canonical locked app/executable all-target check with fault features passed. All 56
changed source/test paths match the canonical checkout, independently verified. The unchanged
published dependency graph and native boundary evidence were reused within their accepted scope.

Syndic qualification passed 49/49 cases, run `dbd21465-d3d5-4374-ae33-545541752f93`.
App qualification passed 29/29 cases, run `fcaf23f3-05ee-4d2e-b199-a3e02912e0f4`, covering
mounted copy/cut, clipboard failure, stale capacity release, exact cut Undo, notification priority,
recovery and shutdown. Tests use isolated GPUI contexts and injected complete-write acknowledgement.

Independent review accepted source derivation and origin fencing, committed replay, bounded
provenance, acknowledgement before deletion, graph custody, promotion/noncommit/ambiguity,
one-step history, expiry, stale release and production/recovery mounts, with no blocking findings.
Mounted paste and current-platform-snapshot admission remain the next acceptance boundary;
large-draft clipboard refusal preservation remains separate qualification.
