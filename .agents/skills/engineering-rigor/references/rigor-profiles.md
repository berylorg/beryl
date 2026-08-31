# Rigor Profile Catalog

This catalog is normative for `# Engineering Rigor` declarations. Profile and modifier identifiers
are stable contracts. Never change an existing identifier incompatibly; add a new version instead.

## Contents

- Verification semantics
- Profiles
- Modifiers
- Selection rules

## Verification Semantics

An unqualified `verify` requires proportionate objective acceptance evidence; it does not by itself
require runtime, per-operation, pre-success, or continuous checks. Evidence may include focused
tests, fault injection, static analysis, exact documented dependency guarantees, repeatable
qualification, review, or runtime checks. Require runtime enforcement or detection only when needed
to uphold a stated supported-envelope guarantee. Do not infer exhaustive scans, duplicated pre- and
post-operation checks, or revalidation of unrelated state.

Credit exact guarantees of the selected dependency version, configuration, and operating mode.
Require correct application use of those guarantees and evidence for application-owned invariants,
but do not re-prove dependency internals at runtime without a concrete uncovered consequence.

## Profiles

### `exploratory-prototype/v1`

Use for disposable experiments, spikes, demonstrations, and feasibility work whose result is
learning rather than a maintained capability.

- Assume trusted, cooperative operators and deliberately selected inputs.
- Support only the stated demonstration or investigation path.
- Permit invocation failure, rough diagnostics, and disposal of generated state.
- Verify enough to demonstrate the result or answer the investigation question.
- Do not require compatibility, broad edge-case handling, operational hardening, or independent
  review unless a modifier or other authority requires it.

### `personal-utility/v1`

Use for maintained local tools operated by one person or a very small trusted technical audience.

- Assume cooperative operators and inputs within the tool's ordinary documented purpose.
- Require core workflows to work and common operator mistakes to fail understandably.
- Permit failure of the current invocation outside the supported operating envelope.
- Use smoke verification or focused automated tests for the core behavior.
- Let objective verification and author self-review suffice unless a modifier, weak verifier, or
  other authority requires independent review.

### `trusted-internal-tool/v1`

Use for maintained team tools, dashboards, automation, and internal services used by trusted,
technically informed operators.

- Assume cooperative operators rather than hostile users.
- Support expected workflows, likely mistakes, and inputs within ordinary declared resource bounds.
- Require failures within that envelope to be diagnosable enough for the team to recover or retry.
- Permit failure of an isolated invocation for unsupported or grossly out-of-envelope inputs.
- Do not require adversarial resource-exhaustion resistance, arbitrary input scale, or elaborate
  recovery outside the supported envelope.
- Verify core workflows and likely failure paths with focused automated or repeatable checks.
- Require focused independent review only when a modifier explicitly requires it, or when a
  consequence, weak objective verification, or another authority justifies it; do not require
  general adversarial review.

### `production-application/v1`

Use for customer-facing or broadly deployed applications and services with meaningful security,
availability, compatibility, or recovery expectations.

- Treat externally controlled boundary inputs as untrusted.
- Require safe failure within documented operating limits and protection of shared state and
  resources.
- Address important abuse cases, authorization boundaries, persistence failure, concurrency,
  migrations, rollback, observability, and compatibility when applicable.
- Verify documented behavior, important edge cases, and recovery paths proportionate to their
  consequences.
- Require independent semantic review. Use adversarial review when a security, persistence,
  concurrency, lifecycle, resource, or other boundary is both high-consequence and weakly
  verifiable.

### `critical-system/v1`

Use for safety-sensitive, regulated, financial, privacy-critical, infrastructure-critical, or
difficult-to-recover systems where failure can cause severe or irreversible harm.

- Require explicit trust boundaries, failure analysis, and supported operating limits.
- Defend critical invariants against hostile inputs, resource exhaustion, partial failure, and
  operator error as applicable.
- Require auditable verification evidence and exercised recovery or rollback paths.
- Require independent adversarial review of high-consequence and weakly verifiable boundaries.
- Require explicit Operator approval for irreversible actions when project instructions do not
  already impose a stronger gate.

## Modifiers

Apply modifiers when a scope needs a guarantee that its base profile does not already require.
Modifiers strengthen or specialize a profile; they never erase its requirements.

### `persistent-state-integrity/v1`

Prevent supported failures and unsupported operations from corrupting previously valid affected or
published persisted state. Verify the relevant failure boundary. This does not require validating
all persisted, derived, or unrelated state before and after every operation.

### `untrusted-input/v1`

Treat boundary input as potentially malformed or hostile. Validate it before unsafe use and verify
representative abuse and malformed-input cases.

### `shared-resource-protection/v1`

Prevent one invocation or user from exhausting resources needed by other users or processes.
Verify the enforced bound or isolation mechanism.

### `sensitive-data/v1`

Protect confidential, personal, regulated, or secret data throughout storage, processing,
diagnostics, and disposal. Review the affected data boundary independently.

### `privileged-access/v1`

Protect credentials, authorization decisions, or elevated capabilities against misuse and
unintended disclosure. Require independent review of the privilege boundary.

### `external-side-effects/v1`

Make externally visible writes, messages, charges, deployments, or remote mutations deliberate,
diagnosable, and safe against unintended repetition as the integration permits. At the escape
transition, account for acknowledgement, operation identity, and reconciliation needed for retries
or outcome uncertainty. Verify that effect boundary; do not infer exhaustive target validation.

### `irreversible-operation/v1`

Make irreversible or difficult-to-recover operations explicit and guarded, with recovery or prior
confirmation appropriate to the consequence. Require independent adversarial review.

### `availability-required/v1`

Define and protect the required continuity of service. Verify degradation, recovery, and resource
behavior relevant to that continuity requirement.

### `migration-recovery/v1`

Preserve valid state across migration and provide a verified rollback, retry, or forward-recovery
path appropriate to the migration boundary.

## Selection Rules

Select a profile from actual exposure and consequence, not labels such as "internal" or
"production" alone. Consider operator trust, input trust, supported operating envelope, blast
radius, state authority, state value, rebuildability, rebuild cost, downstream consequence,
reversibility, shared resources, privacy, security, and availability. Persisting derived state does
not by itself require continuous semantic proof.

Choose the least costly profile that honestly covers the scope, then add only necessary modifiers.
Do not select a stronger profile to avoid writing one precise modifier. Do not select a weaker
profile when several modifiers would merely reconstruct a stronger profile.

For manually selected files in a trusted internal tool, grossly out-of-envelope input such as a
multi-gigabyte file need not be handled gracefully unless it can affect shared resources or another
applicable declaration requires that behavior. A cheap usability guard may still be implemented,
but it is not a blocking requirement by default.
