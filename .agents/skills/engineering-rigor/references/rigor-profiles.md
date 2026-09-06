# Rigor Profile Catalog

This catalog is normative for `# Engineering Rigor` declarations. Profile and modifier identifiers
are stable contracts. Never change an existing identifier incompatibly; add a new version instead.
The `/v1` entries below retain their original requirements, including independent-review rules.
Use a `/v2` entry only after explicit adoption in design authority. General guidance and examples
do not relax a selected version. Versions refine contracts within the same five profile grades;
they do not add grades or define a numeric score.

## Contents

- Verification semantics
- Profiles
- Modifiers
- Selection rules
- Worked examples

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

Verification can be strong while runtime behavior stays simple. A focused failure test may establish
that an ordinary error leaves the protected state intact; it does not create a requirement for
automatic recovery. Credit permitted invocation failure, manual retry, or rebuild as acceptable
outcomes where the selected contract allows them. Choose independent review from that contract
and the consequences of mistakes, not merely from the absence of a strong verifier.

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

### `personal-utility/v2`

Use for maintained local tools operated by one person or a very small trusted technical audience.

- Assume cooperative operators and inputs within the tool's ordinary documented purpose.
- Require core workflows to work and common operator mistakes to fail understandably.
- Permit occasional failed invocations, ordinary errors, manual retry, or rebuild within the
  supported envelope when they preserve declared protected outcomes; do not permit a broken core
  workflow. Out-of-envelope invocation failure is also acceptable.
- Use smoke verification or focused automated tests for core behavior and the relevant failure
  boundary. Objective evidence plus author self-review normally suffice; weak verification alone
  does not require independent review for low-consequence work.
- Default to no automatic retry, journal, redundant validation, recovery framework, or exact
  resource accounting. Add machinery or independent review only for a concrete protected outcome
  or an explicit requirement from a modifier or other authority.

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

### `trusted-internal-tool/v2`

Use for maintained team tools, dashboards, automation, and internal services used by trusted,
technically informed operators.

- Assume cooperative operators rather than hostile users; support expected workflows, likely
  mistakes, and inputs within ordinary declared resource bounds.
- Require failures to be diagnosable enough for the team to recover or retry. Permit an isolated
  failed invocation, manual retry, or rebuild within the supported envelope when declared protected
  outcomes remain intact; core workflows must still work.
- Permit failure for unsupported or grossly out-of-envelope inputs. Do not require arbitrary scale,
  adversarial resource-exhaustion resistance, or elaborate recovery without a concrete consequence
  that the applicable contract protects against.
- Verify core workflows and relevant likely failures with focused automated or repeatable checks.
  Objective evidence plus author self-review suffice for low-consequence work; weak verification
  alone does not require independent review. Require focused independent review when an explicit
  modifier, consequential boundary, or other authority requires it.
- Default to ordinary errors and manual retry or rebuild. Automatic retry, journals, redundant
  validation, generalized recovery frameworks, and exact resource accounting need a concrete
  protected outcome. Do not require general adversarial review.

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

### `production-application/v2`

Use for customer-facing or broadly deployed applications and services with meaningful security,
availability, compatibility, or recovery expectations.

- Treat externally controlled boundary inputs as untrusted.
- Require safe failure within documented operating limits and protect the shared state and
  resources whose loss would violate declared outcomes. A failed request or manual retry is
  acceptable when the contract permits it and consequential state and effects remain protected.
- Address important abuse cases, authorization boundaries, persistence failure, concurrency,
  migrations, rollback, observability, and compatibility when applicable to protected outcomes.
- Verify documented behavior, important edge cases, and recovery paths proportionate to their
  consequences. Strong verification does not itself require extra runtime machinery.
- Require independent semantic review for changes that materially affect consequential behavior or
  boundaries. Mechanical changes may use objective evidence plus author self-review when that
  evidence establishes preservation of the affected guarantees and no modifier or other authority
  requires independent review. Weak verification alone does not require independent review for a
  low-consequence change.
- Use adversarial review when a security, persistence, concurrency, lifecycle, resource, or other
  boundary is both high-consequence and weakly verifiable. Choose the simplest mechanisms that
  satisfy these guarantees.

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

### `persistent-state-integrity/v2`

Protect previously valid authoritative or published persisted state whose loss, corruption, or
incorrect use would violate a declared outcome. Disposable derived state may be discarded and
rebuilt when rebuild cost and downstream effects are acceptable; do not infer that permission for
authoritative state. Verify the relevant failure boundary. Persistence alone does not require a
journal, recovery framework, continuous semantic proof, or validation of unrelated state. Permit
ordinary errors and manual rebuild where they preserve the protected outcome.

### `untrusted-input/v1`

Treat boundary input as potentially malformed or hostile. Validate it before unsafe use and verify
representative abuse and malformed-input cases.

### `shared-resource-protection/v1`

Prevent one invocation or user from exhausting resources needed by other users or processes.
Verify the enforced bound or isolation mechanism.

### `shared-resource-protection/v2`

Protect other users' or processes' required resources against interference that would violate a
declared outcome. Name the protected resource and affected workload in the owning authority.
Mere use of a shared machine does not require exact accounting, quotas, or
adversarial exhaustion resistance. Use the simplest adequate bound or isolation and verify it at
the consequential boundary. Ordinary invocation failure or temporary contention is acceptable when
the supported envelope and other workloads' required outcomes permit it.

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

### `external-side-effects/v2`

Make externally visible effects deliberate and diagnosable. Protect against unintended repetition
or uncertain outcomes when their consequences would violate the contract. For harmless,
reversible, or acceptably repeatable effects, an ordinary error and informed manual retry may
suffice. Require acknowledgement handling, operation identity, reconciliation, or automatic retry
only as needed for the actual effect and integration guarantees. Verify the consequential effect
boundary; do not infer exhaustive target validation or a general effect journal.

### `irreversible-operation/v1`

Make irreversible or difficult-to-recover operations explicit and guarded, with recovery or prior
confirmation appropriate to the consequence. Require independent adversarial review.

### `irreversible-operation/v2`

Identify irreversible or difficult-to-recover effects and guard them proportionately to their actual
loss, rebuild cost, and downstream consequence. Discarding disposable outputs with an acceptable
rebuild cost needs no special confirmation or independent review merely because exact bytes cannot
be restored. Require independent adversarial review for irreversible boundaries that are both
high-consequence and weakly verifiable. Strong evidence does not itself require adversarial review;
lower-consequence operations may use focused evidence and author self-review unless another
authority requires more. Preserve explicit review obligations and existing authorization gates.
Use recovery or prior confirmation when the consequence requires it.

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

State the outcome that must survive failure and the failure that may be tolerated. Prefer the
simplest implementation that meets both. Apply parent guarantees only where the change can affect
their protected outcomes; a development tool does not inherit a production classification merely
because it supports a production product. Do not add a modifier solely because code writes a file,
uses a shared machine, calls an external service, or discards generated output. Consider state
value, acceptable rebuild or repetition, interference, and actual downstream effects.

For manually selected files in a trusted internal tool, grossly out-of-envelope input such as a
multi-gigabyte file need not be handled gracefully unless its shared-resource effects violate a
protected outcome or another applicable declaration requires that behavior. A cheap usability guard
may still be implemented, but it is not a blocking requirement by default.

## Worked Examples

These examples use explicitly adopted `/v2` contracts; they do not reinterpret `/v1` declarations.

### Personal report generator

Select `personal-utility/v2`, with no modifier when reports are disposable and inputs stay intact.
An occasional filesystem error or interruption may fail the invocation; the operator can rerun it
and regenerate the report. Minimum verification is a core report smoke check and a focused check
that the relevant write failure reports an error without damaging inputs. Exclude automatic retry,
an operation journal, redundant full-input validation, and a recovery framework unless another
declared outcome needs them.

### Team development cache builder

Select `trusted-internal-tool/v2`. Add `shared-resource-protection/v2` only if interference can
violate another workload's required outcome, and protect that boundary with an adequate simple
bound. A build may fail and its disposable cache may need deletion and manual rebuild; consumers
must not treat an incomplete cache as a valid result. Minimum verification covers a normal build,
the relevant interrupted-publication boundary, and any required resource bound. Exclude a durable
recovery journal, automatic retries, exact resource accounting, and production classification based
solely on the product it helps build. If rebuild cost or downstream effects become consequential,
revisit the declaration.

### Mechanical production refactor

Under an explicitly adopted `production-application/v2`, a rename or extraction that preserves
behavior may use focused objective evidence plus author self-review when no modifier or other
authority requires independent review. Existing permitted request failures and manual retry remain
tolerated; new data loss, duplicate consequential effects, or weakened availability are not.
Minimum verification is the relevant behavior check plus comparison of the affected call paths and
failure propagation. Exclude new runtime validation, retry machinery, or independent review
solely because the file belongs to a production application. A semantic change to a
consequential boundary falls outside this example and requires the profile's independent review.
