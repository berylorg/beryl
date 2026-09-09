# Runtime Admission Contradiction

## Scope

Managed execution-session admission in `beryl-app`, implemented under Phase 346.

## Invalidated Approach And Evidence

The initial handoff treated every additional-session admission error as local to that candidate.
Its real managed fixture returned a successful authenticated `config/read` with a required flag
false, yet the test expected the existing runtime activity period to remain usable and a later
session to succeed against the same process.

Independent review identified the conflict with
[backend runtime ownership](../systems/backend-runtime/design.md#launch-and-listener-security):
configuration mismatch makes the runtime unavailable. The strict config decoder rejects false or
unproven required settings as `ForegroundIngress::MalformedResponse` during `config/read`, before
`ReleaseAdmissionEffectiveConfigUnproven` can be returned. Matching only the latter leaves the same
bug. Neither typed configuration-admission failure is a transport or capacity failure; earlier
successful admission cannot override it.

## Correction

End the exact matching runtime activity period, remove its private connector and wake the existing
ordered retirement owner for either exact configuration-admission failure. Preserve that unavailable
outcome through disposal. Existing interest cannot revive the period; another usable period
requires a fresh managed process after exact retirement. Keep capacity and transport failures
local to the failed candidate.

The `runtime_execution_sessions` integration target verifies immediate invalidation, refusal of
same-process re-admission, joined resource release and a distinct replacement process/period.
This is an implementation correction under existing authority; no target-state change is needed.
