# Model Selection Provider Fixtures

## Scope

Model-reader and mounted-menu qualification for the pinned CAS response contract.

## Invalidated Approach

The loopback fixture used a nonblocking listener while its accepted-connection handler assumed
blocking reads. On Windows the accepted socket inherited nonblocking mode, and the handler treated
WouldBlock as connection closure before receiving initialization.

The fixture also constructed initialized/configuration/model responses as generic JSON values
and reserialized their payloads. This changed required response-family field order. Correcting
payload order alone could not repair the earlier accepted-socket defect.

## Evidence

The initial partial App run passed four real ordinary-dispatch cases and five other reader/runtime
cases before the first genuine source fixture timed out on initialization. The subsequent combined
run passed twelve cases and failed all fourteen cases using that fixture at the same admission
boundary. Isolated run `d1b8f1b6-8338-4a2e-8380-da491d28aada` then observed one connection, no
requests or initialization responses, and socket error 10035. This proves the accepted connection
closed before any protocol exchange. [Microsoft's accept contract](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-accept)
confirms inherited socket properties.

The backend's existing `initialize_rejects_reordered_or_out_of_domain_required_facts_after_consumption`
case and ordered provider fixture separately establish the required payload-order correction.
Neither defect establishes a production runtime or idle-gate failure.

## Course Correction

The fixture resets each accepted stream to blocking mode before handshake and bounded timeouts.
It also writes bounded literal response envelopes and payloads in the pinned family order.
Initialization preserves `userAgent`, `codexHome`, `platformFamily`, then `platformOs`; model
records preserve the seven required fields and pages preserve `data` before `nextCursor`.
Configuration retains the required `model_reasoning_effort` slot as null when its effective value
is unknown; omitting the slot fails the compatibility proof. Unknown effort values remain valid
model-menu qualification inputs. No parser loosening, timeout increase,
automatic request retry or alternate production route was introduced.

## Verification

Corrected source/menu qualification passes all 45 current boundary cases, including eight native
scenarios, in run `78209c17-99ce-4c42-98b2-400b005f60eb`. Retained diagnostics are under
`.tmp/model-selection-evidence/source-wire-probe.log`; accepted final evidence belongs
in the [mount qualification](../audits/model-selection-mount-qualification.md).

## Runtime Attempt Fixture Custody

Incrementing a controller's attempt field directly made an old page ineligible, but invalidated
the original worker's cleanup publication identity. Combined run
`ec12a2bc-6ad3-47b2-8179-8ff32e30a34a` therefore passed the stale-page assertions and failed
service closure. Test attempt mutation cannot substitute for an owned runtime transition.
Qualification now retires readiness through normal controller cleanup, then uses the exact
failure snapshot and original launch specification in production retry acquisition. It admits
and publishes a real successor session, positively elects its defaults, and rejects the original
query and page. The current combined run passes that genuine successor-readiness qualification.

## Native Model Page Frames

Native diagnostic run `780ae52a-6841-4446-8392-41df5c6b398d` reaches successful query
revalidation before overflowing inside the page read, before any page arrives at the menu.
The backend's fixed 64-record array was inline in `ModelPage`, making construction materialize
a large native worker frame even though the enclosing response already boxed the page.
Its private backing now holds exactly 64 slots on the heap; construction uses a bounded
allocation without materializing the complete array on the stack. Public operations, parser
validation, record count and production stack budgets are preserved. All eight focused backend
cases pass in run `9b0034e8-0906-4cea-a1a5-ad337dcbc8c8`. Combined native run
`977a9e09-1f56-4616-94c5-05eaa0befe98` confirms page construction and progressive interactions
without a stack overflow. Its native aborts follow explicit assertion panics at fixture setup or
Exit; the Windows abort status alone does not identify a stack overflow.

## Completed History And Exit Fixture Admission

The generic exact-CAS fixture originally activated history with its own fixed test tool profile.
App recovery correctly rejects that completed binding against the installed canonical conversation
tool profile. Seed the explicit canonical profile before initial activation instead of rewriting
a completed binding or weakening recovery classification. The genuine ordinary-turn metadata case
already uses canonical App admission and passes same-thread resume in the current combined run.

Retiring runtime readiness does not remove the service's retained connection-cleanup custody.
Requesting Exit enters the existing work-confirmation path while the fixture expected immediate
termination. An attempted no-work wait also failed correctly in run
`8855ef51-2a6f-4715-872c-31442c03c56d`: that cleanup is performed by Exit, so it cannot be an
Exit prerequisite. The fixture must activate Exit and use the existing PID/window-qualified
native confirmation helper, allowing production teardown to drain its custody. Retire each exact
Ready entry only once; a second test retirement is an invalid assertion, rather than another
production transition. This run passes all 37 core cases and both passive native cases; the six
Ready native scenarios reach their feature assertions and fail only the invalid no-work oracle.
