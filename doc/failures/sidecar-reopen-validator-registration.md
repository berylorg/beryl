# Sidecar Reopen Validation Was Recovery-Only

## Scope

`beryl-home-store` typed-domain registration and sidecar-owning domain startup validation.

## Invalidated Approach

An existing persisted domain ran only `StorageDomain::validate` when a new process registered it. The sidecar-aware `validate_reopen` hook ran during health verification and forced same-home recovery, but not during ordinary startup registration.

## Evidence

The Phase 7 asset proof `missing_referenced_sidecar_rejects_domain_reopen` removed a committed referenced sidecar, reopened the home, and observed successful asset-domain registration. Record and index validation passed because the missing physical file was visible only to `SidecarVerifier`.

## Why It Failed

Ordinary registration of an existing persisted domain is itself a reopen boundary. Restricting physical-reference validation to failure recovery allowed normal startup to publish a healthy typed handle whose durable metadata named missing bytes.

## Course Correction

`HomeStore::register_domain` now invokes the sidecar-aware reopen validator before publishing a handle whenever the domain already exists. Fresh empty registration keeps the ordinary creation path. Forced recovery continues to run the same reopen contract across every reacquired domain.

The home-store regression test `existing_domain_registration_runs_its_sidecar_reopen_validator` and the asset missing-sidecar test both prove the corrected startup path. The package authority in `crates/beryl-home-store/doc/design.md`, Phase 7 result in `doc/plan.md`, and Checkpoint 2 tracker now record the behavior.

## Durable Lesson

Any validator whose evidence is required to trust persisted state must run on ordinary process startup as well as explicit recovery. Naming a hook “reopen” is not sufficient unless every path that republishes existing durable authority actually invokes it.
