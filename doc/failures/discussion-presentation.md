# Discussion Presentation

## Local Observation Must Fence The Write

A revision increment after a local registry/disposition write cannot protect an already prepared
snapshot: publication can hold the revision lock while another thread changes the facts and then
blocks trying to increment it. The old snapshot can publish against the unchanged revision.

The presentation boundary must guard each short in-memory mutation before changing those facts.
Keep storage, reconciliation and command I/O outside that guard. This applies to admission witnesses
before a durable latest-attempt pointer exists, and to local Collision without any durable write.
The owning contract is [Discussion presentation](../../crates/beryl-app/doc/design-discussion-presentation.md).
Runtime and complete-boundary [qualification](../audits/discussion-presentation-qualification.md)
accepted the correction and its evidence limits.

## Elected GUI Application Must Not Requery Its Source

The mounted discussion test reached ordinary branch invocation and then timed out waiting for
publication. Its provider publication election already held Home coherence, including Home health.
Applying the prepared discussion snapshot called `reader.current()`, which queried the same health
mutex and deadlocked.

Check currentness before the exact election; inside it, apply only the pre-elected identities and
prepared in-memory presentation. Recovered publication needs the same separation between fallible
source/appearance/protection checks and pure prepared application. Replacing the health lock with a
weaker check or publishing outside the election would lose the coherence guarantee. Fresh mounted
qualification accepted this correction.

## Prepared Release Must Preserve Existing Lock Order

Preparing a retained selection release by holding its acquisition-registry mutex before process
reopening reverses the order used by a window reservation's destructor. Borrowing the original
retired selection lease excludes its own destructor but does not exclude other reservation owners.

Prepare and validate the exact release intent before the local presentation election. Under the
existing CAS-to-process reopening guards, try the short registry ownership election before entering
Home election. Busy or changed ownership retains the original fence and custody. Only prepared
memory changes run inside Home election; release the guards before dropping retained owners.

## Durable Settlement Does Not Retire Original Local Capabilities

Retaining a retry outcome in the process attempt and immediately releasing that attempt after an
ordinary exact-new audit still lets widget disposal drop the last original later-failure or
local-finalization evidence. Natural-record agreement alone does not establish original attachment
retirement.

Retain those obligations in the original bounded slot. Release qualification uses the actual typed
fresh same-Home recovery candidate, exact natural/registry settlement, original Home/path and opaque
opening identity, plus expiry of the exact original owner. Generic borrowed candidate access also
supports initial opening and cannot prove that recovery disposed the original physical attachment.
Independent lifecycle review and owning runtime qualification accepted this distinction.

## First-Frame Observations Follow The Last Preparation Mutation

Preparing an early restored window's status before later windows publish claims/editor commands
makes its global Home observation stale before hidden mounting. A strict publication election then
correctly refuses that earlier snapshot.

Refresh the prepared status off GUI after the restore set's final preparation mutations, preserving
each original editor and coherent publication. Recovery likewise revalidates its prepared tuple at
the final ready boundary. This is a preparation-order requirement, not permission for a constructor
to query storage or for a discussion to appear temporarily writable without its prepared status.

A recovered first-frame Retry descriptor is also an observation capability, not a stable job key.
Its equality includes Home and local observation revisions. A later live descriptor for the same
exact job can legitimately differ after readiness or other Home mutations. Verify unchanged
thread/job/revision/attempt and recovered generations, then let the mounted action capture its
current descriptor and epoch. Requiring equality to the historical capability can prevent a valid
explicit Retry forever; it must not be fixed by bypassing freshness or reusing the old descriptor.

## Unpublished Session Allocation Is Separate From Edit Permission

Restoring a retryable-failed discussion repeatedly failed its native startup deadline even after
the fixture settled the exact resolving turn. The session-opening writer applied the discussion
edit guard before allocating the unchanged candidate head, opening receipt and private history
fork. Pending and archived discussions therefore could not construct the inert or readonly editor
required by the feature. Existing live editors hid the error because exact opening replay returned
before that guard.

The owning contract permits unpublished preparation. Qualify structural thread/archive/gate
consistency for session allocation while retaining every actual edit, history-adoption, publication,
input and steering guard. Preserve the original bounded opening, receipts, outcome custody and
unchanged-session disposal. A separate readonly session family, fake writable gate or abandoned
startup retry would not repair this boundary. Owning storage and native first-frame/recovery
qualification accepted this correction.

## Recovery Probes Must Observe Current Service Custody

A passing exact-release test later raced a legitimate transfer of services away from the GUI.
Its follow-up probe called the test-only GUI-service getter after a separate completion observation
and panicked. A completed lifecycle outcome is not proof that a later GUI turn still owns services.

Observe completion, current service presence and the needed graph/job/generation under one retained
owner borrow. Keep the prepared-release tuple separately and compare its exact generation afterward.
Temporary transfer means bounded waiting; it must neither fabricate a current graph nor weaken
window/editor/job identity assertions.

## Shared Gate Refresh Does Not Own Every Enable Transition

Moving history-completion enablement into common gate synchronization removed its old `is_live`
condition. Release fencing already disables input but still permits admitted work to pump; a late
completion could therefore reenable it. Conversely, cancelled-navigation resume directly enabled
input without consulting the current discussion contribution.

Live synchronization computes the existing pending/startup/discussion eligibility. Other lifecycle
owners retain their established enable authority: preserve ordinary non-Live state and the explicit
RecoveryFenced staged-readonly rule. Resume must use the same discussion-aware synchronization.
Do not globally disable every non-Live phase or introduce stronger recovery protection; Detached
and prepared recovery intentionally retain their existing readonly interaction.

## Component Reconciliation Does Not Prove Graph Recovery

A component test manually reconciling an uncertain admission released its original audit and slot
before candidate convergence. The actual process recovery prefix handled only committed Retry
obligations, so an ordinary retained admission could strand that same slot when the fresh graph
tried to acquire the job. An admission may have retained custody even without a visible durable job;
the live-job index cannot discover every such operation.

Exercise the original tool route and graph replacement without test-owned reconciliation. The
existing bounded process audit registry must settle exact old/new command outcomes before live-job
convergence, retaining pending or Collision evidence instead of dropping custody to free capacity.

A Collision fixture must not globally fence process admission and expect to reopen it afterward.
The exact Home reconciliation already seals Collision as a retained Closed scope; global coherent
reopening correctly requires vacancy and returns `ReconciliationPending`. Unrelated ordinary work
can still reserve another scope on the healthy Home. Synchronize contradictory fixture setup with
the original retained job Flight and completed worker capture instead of changing that lifecycle
guarantee or vacating original Collision custody.

The same retained scope also blocks a fresh ordinary selection's final coherent publication.
Its actual refusal was `Coherence(ReconciliationPending)`, collapsed to `Running threads source
changed`; repeated native draws cannot complete that selection. To qualify mounted Collision,
drain prior child observations through the sequential poll's healthy unviewed positive proof,
pause selected discussion observation, mount the child while healthy, then execute the actual
uncertain admission and contradiction. Resume observation only after the original audit classifies
Collision. This proves projection on the already selected editor, not navigation into Closed scope.

## Read The Reconciliation Entry Point Before Diagnosing Status

An initial diagnosis treated an indeterminate command's visible job as unconfirmed status even
after the reader had legitimately reconciled its original audit. The reader already performs that
exact reconciliation before constructing the status snapshot; a confirmed Resolution pending
snapshot is then correct. Requiring Open until a test manually reconciles invents a stronger policy.

Keep that valid observer behavior. To qualify process recovery independently of GUI reconciliation,
remove the child's selected observer through ordinary navigation before the actual tool call, then
exercise the retained original audit through graph replacement without test-owned reconciliation.

## First Local Submission Can Have An Inherited Tail

Initial submission eligibility assumed an empty committed tail. A newly captured discussion owns
an unsubmitted context draft but inherits its source turn, so that test rejected its first ordinary
submission before context-aware CAS preparation could run. Direct execution fixtures had already
admitted the first input and did not exercise this GUI prerequisite.

Qualify the exact current context-owning draft and its discussion submission intent through the
existing initial runtime route. Preserve Unbound, idle-input and readiness requirements and ordinary
loaded-binding checks after first admission. Create the native source reply through real ordinary
execution; a fake seeded CAS binding cannot qualify a live parent projection.

## Native Input Preflight Must Cover The Actual Mutation Gate

A native helper intermittently inserted no text or obtained no submitted successor despite an
enabled, settled input and retained submission proof. Enabled inputs may still be readonly while
their discussion contribution is refreshing, and retained submission proofs can become unavailable.
An IME call can return without mutation when the gate or platform selection is unavailable.

Check current proof availability, exact editor/mount identity, the actual writable fresh
contribution, platform selection and editor readiness in the same native callback that issues the
single input action. Wait on preflight before issuance rather than replaying an unclassified input.
For shell keys, capture the root entity before the window callback and release its read before
dispatch: nested window lookup fails while GPUI owns the update, and holding a typed mutable root
borrow conflicts with shell actions. This corrected helper admitted real source text through Enter;
it does not by itself qualify subsequent ordinary dispatch or the complete discussion flow.

## Copied Verification Inputs Must Actually Recompile

A failed input copy allowed an older test source to compile. Correcting the copy preserved the
edited source's earlier timestamp, so Cargo reused the old artifact even though checkout hashes
then matched. The unchanged reported test name exposed the stale verification result.

Hash equality proves checkout identity, not artifact freshness. When synchronizing changed inputs
into a verification checkout, advance their destination timestamps before Cargo and check that the
changed target or test identity actually compiled. A passing older case supplies no evidence for
its intended replacement.

## Preserve Authenticated Pending Provenance Through Preparation

The recovered-pending scan authenticated the original canonical input and immutable turn kind,
then discarded that kind. Managed preparation consequently reread complete pending evidence through
the discussion reservation path even for an ordinary source. A concurrent Syndic mutation rejected
that redundant read with `ConcurrentChange`; no session was created, and its completion notified
only idle maintenance after the original pending scan had ended.

Carry the compact original authenticated source into the private service-issued admission. Known
ordinary user and lifecycle sources avoid that discussion-only lookup while generated and unknown
sources retain fresh exact reservation custody. Preserve home/service, worker, flight, binding and
selected-turn checks before effects. Unconditional preparation-completion retries would add a
self-wake loop and could spin on permanent refusals; they do not repair the ownership boundary.

Generation tests must use the actual same-Home recovery boundary. A fresh independent open resets
its numeric Home generation, so close/reopen alone does not prove retired-generation rejection.
Private original-service custody excludes transporting scheduler admissions across openings;
numeric source fields alone do not establish an opaque opening identity.

The launch-spec lookup also correctly rejects a changed whole-Home read interval. Its generic
`Changed` result discarded the observed durable advance; no-session completion then issued only
idle maintenance, leaving the original accepted input pending after the canonical scan exhausted.
Preserve that specific progress evidence through completion and the existing generation-fenced
execution-ready notifier after releasing original custody. Stable mismatch, absence and read
failure provide no such evidence and must not create completion-driven retry loops. Weakening the
cross-domain read guard would lose the exact target proof rather than repair scheduling.

## Qualify Progress At Its Original Pre-Effect Stage

A controlled native run reached the first child's projection after registration, then reported
`RecoveryProjection(ConcurrentChange)`. The scheduler classified it as projection refusal and
closed the service while its original accepted input remained pending. An earlier run with
Operator GUI interaction is excluded; the untouched repeat independently reproduced the refusal.

The bare error cannot authorize retry. Context seed preparation uses it for revision drift and a
stable non-seeding basis, and recovery cursor creation can return it after fork/start. Cleanup can
return that same bare primary after releasing the created target. Reclassifying all such errors
would erase the distinction between read-only planning and possible remote effects.

Retain observed progress only at the exact first-context seed preflight, before source lease
acquisition or a remote request. Require the healthy uncancelled attempt's current Syndic revision
to differ from its freshly authenticated basis. Carry that stage-specific outcome through both
scheduler worker paths; after joining the original worker and releasing custody, reopen the
existing bounded pending/next lanes through their normal generation checks. Generic concurrency
errors and post-effect uncertainty remain outside this continuation.

Do not infer that seed preparation is the actual failing stage from this shared error. The first
seed-specific guard passed its real-domain tests, but the controlled native run still returned the
bare error. Wire receipts then showed a child fork followed by unsubscribe, without injection.
The earlier pre-RPC attribution was unsupported for that run: cursor opening after target creation
can produce the same error, and successful cleanup unwraps the primary. Qualify both the originating
stage and cleanup result before extending continuation semantics.

Bounded stage tracing subsequently established successful seed/body preparation followed by cursor
opening failure after the fork and before any injection invocation. Preserve the frozen cursor
source: continuing on the same target would weaken its identity contract. Qualify revision advance
before abandoning the unused target, and require successful original lease release and exact stale
publication before offering a fresh projection attempt. Abandonment publication itself advances
Syndic and cannot supply the progress witness. Failed or uncertain cleanup retains its original
error custody; cancellation and retired Home generations remain fenced after cleanup.

Cursor-open qualification alone also misses SDK source validation after invocation. Native receipts
showed `InjectionNotDispatched` with exact `RevisionDrift` and `transport_bytes_written=false`,
followed by global scheduler fatal. The SDK's write progress is monotonic over the entire message,
including earlier streamed frames; false proves no request bytes were sent. Qualify only that
typed zero-write drift with a successful unequal Syndic revision witness obtained before cleanup,
then require the same original target abandonment and fresh authority checks. Do not treat an
invoked API as a dispatched effect, or generalize the exception to another source error, a written
prefix, unknown dispatch, failed cleanup or an advance supplied by abandonment itself.

## Terminal Settlement Must Return Usable Projection Custody

The complete native flow executed both child turns and correctly deferred resolution for queued
input, then could not submit the parent's next ordinary input. Its mounted editor, selection,
platform range and mutation positions were current, but submission reported `BindingUnavailable`.
Healthy terminal settlement had released the actual loaded projection; later submission required
its now-missing live stamp. Navigation and selected runtime warming correctly performed no CAS
projection work, so waiting for them could not repair the lost custody.

Owning direct execution tests retained their returned projection themselves and therefore missed
the scheduled return boundary. Queuing a second input while the first turn was active also missed
the later idle-submission failure. Preserve the process owner's real one-slot projection custody
through ordinary terminal return and exclusive checkout. Pair that retention with exact idle
extraction, cleanup outside ownership gates and fresh retirement validation; retaining an idle
lease forever would replace the availability failure with a lifecycle leak. Do not substitute
weaker submission proof or backend work during navigation for the missing ownership transfer.

Trace low-level settlement before diagnosing a late return as lost failure custody. The original
loaded lease's drop selects its connection's exact persistent-failure branch and revokes only its
own key, owner, generation and token without ordinary backend retirement. Bypassing the explicit
disposer's accounting counter alone does not prove an ownership leak or require another disposer.
Fallible reuse checks should still keep the original projection available to outer outcome handling.

Retaining idle authority does not promise reuse after a new canonical input. Idle admission and
accepted-input promotion deliberately publish Unbound; native planning then obtains the correlated
parent source through Resume. A test forbidding unsubscribe/resume after that accepted mutation
mistook changed binding authority for a failed Current predicate. Require intact idle custody and
the exact same-source resume/input instead of bypassing native publication requirements.

Moving old-target disposal from provider checkout into projection preparation also moved its
failure classification. The provider had declined only the affected runtime; a generic
`LeaseRelease` at the new boundary became global scheduler fatal. Preserve the exact pre-projection
stage in the outcome, original cleanup owner and pending input. Do not broaden every lease-release
or unknown projection failure into an automatic retry.

Returned idle custody must also join the admitted shutdown drain. A completed predecessor's
projection can otherwise block session retirement while a captured pending successor correctly
prevents ordinary idle cleanup. Settle the drain's own available projection through its original
cleanup authority, then revalidate retirement; neither execute the pending successor nor bypass
the external-lease, checkout, promotion and cleanup exclusions.

Holding the original command and process fences does not imply idle maintenance or late lease
return has stopped. Those ordinary paths can already own session or connection authority while
consulting the command gate. Blocking on either inner mutex from fenced shutdown reverses that
order and can deadlock. Use shutdown-only nonblocking acquisition for the bounded extraction and
retirement election, returning waiting on contention through the existing drain. Keep backend
release outside all gates and preserve ordinary lock order and exact ownership exclusions.

Size composed native fixtures for retained owners, not only scheduled workers. The controlled
parent/child overlap used eight permits, but runtime readiness and the two execution connections
each retain a driver/ingester pair. Those six permits plus the active child worker leave only the
protected steering permit, so the next parent worker correctly waits. This topology needs at least
nine permits; the fixture uses ten. Source-derived ownership and nonfatal capacity waits establish
that bound, not an uncaptured exact pool-count sample. Preserve production capacity and steering
reserve instead of treating the fixture's circular wait as a scheduler defect.

## Preserve The Draft Checkpoint Across Enclosing Thread Advance

The composed conversation completed archive and navigation but ordinary Exit silently returned
to its open windows. Adding real native frames before and throughout the single Exit reproduced
the failure. A probe before flush failure collapse identified `DurableSelectorChanged`; the outer
preparation reported only `Unsatisfied(Recoverable)`, while its generic failure getter stayed empty.

Generated parent admission preserves the draft, session, root and history but advances its enclosing
thread revision and reverse-index proof. The host retained its older complete durable selector,
so strict selector equality rejected a valid preserved checkpoint. Equal draft identity or text
alone cannot fix this safely. Syndic must authenticate the exact live candidate and unchanged
durable checkpoint in one confirmed read interval, permitting only the enclosing revision advance.
The host can then renew only its selector before ordinary saved qualification or capture; writer
guards and dirty state remain exact. Original publication receipts remain historical evidence,
with successor eligibility qualified separately. Owning draft-storage and catalog/composer design
define the correction; clean, dirty, changed-checkpoint and intervening-mutation proof is required.

Preserve the existing known-checkpoint classification before invoking the new qualifier during
convergence. Qualifying a successor already known to be different replaced `DurableBaseConflict`
with a generic concurrent-read failure in both a new case and the existing competing-session replay
regression. Guard qualification with the existing current-session/checkpoint predicate; known
conflicts keep their original terminal custody, while changes during a qualifying read retain the
original pending outcome without reexecution.

## Managed Fixtures Must Admit Ordinary Request-Only Readers

The full service graph opens request-only model-reader connections with notification opt-outs.
The discussion fixture inherited an execution-only handshake assertion and rejected those readers;
joining their panicked threads could terminate the fixture while another connection owned a turn.
Execution-only protocol tests did not exercise this boundary.

Keep the production reader unchanged. The discussion fixture now admits bounded request-only
configuration, model-list and readonly thread queries, and its direct protocol test exercises them
while an execution turn is active. That passing fixture case does not qualify the complete native
flow: a subsequent run still ended before assistant emission and needs its own terminal evidence.

The original terminal evidence was `CompletionUnknown` with provider `Schema(EnvelopeShape)`,
before activation. The new fixture built lifecycle parameters with thread identity before `item`,
and assistant items with `id` before `type`; both violate the bounded decoder's pinned grammar.
Its tool request and rejection envelopes also used the wrong first fields, and the terminal frame
inserted an unsupported root field. A valid JSON parse of the fixture's socket messages missed all
of these contracts. A stream-loss label and client connection reset did not identify the initiating
failure; capture of the original unknown-start error did.

Serialize pinned fixture envelopes explicitly with JSON-escaped scalar values. Do not depend on
the dependency graph's `serde_json::Value` map-order feature or loosen the decoder to accept a
test fixture. Verify submitted echoes through a real foreground session with its streamed-input
verifier and ordered sink; bare JSON reception remains protocol-control evidence only.

Context-capable native fixtures must also supply an explicit finite model context window through
their original startup request policy. The general ordinary fixture's absent window is valid for
context-free work but cannot prepare the first inherited discussion seed. Session registration
alone does not prove that native projection planning can dispatch. Configure the test model's
bound rather than adding a production fallback or weakening the planning refusal.
