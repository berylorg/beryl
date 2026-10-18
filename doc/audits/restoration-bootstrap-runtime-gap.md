# Restoration Runtime Readiness Gap

## Observed Boundary

Before correction, independent semantic review found that an idle, unbound
thread without submitted history can obtain an initial submission proof before CAS is ready.
`RuntimeInterestOwner::initial_submission_preparation_open` accepted absent runtime entries and
`Starting`, and both `ExactStopRead::selected_submission_proof` and
`SubmissionProof::availability` used that preparation permission. Composer admission evaluated
the same proof and could propagate Submit without a readiness denial.

The original discovery tests did not invoke submission, and their reopened passing fixtures
checked that no submitted turn existed. That evidence established an admission-control gap.
Internal preparation permission and later dispatch fencing did not satisfy the feature's
disabled submission control requirement.

## Authority Requiring Reconciliation

[Conversation Threads](../features/conversation-threads/design.md#progressive-shell-readiness)
requires unavailable submission until the selected thread, draft, binding and CAS are ready.
Its first visible shell and healthy draft remain independent of backend readiness.

Before correction, the [backend-runtime system](../systems/backend-runtime/design.md#progressive-warm-up)
said opening Beryl and restoring conversation shells does not launch CAS. It permitted warming only for
currently selected views or admitted work, and its lifecycle permits canonical input before a
runtime exists. The [App scheduling authority](../../crates/beryl-app/doc/design-live-projection-and-scheduling.md)
preserves lazy warm-up and the ordinary initial preparation route.

The inspected restored idle selections did not acquire a runtime launch merely by becoming visible.
Requiring ready-only GUI admission without another preparation trigger would leave initial
submission disabled. Changing the startup launch trigger or allowing a Submit-triggered
preparation state needs an explicit reconciled contract rather than a test workaround.

## Operator-Approved Contract Correction

Operator approved the recommended policy with "sounds good, continue" after the concrete proposal
and startup-policy question. The owning backend-runtime, Conversation Threads and App authorities
now specify post-publication selected-view preparation and genuinely ready GUI submission.

Keep complete native publication and healthy draft input independent of backend readiness.
After the complete ordinary shell is published, its selected view acquires genuine bounded
runtime preparation interest through the existing process owner. Coalesce shared runtime demand;
catalog membership and unselected stored threads create none. Preparation sends no draft or model
turn. Cancellation and retirement retain the original runtime/session cleanup owners.

Separate permission to perform initial preparation from GUI submission readiness. The control
remains unavailable while preparation is absent or starting, and enables only for a genuinely
usable matching CAS service and root binding. Failed preparation produces the existing backend
notice while local history and draft input remain usable. Internal canonical admission and
already admitted background work retain their current preparation authority.

Clarify the backend-runtime system's startup clause to forbid CAS launch on the prepublication
restore path while permitting the selected-view demand after publication. Specify the separation
of preparation permission and GUI proof in the App authority. Conversation Threads' current
readiness promise stays intact.

The existing runtime owner already has genuine interest acquisition and a strong `Ready` state:
managed launch admits a foreground connection, initializes it, validates the exact supported
release and launch identity, then publishes readiness after process/connection/retirement health
checks. `ProjectionConnectionService::acquire_runtime_interest` was present without a production
caller. Reuse that owner and readiness authority; do not equate a live process, absent entry or
`Starting` with usable CAS, or create a parallel startup launcher.

The alternative Submit-triggered preparation policy was not selected.

## Current Implementation And Verification

Phase782 is accepted. Source separates internal runtime preparation permission from
Ready-only GUI submission and acquires selected-view runtime interest after shell publication.
Healthy local drafts, history and ordinary shell visibility remain independent of backend readiness.

Supporting corrections preserve the original observation lease during off-GUI validation, wake
ordering, current selection/root binding checks, native input ownership and cancellation custody.
Input prepaint scheduling uses the existing defer mechanism. Published widget dependencies are
listed in the active [plan](../plan.md); temporary instrumentation and local diagnostic overlays
have been removed. Native fixtures use their original Windows window identity.

Focused runtime-interest and managed-readiness tests passed, as did representative independent
producer, cancellation, threadless startup, first-editor/save and publication cases. These do not
accept the whole phase alone. Subsequent shared fixture changes received fresh affected verification.

## Storage Fixture And Scan Findings

The accepted-route continuation guard needed a private maximum row allowance of262 bytes:
77 bytes of order metadata plus185 bytes of maximal valid route data. Public byte/record limits,
caller-smaller budgets, cursor/revision checks and typed first-row refusal remain unchanged.
Promoted NEXT and terminal-heavy READY regressions cover continuation without skipping candidates.

The global source-page fixture unnecessarily constructed a reference database for each of300
threads before constructing destination-backed roots again. It now uses genuine canonical roots
from one destination Home, preserving the original256/44 page assertions and60-second timeout.
The corrected test passed in2.314 seconds. READY fixture arithmetic also had to reflect its actual
post-mutation rows: Delivered190, NextTurn Retryable191 and final Admitted157, rather than an
assumed first-row shape. The corrected continuation tests pass.

Run421 passed all167 cases in the storage library and accepted-next/ready/promotion targets.
Run422 passed26 cases before the delivery fault matrix exceeded its original60-second timeout;
58 selected cases were not run. This does not establish a production deadlock or a completed
fault-cut outcome.

The populated seed already creates three genuine destination drafts, but its filtered record
builders opened eight auxiliary reference Homes per complete seed. The twelve-cut matrix therefore
had substantial redundant setup. Four support files now pass the three destination root pairs
through those builders, including the separate active-route append. Independent source review
verified original-file recovery, unchanged default factory order, owner checks and unchanged
fault cases, current-CAS commands, provider work, recovery and scrub assertions. Fresh canonical
run423 passed both unchanged matrices: nonfault transitions in 5.673 seconds and all twelve fault
cuts in 19.015 seconds, within the original 60-second limit and with zero retries. Broader
shared-fixture verification completed through the owning groups below.

## Accepted Boundary And Evidence

Owning storage groups1–14 have completed, including the current226-case terminal-convergence
consumer refresh and175-case library/accepted-route/source-page refresh. The latter retained
the original60-second per-case profile; the300-thread NEXT source-page case passed in2.217 seconds
with its original256/44 page assertions. Recent corruption-diagnostic and live-projection fixture
corrections passed their owning targets and independent review, retaining production release,
finalization, scrub and reopen checks. Home current-domain/current-home/typed-command run447 passed
25/25, including physical revision capture after writer admission, cancellation, reentrancy refusal
and unchanged conflict ordering. Fresh App run458 passed141 original state/deterministic library
cases; run460 passed211 shared-fixture and owning model cases, including direct recovery-handoff,
startup-history, custody and resource families. Terminal startup now prepares coherent live
projections through blocked finalization frontiers before transcript publication; it preserves
provider correlation, unresolved assets, finalization and production gate checks. Both genuine
regressions passed, covering ordinary/initial-candidate/same-home recovery and later text behind
`PendingAsset`. Independent implementation and fixture review found no blocker. The owning terminal
and resolution targets completed across467/479/480 after a test inventory retry was aligned with
the exact source-revision drift outcome. The other affected integrations completed with9 creation
cases in468 and75 settlement/generated-input/runtime-activity cases in481. The production-change
library refresh passed211/211 in382.917s (469). Mapped native/mounted/scale coverage completed below.
Shared populated seeding is used by activity-service, recovery-handoff, app-service recovery,
startup-history and generated-handoff fixtures, including integration consumers.

Model-page diagnostics proved runtime contention in pre-request idle validation after earlier
menu validation succeeded. Typed pending-admission outcomes now cover only proven unissued query,
source and runtime cuts and reuse the existing menu wait, preserving exact cursor custody and
ordinary backend/post-request failure Retry. All temporary diagnostics were removed; fresh normal
run441 passed35 reader/menu/native cases with original request limits and deadlines. Independent
semantic review accepted this correction. The historical run413 lock owner and run437 inner-read
stage were not established; no claim of exact historical attribution is made.

Selected native startup/restoration, first-editor, claim, minimum-window, theme and scale coverage
and independent semantic review are complete. Older focused results alone do not supply blanket
current-source acceptance.

Independent whole-boundary source review found no blocking defect in whole-set readiness,
versioned wake custody, renewed off-GUI validation, original editor/window preservation,
claim/disposal/Retry handling, independent producers, Ready-only submission, selected runtime
custody, or the integrated native minimum/appearance publication. The105 reviewed changed
source/manifest paths matched the canonical checkout. This accepts the reviewed semantics;
fresh execution and final coverage reconciliation below complete whole-phase acceptance.

Normal native qualification covers all936 required App identities, with original modes
D282/S244/N410. The case-sensitive union uses only passing, source-relevant normal results from
441/467–472/474–477/479–482/489–493/498/500/501. No trial or diagnostic result supplies blanket credit.
Run500 completed364/377, with all13 failures confined to context-status native fixtures. Independent
review accepted strengthening those fixtures' genuine interactive-input and actual-paint barriers;
normal501 passed their complete14-case target in71.800 seconds. Normal474 passed all29 ordinary
notice cases in40.647 seconds. Normal476 passed the three original unoptimized scale cases in
348.222 seconds under the unchanged600-second profile. Production478 checked `beryl-app` and
`beryl` without test faults in32.51 seconds, with warnings but no errors. Normal475 passed all seven
large notice cases in80.628 seconds under their original optimized1800-second profile. Normal477
passed the full optimized scale workflow in583.174 seconds under its original1800-second profile,
retaining257 mutations and4096-round bounds. Original build profiles, native deadlines, payloads,
focus and disposal assertions remain intact. Both Code interaction cases passed500 after
the reviewed genuine extent-completion precondition. All11 Table consumers, resource revision
refresh, mixed history, fairness/disposal and exact notification ownership passed498 with baseline
production scheduling. Both discarded scheduling trials and all temporary diagnostics are removed.

The historical capacity timeout in483 remains unattributed. Five diagnostic passes did not
identify its failing stage; unchanged normal489 passed the owning case. Independent execution
review accepted preserving that evidence limit without claiming a cause or fix. The complete
current-relevant normal acceptance union is now established. Diagnostic499 likewise establishes no
cause for the older Code menu timeout and grants no acceptance credit. Neither494 nor497 grants
blanket acceptance from discarded production trials.

Final source-relevance review compared235 changed/untracked Rust and Cargo paths between root
and canonical:229 were byte-identical, and six differed only in formatting or independent import
order. All three corrected context files and all accepted storage fixture paths matched exactly;
Cargo manifests and lockfile matched. No behavioral mismatch invalidates scoped normal receipts.
The six formatting/import differences were aligned to root before production478. This source
review and the completed notice, scale and production checks establish the integrated boundary.

## Execution Bookkeeping Correction

Operator canceled the source-snapshot/archive workflow on2026-10-17. It had accumulated repeated
dirty-tree patches, custom reuse guards, archive tooling, arbitrary artifact budgets and excessive
receipt tracking. Those were agent-created process overhead, not product requirements or phase
acceptance conditions. Follow the existing project skills, use Git for history, rebuild changed
targets and keep only evidence still needed for current verification or diagnosis.
