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
