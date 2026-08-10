# CAS Phase 87 Postcommit Dispatch Fixture Diagnosis

## Scope

Diagnosis of an accepted-input fixture that appeared to reconcile an ambiguous postcommit
promotion through the running-session supervisor but then omitted the required ordinary
`turn/start`.

## Invalidated Approaches

The obsolete direct-service fixture waited in `verifying` because no process supervisor owned
verification. Direct `verify_health`, polling, or fabricated managed-launch provenance would
create alternate authority and remain invalid.

A raw accepted-input fixture was also augmented with binding and CAS-thread indexes. Domain
validation passed, but those synthetic records did not prove the exact production-created terminal
source and CAS-turn provenance. Raw structural validity is not a substitute for native projection
authority and was removed from the diagnosis.

An attempted typed physical-fault scope was also invalid. Accepted-input promotion executes as a
heterogeneous `HomeCommand`, while typed `FaultScope` applies only to one
`CurrentDomainCommand`. No fictitious promotion scope was retained; the fixture instead uses the
existing reservation and post-command reconciliation barriers to hold the exact worker across
supervisor verification.

## Decisive Evidence

The production-created completed-history fixture retained the exact terminal source, CAS-turn
correlation, usable binding, represented prefix, execution binding, lineage, and native position.
It reconciled the route to `Promoted` and issued `thread/resume`.

A bounded typed diagnostic at the accepted-next execution boundary captured the first failure as
`ProjectionEstablishmentFailure(Backend(ForegroundIngress(MalformedResponse)))`. The shared
`NormalTerminalServer` resumed-thread response provided `initialTurnsPage` but omitted the
current 0.146 response fields `turnsBackwardsCursor` and `itemsBackwardsCursor`. The backend
response machine correctly rejected that malformed result before valid-binding publication,
ordinary preflight, or activation.

The later missing `turn/start`, server socket close, and `SchedulerShutdown` were downstream
effects of the malformed fixture response. They were not evidence of a production settlement or
scheduler-ownership defect.

## Course Correction

Keep the production-created supervisor fixture and exact single-dispatch assertion. The shared
test server now emits `initialTurnsPage`, `turnsBackwardsCursor`, and
`itemsBackwardsCursor` in the current ordered response shape. The exact postcommit scenario then
passes and observes one ordinary dispatch.

The temporary typed diagnostic enums, capture storage, classifiers, and test reporter were removed
after the cause was proven. No production scheduler, settlement, projection, or backend behavior
was changed.

The lifecycle candidate admission helper is exposed only as doc-hidden `test-faults` support. The
production admission path continues to require exact managed-launch provenance.

## Durable Lesson

When a protocol-backed fixture observes a downstream command omission, capture the earliest typed
backend failure before attributing the symptom to scheduler settlement. Shared synthetic response
builders must evolve with the exact CAS response schema, including nullable cursor fields whose
absence is semantically different from an explicit `null`.
