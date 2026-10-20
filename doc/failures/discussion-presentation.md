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
