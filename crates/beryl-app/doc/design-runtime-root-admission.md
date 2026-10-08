# Runtime And Root Admission

This supplement owns the application's selected-path admission capability and inherits the rigor
contract in [the package entry point](design.md#engineering-rigor). Product behavior remains in
[conversation threads](../../../doc/features/conversation-threads/design.md#runtime-and-root-configuration),
and cross-domain atomicity remains in
[storage authority](../../../doc/systems/beryl-home-storage/design.md#thread-claims-and-empty-thread-acquisition).
Native picker and shell mounting are separate consumers of this capability.

## Validation Boundary

`RuntimePathValidator` accepts the composition root's typed token directory, bounded limits and
immutable optional WSL supervisor descriptor. It derives the selected executable's exact Host or
WSL environment and validates canonical executable, directory and user-home facts off the GUI and
storage writer. Root validation cannot cross the selected runtime's environment. No guessed home,
alternate distribution, runtime artifact override or installation is permitted.

Runtime validation also requires the explicit standalone-server or CLI launch form selected by the
invoking command. It forwards that form through managed qualification, authenticates it in launch
provenance and persists it with the admitted runtime. A duplicate canonical executable resolves
the existing record and its retained form. Ordinary runtime composition and recovery consume that
persisted form; no filename inference or alternate-form retry is permitted.

Production WSL observations use the backend's fixed typed observation capability. Runtime
qualification uses the exact managed foreground launch and its existing release-admission
capability under [backend authority](../../../doc/systems/backend-runtime/design.md#runtime-ownership).
Caller-supplied successful reports cannot mint production release proof. The descriptor is forwarded
to both WSL observations and managed qualification; its absence makes WSL validation unavailable.

Validation owns its original worker, result channel, managed process/session/token and any nested
cleanup error until their disposal is joined. Cancellation and expiry retain that same owner,
including a late result that contains further cleanup custody. A failed cleanup is a retained
failure, not completed validation. Errors preserve the original cause and bounded later failure
information. No registry publication or successor validation can bypass unsettled cleanup.

## Command And Outcome Boundary

`RuntimeAdmissionService` composes admitted facts with one exact home generation, State/Syndic
services and the existing process window registry. Its source binds the invoking window and
coherent session facts. It authenticates that source and holds the ordinary selection lease through
validation, command outcome classification and publication. Cancellation or a duplicate request
does not create another concurrent operation or replace the held source.

Canonical duplicates resolve to their existing runtime/root. New runtime admission joins its
non-removable home root in the same revision-checked command. For the first runtime, that command
also creates the Syndic thread and empty draft, catalog row, exact invoking threadless-window claim,
selected window/session facts and fallback. State's future catalog facts accompany their original
creation/replacement contributions; they are not persisted-source proof. Later runtime/root
additions preserve the existing window selection.

Typed outcomes distinguish exact commit, proven noncommit, pending reconciliation and terminal
Unavailable. Commit retains the exact resulting facts and original publication lease. Ambiguity
retains the original reconciliation handle and full intent; Collision remains terminal and retains
its operation/publication fence. No outcome authorizes resubmission or publishing a partial registry,
catalog or selected-window result.

Service retirement cancels and joins its original operation, preserves pending cleanup and retires
publication authority. Returned errors and the service share the same cleanup owner, so consuming
or retrying either path cannot create replacement work over incomplete disposal. The future shell
consumer must honor this retirement and outcome contract when mounting the capability.

## First Conversation Preparation Recovery

First-runtime mounting prepares the selected conversation under the committed onboarding window
and claim and retains the original admission publication fence. It preserves the existing native
window reservation. A selected predecessor or a Restoring claim is not required for that first
conversation; construction supplies no replacement window or new claim.

If Home fails before first-conversation publication, the app captures move-only compact cleanup
intent for its original unpublished fresh editor. This includes the original opening request,
disposal operation, exact known outcomes and pending reconciliation handles even when opening
committed before its head could be classified. The retained intent is bound to the same durable
home identity and contains no old Home service reference, Syndic handle, composer host/service,
widget, native reservation, worker or admission publication lease. All old runtime resources are
fenced and joined before reporting the graph retired. Intent transfer neither proves editor
disposal nor grants publication authority.

The existing same-home recovery attempt retains that intent outside the retired graph. Fresh
Syndic handles and borrowed private recovery access authenticate and settle the exact opening and
fresh-session abandonment under the storage contract. No successor editor or graph is published
until cleanup is proven. Failed candidate work preserves original intent and outcome custody for
the established recovery path; cancellation and terminal close preserve ordinary conservative
storage custody. No runtime/root registration, opening or abandonment is guessed or resubmitted
with new identities. Successful cleanup does not undo the committed onboarding thread or registry.

## Committed First Conversation Recovery

The process recovery owner distinguishes an unpublished first conversation from both an empty
initial window and a published selected resident. Before retiring the failed graph it captures the
original admission identity, immutable resulting window/thread/draft/Active-claim facts, actual
admission outcome and any unpublished editor cleanup custody. This capture also applies before an
editor preparation owner exists. Pending admission settles through its original reconciliation;
only exact committed onboarding admits selected reconstruction. Proven noncommit retains the
original empty-window route; terminal uncertainty retains unavailable custody without replay.

Ordinary failure capture and preserved-window validation consume this typed onboarding custody
instead of deriving its selection from a nonexistent resident. The original window reservation and
recovery fence stay with the surviving shell. Old admission leases and services are retired and
release their resources before reopening; retained facts identify recovery work but grant no fresh
publication authority. The process owner retains at most one unpublished first-conversation owner
per surviving window within the existing bounded window set.

Fresh private recovery access authenticates the same home, exact selected window, remembered target,
fallback, catalog/thread/draft and paired Active claim under coherent revision checks. No State
rollback, replacement claim, Restoring transition or predecessor editor is manufactured. Complete
original editor cleanup precedes a new opening. Each fresh opening is one immutable attempt-owned
request, with its own disposal identity and original outcome/reconciliation custody. It is not a
replay of a previous uncertain opening. Repeated recovery may prepare another editor only after the
previous attempt's opening and disposal settle exactly.

Candidate-only construction prepares the new editor session, host, bounded initial text/markers,
slot and service without admitting ordinary healthy-home work. Worker qualification supplies GUI
preparation with immutable coherent facts; storage reads and outcome settlement stay off the GUI.
The new editor remains recovery-fenced while its composer, transcript, appearance, notices,
subscriptions and close custody are prepared and attached to the same native reservation through
an explicit attachment that needs no selected predecessor. All fallible preparation and complete
State/shell binding revalidation precede atomic whole-graph publication and interaction release.

Cancellation, stale delivery, partial attachment and candidate failure dispose and join every fresh
runtime resource while retaining actual opening/disposal evidence outside the disposed graph.
Incomplete cleanup or collision blocks a successor and keeps the original recovery request
unavailable. Close/Exit remains serialized by the process recovery fence; publication cancellation
or closing the newly published graph follows the existing supervisor rule. Successful recovery
preserves the admitted runtime, root, thread, draft, claim, native identity and placement.

Qualify the complete ordinary failure consumer from committed onboarding through retirement, private
candidate preparation, preserved-shell attachment, graph publication and interaction reopening.
Cover failure before editor construction, opening commit before classification, preparation and
partial attachment failure, exact noncommit/commit/ambiguity/collision, cancellation, repeated
failure, stale delivery and close/Exit races. Independently review exact outcome/resource custody
and production consumer correspondence; isolated opening wrappers do not establish this boundary.
