# Native Transcript Interaction Evidence

Historical replacement qualification on 2026-10-13 exposed a recurring fixture assumption:
retained transcript state was treated as proof that a native interaction was ready. That assumption
produced false failures for geometry, subscriber delivery and menu activation, and initially hid
the real resource-provenance defect. It did not require changing product behavior.

Virtualized rows may retain measured geometry outside the viewport. Native fixtures must scroll
the actual outer surface, acquire current geometry again, and verify that the content hit point
is inside the viewport. Bypassing clipping or reordering production hit testing to admit an
offscreen point is invalid. Likewise, recovery assertions must inspect the canonical authored
child and narrative source rather than require a legacy text representation.

GPUI queues `Context::emit` effects and flushes them when the outer `App::update` finishes.
An action and its subscriber-delivery assertion inside that same update cannot prove delivery.
Separate the native action from the subsequent observation, including preview dismissal and
return-focus checks. Anchored menus also retain their actual focused row: disabled Copy remains
focused initially, and Enter correctly does nothing. Navigate to the enabled Edit row before
asserting its activation instead of changing menu semantics to satisfy the fixture.

Current geometry and a callback still do not prove command authority. Code menu admission
correctly rejected a provider record whose authenticated resource payload had not been copied
into its source provenance. The correction carries that same resource identity into both
projection and view records while retaining every existing ownership and currentness check.

Captured branch qualification additionally showed that narrative selection is not the complete
assistant-text surface. Code selections use canonical source coordinates already supplied by
indexed Source pages and their binding. The provider subtracts the binding start only for its
resource-relative read, then returns canonical coordinates. Adding a fence or binding offset again
would target different bytes. Native Code fixtures should use current measured source endpoints
and preserve Code-owned selection rather than reconstruct narrative selection or exported Copy
text. A retained Retry must save its currently invoking origin before staging can clear that
selection; a backup left by a different ended capture is not evidence for the retried request.

A small fully realized Code resource may retain only an indexed Display response. Its authenticated
`Fragment.text` is raw UTF-8 read from the exact `Fragment.source` interval, not a formatted export.
Matching binding, root, metadata and current rendered coverage can qualify those retained bytes
without a second Source response or offscreen read. An empty placeholder fragment proves no
nonempty selection. Requiring a separate Source page would disable a legitimate rendered capture.
Host resource pages can also be released after layout preparation, even while Code stays ready.
Capture must follow the layout owner's bounded exact-byte custody through that transition;
ready geometry alone cannot authenticate bytes, and transformed renderer text cannot replace them.
Code layout's authenticated Range seed presentations supply the original layout bytes and are
already charged. Seed reuse alone proved insufficient: range reconciliation can release them
while prepared Code remains ready, and later Display/Source-only deliveries are not seed-owned.
Exact-byte custody must follow the currently prepared source ranges, with release and charging
inside the existing realization budget. Prepared geometry and streaming text placeholders cannot
substitute for that source proof.

Indexed resource demand owners legitimately retain their creation presentation revision while
resource publications advance the shell revision. Existing owner currentness authenticates exact
activation, view/provider revision, protected record, source provenance, resource identity and kind.
Comparing that older demand stamp with the current shell presentation disabled valid Code captures.
Separate logical source-owner authentication from the fresh invocation stamp; retain exact current
menu/command and measured-layout fences. Contiguous multiline selection also needs authored LF or
CRLF coverage between rendered content lines. Use the authenticated CodeLine terminator contract,
never infer separators from formatted Copy output or bridge an oversize placeholder.

A first measured Code endpoint does not prove that the complete current viewport has finished
layout. Wait for current ready layout before a native menu invocation; a later ready frame does
not retroactively execute an earlier rejected key event. Likewise, same-view source refresh builds
a fresh core and reestablishes indexed demand through ordinary resource reconciliation. Preserved
selection alone does not prove that a checked Copy has current read custody. Wait for the current
indexed owner and settled demand before the first native Copy, retaining the real retired-source
refusal for an action whose source changes after admission. Indexed owner currentness alone is
insufficient: the checked provider read also authenticates the published head and history summary.
After failed activation, wait for ordinary publication to match the actual current parent head and
summary before invoking Copy once. Do not retry a refused native command or bypass its reader.
Likewise, a later horizontal Code menu invoked while resource staging remains in flight is correctly
invalidated by that delivery. Wait for staging quiescence before its first invocation rather than
preserving a menu across source publication.

Final diagnostic cleanup changed a fixture facts string from an optional debug value to a boolean.
An assertion still matching the old string failed before issuing the native right-click. Behavioral
readiness assertions should use typed predicates; diagnostic formatting is not an acceptance API.
An idle resource worker also does not establish quiescence when a demand has not launched or a
completed delivery awaits measured publication. Observe queued demand and staged publication
together with current source and geometry before the first native invocation.

Authenticated CodeLine metadata alone also cannot grant rendered raw-line coverage. A native drag
from an oversized combining-character placeholder's end to the next line's start can select only
its LF. Both lines may be prepared, but the placeholder retains no raw source bytes. Separator
bridging requires both adjacent prepared lines to be blank or retain a genuine raw fragment;
full selected content still needs contiguous raw-bearing coverage. Preserve positive multiline
LF/CRLF and Tab behavior while refusing that placeholder-only boundary.

Diagnostic activation fixtures must hold a resource read that the ordinary initial viewport
actually demands. A long Code line earlier in the history does not prove a pending read while
activation places the viewport at the narrative tail. Range or Display responses can also already
supply its bounded raw content without a Source request. Qualify the real requested resource kind
and current revision in the visible target, preserving normal demand and publication; do not
inject offscreen demand or add an activation gate to manufacture pending evidence.

Tail-oriented activation uses the explicit LiveTailFollowing mode. Its active semantic target is
the measured last record at the viewport bottom; the first-visible reading representative can
change when preceding row heights refine. Compare the actual published tail identity/bottom,
published viewport height, mode and manual displacement through resource settlement. Do not impose
detached reading-anchor invariance on Tail mode. Short-content leading alignment retains its
separate first-anchor proof. Neither case permits a new late placement command or readiness gate.

Diagnostic view observation must use coherent selection custody rather than creation eligibility.
The creation-target getter intentionally returns unavailable during pending activation. Observe
the existing live composer/claim agreement directly rather than equating command availability with
selection. Ordinary flush can also legitimately park the predecessor composer in Fencing while
the activation worker owns its original typed custody. Report the live selection unavailable at
that cut, alongside the unchanged published view and exact pending target; do not invent a claim
from frame identity, duplicate locked custody or weaken the lifecycle gate. Likewise, generic count/query error state
cannot stand in for an activation outcome: healthy polling clears it and query failures reuse it.
Retain the bounded exact-target activation failure at its actual lifecycle boundary, independently
of unrelated polling.

Cancellation ends at writer admission. A token cancelled while a `BeforeCommit` fixture hold is
already inside the active writer cannot require noncommit or predecessor preservation. Use a
genuinely missing exact target to qualify retained activation failure, and release an admitted
held activation to its ordinary exact outcome. EOF evidence must observe the actual stdio terminal
and attach's Exit handoff, then positive single-operation settlement and Home release. A transient
startup-command pending flag may already be consumed by the running owner; it is not a durable
receipt that EOF was or was not handled.

An EOF Exit handoff while thread activation gates Exit is not accepted shutdown admission.
The ordinary command correctly consumes that unavailable invocation without queuing a later Exit:
the exact activation can commit and settle, leaving the same window alive with Exit eligible again.
Observe the terminal endpoint, single handoff, single original activation outcome and absent
shutdown attempt. Qualify disposal with one fresh explicit ordinary Exit after eligibility returns;
do not add an automatic retry or claim that the gated EOF invocation itself released the Home.

## Combined Mounted Workflow Fixture Ownership

The first combined transcript cases compiled but failed before qualification: the ordinary
service graph required at least four workers while the fixture supplied one. Use the accepted
nine-worker ordinary graph envelope when composing its activation, replacement and branch owners;
a smaller unrelated fixture default does not establish readiness for that composition.

Mixed inline and indexed Tables exposed another invalid assumption: older single-Table test
accessors observe the first retained panel. They cannot authenticate which Table supplied geometry
or selection in a mixed transcript. Observe the exact retained Table owner and its kind, then drive
native input against that owner's current geometry. Keep such observations test-only rather than
changing production ownership to accommodate the fixture.

A directly mounted Home also omits startup's text-input binding registration. Install the existing
canonical bindings before testing ordinary Select All and Backspace; prove current selection and
binding/caret settlement rather than substituting a direct payload clear. These failures produced
assertion panics crossing a non-unwinding native callback and Windows abort `0xc0000409`, not
evidence of insufficient worker stack capacity. Their failed runs provide no workflow acceptance.

Widget quiescence alone does not establish resident selection admission. A debugger rerun observed
surface selection `0..82` while admitted composer positions still described the previous caret;
Backspace failed with `MissingPositionProof`. Wait for the existing exact current-binding and
directed selection proof with no active dispatch before issuing the mutation. Also, GPUI temporarily
owns the Window during `update_window`: a nested `window.read(app)` can report `window not found`.
Capture its root Entity before the native update and observe that Entity within the callback.

Native Code recovery/range routing passed in `run-046`; the complete grouped image workflow
passed in `run-050`, including Code/Table/marker origins, cross-marker selection, primary pointer
and keyboard actions, preview dismissal, focus return, actual Edit/import, dimming and Escape
cancellation. These lessons inform the remaining transcript workflows in [the plan](../plan.md)
and the [transcript contract](../features/conversation-threads/design.md); they are verification
guidance, not a replacement for their authority.
## Mounted Publication And Submission After Mixed Clearing

After accepted mixed replacement and default-worker clearing, full native run
`010bea12-e000-46c5-99e8-30cfad423af6` still fails both remaining workflow cases.
Test-only preservation of the original slot error identifies repeated-navigation refusal as
`Host(PublicationPending)` at publication source advance, rather than a stale receipt or
projection identity mismatch. The replacement case admits submission start but its worker returns
`SubmissionUnavailable(BindingUnavailable)`; the captured ticket identifies session generation
114 and candidate generation 3 with the reimported two-marker root. These are exact attribution
results, not evidence for refreshing a generation, weakening a guard or retrying a submission.

The fixture's submission-start `.unwrap()` alone cannot prove successful admission because the
mount may record a refusal and return `Ok`. Likewise an aggregate Unavailable status may occur
before commit or after acceptance during successor publication. Preserve the actual original
outcome and trace its owning phase before selecting a correction. Both failures end with a
non-unwinding assertion panic (`0xc0000409`), not renewed worker overflow.

Artifacts and exact fixture homes remain under
`.aipm/tasks/mounted-transcript/run-015-attribution/` through diagnosis. Complete mounted workflow,
submission, repeated navigation and the transcript Checkpoint 4 facet remain unaccepted.

Publication attribution identifies an owned convergence race in `finish_publication`: aggregate
revision changes between authenticated reads, or the session read returns `ConcurrentChange`.
The original lane and command outcome remain retained. The exclusive private driver selects and
executes the matching lane stage, but its flush caller classifies `PublicationPending` as a failed
save. Correct only owned execution/reconciliation driving to report existing bounded Progress;
preserve direct admission refusals, other errors and receipt/source/checkpoint authentication.
The actual selection-save convergence fault and full native navigation must qualify this correction.

Independent review blocks the first pending-to-progress adapter: known committed execution can
leave the publication lane `Ready` with a retained original outcome. Yielding there permits
pre-admission cancellation or a second execute that replaces the original evidence. Happy
convergence checks alone miss this interval. Add a private submitted-outcome settlement
continuation, resume original custody without execution, and prohibit cancellation election after
admission. Renew execution and reconciliation races with one original execute and cancellation
between yield and resume, plus the shared autosave/lifecycle consumers. Existing settlement
helpers suffice; this does not require a durable format or storage authority change.

The corrected private `Settling` stage preserves the original prepared command and known outcome
before convergence reads, resumes classification without execution/reconciliation, and excludes
cancellation or direct release after admission. Independent source review approves. All four new
convergence/cancellation/autosave/disposal cases pass in run 018
(`d7401dfd-53a7-48c8-ace9-4a0da7fc7732`, 7.995 seconds); 47 lower publication/lifecycle cases
pass in run 019 (99.271 seconds), including exact same-session replay and foreign-session refusal.

Native run 020 (`e4eb621f-dd48-47ec-8306-c00cdd26932b`) passes the complete mixed-content
navigation/invalidation/disposal case in 18.528 seconds. Its replacement case captures a wake with
both binding/window absent and reaches `SuccessorReady`, then fails later literal synthetic-context
readiness. The reused Code wait helper's `facts=None` output only means no Code panel; it cannot
attribute literal-context residency or paint. Preserve readiness and inspect the actual context
record, provenance, realized range and measured endpoint geometry before any fixture/source change.
The earlier BindingUnavailable observation remains historical; a status-worker overwrite was a
static hypothesis and is not confirmed by this successful submission capture.

## Created Discussion Transcript Construction Gap

Native run 023 (`a947f08c-88b5-47da-af1f-27e36ca0d374`) confirms the current panel is the
fixture's retained panel and delivery is not pending. The activated child has a stale transcript
head at generation 1, zero entries, no transcript build and an incomplete history summary.
The literal-context wait cannot succeed: the active panel correctly contains no readable records.

`InitialThreadRecords::new` initializes an inherited-tail child with that stale head. Discussion
creation publishes canonical child/context records and Catalog readiness, while the inspected app
Start/AdvanceTranscriptBuild callers are confined to CAS convergence. The read-only provider
correctly publishes an empty Unpublished seed; coherent activation alone does not create a
readable projection. This is an actual missing construction owner, not stale panel geometry.

Implementation is paused under the Operator's supplied AGENTS.md rule for a technically unworkable
plan step. The clean prerequisite is a bounded selected-transcript construction owner using the
existing canonical Syndic build operations. Establish exact thread/generation admission, original
command outcome custody, cooperation with CAS-owned builds, bounded progress, view-change/disposal
and typed failure in owning design before implementation. Preserve read-only provider semantics;
neither fixture projection injection nor launching CAS substitutes for the missing owner.

Retain the qualified publication correction and its 4/47/20 passing checks, saved full workflow
drafts and exact fixture homes through prerequisite design and integration. Complete mounted
acceptance and the transcript Checkpoint 4 facet remain unaccepted.

The Operator approved the prerequisite on 2026-10-14. Construction is accepted in `7e35c562`;
see the [acceptance](../audits/selected-transcript-construction.md). Native run 006 now proves the
actual child head is Current with two entries, a Complete build and complete summary. Exact
synthetic context becomes readable through ordinary refresh without fixture projection injection.

## Native Fixture Admission And Current Context Geometry

The explicit storage-only submission fixture mode must survive periodic worker attachment.
`with_window_worker` attaches selected-window authority even to a wake with no backend target;
the fixture's one-time source replacement could therefore lose its intended mode. Preserve this
explicit override only in test configuration, leaving production submission guards unchanged.
After that correction, native run 005 completes submission and refuses a later navigation source
election before admission while construction writes advance Home observation. A fresh bounded
ordinary invocation may follow that exact no-effect refusal; never replay an admitted claim.

Native run 008 distinguishes the next failure: Copy ordinal 0 reports no eligible selection,
current window geometry is false, and narrative ranges are absent. Ordinary worker admission is
available and unsuspended. The first Markdown Copy is synchronous, so this cannot be attributed
to stripped-Copy worker capacity or completion. Later geometry recovery cannot recreate a lost
drag selection. Literal endpoint availability alone is insufficient for pointer dispatch.

Draw and verify the actual window's current character geometry and completed ordinary attachment
before dragging, recompute both exact endpoints, and assert the retained complete source range.
Keep normal source work and worker custody running. Native run 009
(`1b2c1a6e-a90f-4ef9-b350-f85b1376267f`) then passes the complete replacement/cancel/submission,
branch creation/Retry, real child context paired Copy, Back and disposal case in 7.563 seconds.
The writer setter itself does not invalidate geometry; do not blame it without evidence.

The Windows `0xc0000409` label in these failed native runs is produced by a fixture assertion
unwinding through a non-unwinding event callback. Inspect the original panic before attributing
that generic label to an actual buffer overrun or stack exhaustion. Final combined regression and
whole-workflow acceptance remain separately required.

## Saved-Source Fixture Head And Build Coherence

Combined run 024 (`dffa99ff-2555-4b6b-bc1f-107c24af0658`) passes 289/297, including both
complete mounted workflows. Seven saved-source refresh failures use fixture contributions that
advance a Current transcript head and history summary without advancing the matching Complete
build manifest. Storage validation requires their revisions and selected path to agree. The
selected construction owner correctly refuses that malformed state, so the panel retains its
previous revision. Repair fixture coherence rather than bypassing construction or weakening
production validation. The fragmented live tilde delimiter failure has a different setup and
requires separate diagnosis; do not attribute it to this fixture mutation.

Run 026 (`031a6822-2948-4824-99bb-3a245eecd153`) passes all seven repaired saved-source
cases. The unchanged live delimiter case still fails: its final head and Complete build agree
at generation 4/revision 11 with two entries, while initial panel activation and resident records
remain absent. Final coherent storage alone cannot explain or clear an earlier retained refusal.
Inspect the original published reader's retained capability without stepping it or substituting
a newly issued reader, which would hide the original outcome.

Public run 027 (`825cf290-e23e-48fa-928e-4a3b458f09db`) passes 189/189 after correcting
obsolete source-text bans on immutable resident descriptor types and the mounted menu callback
location. Storage operations remain forbidden. A mismatched renderer record ID/index is exactly
not realized; correcting the index admits its intended target. Existing stale source, revision,
geometry and provenance checks remain intact. Both fixture and public corrections pass independent
review. Canonical run 028 validates locked metadata and production app/executable source in 14.83
seconds; the remaining live failure still blocks workflow acceptance.

## Live Turn-State And Active Transcript Build Supersession Gap

One isolated replay passes (029), but unchanged functional inputs reproduce failure in 030. That
run fails before activation: ordinary preflight returns the exact loaded projection with
`ConcurrentChange`. Selected construction is legitimately waiting for item projection, with no
terminal failure. The native capture helper incorrectly invalidates the session after that one
refusal. Its test-only caller disposition now permits at most eight fresh invocations on only an
exact-thread PreActivation ConcurrentChange, retains original refusal sources and consumes the
exact returned projection. It never repeats admitted or uncertain execution, and matches the
existing production scheduler's disposition. Independent review approves this correction.

Run 031 attempt 1 reproduces the separate post-activation blank display. Its original selected
construction capability retains two committed commands (Home revisions 17 and 19), followed by
NotCommitted TranscriptBuildConflict and terminal CommandNotCommitted. CAS later produces a
coherent Current generation 4/revision 11 head and Complete build with two entries, but the initial
panel remains at activation 0 with no resident records. The later capture cleanup's
TerminalHistoryCompletionConflict is secondary to the display timeout.

The SDK requires Collecting/Publishing path snapshots to match current turn state and publication
rejects any revision, lifecycle, source/item frontier or timestamp drift. ActivateBinding changes
turn state then calls `refresh_current_path_state`, which skips stale heads. TurnActivated likewise
changes state with `transcript_dirty=false`. A selected build can collect before either neutral
transition and subsequently fail publication while the thread/head/build observation remains
unchanged. The log does not distinguish which transition crossed collection; both have the same
missing active-build lifecycle disposition. Later item events supersede/rebuild the transcript,
explaining the coherent final head without clearing the original owner's terminal custody.

Implementation pauses under the Operator's technical-plan rule. The clean prerequisite belongs
in Syndic: atomically supersede affected active builds when selected turn-state advances invalidate
their captured path snapshots, while preserving neutral Current-head refresh. Keep the six-read
construction envelope, exact peer-progress proof, original outcomes and read-only provider.
Do not classify every build conflict as progress, clear retained failure, issue a fresh reader to
hide it, replay a refused effect blindly, or freeze ordinary CAS work in the native fixture.
Define the lifecycle correction in owning authority before production continuation and qualify
both activation transitions racing collection/publication, followed by the actual streaming
native workflow and combined regressions.

Decision evidence: `mutation/binding/active.rs`, `mutation/live/event.rs`,
`mutation/transcript/refresh.rs`, `mutation/transcript/publication.rs` and
`validation/projections/transcript/snapshot.rs` under `crates/syndic-storage/src/`;
`.aipm/tasks/mounted-transcript/run-031-live-disposition/attempt-1/tests.log` retains the original
capability receipt/refusal diagnostic. The accepted construction boundary remains recorded in its
audit; the complete concurrent mounted workflow remains unaccepted.

The Operator approved the atomic supersession prerequisite on 2026-10-14. The shared Syndic state
effect uses existing checked invalidation and contributes state, superseded build and replacement
head together; it leaves Current refresh and uncaptured/no-active-build cases intact. Eleven
deterministic cases pass, including both activation transitions in collection and partial
publication. Broader storage verification exposes a separate recovery-fixture precondition:
`terminal_home` never projects its known submitted item before `finish_transcript` requires that
saved projection. Preparing that exact item's derived projection, with complete TurnState equality
and unchanged FinalizingHistory assertions, corrects the fixture without freezing/finalizing,
releasing a gate or weakening terminal evidence. Independent correction review approves. Required
real streaming, corrected recovery and canonical checks remain pending before acceptance.
