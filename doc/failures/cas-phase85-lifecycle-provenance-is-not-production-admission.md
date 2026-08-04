# Scope

Phase 85 compatibility-sequence tests in `beryl-backend` and projection-session fixtures in
`beryl-app`.

# Invalidated Approach

The Phase 31 request-flow fixture called the production compatibility admission method through a
`lifecycle-test-support` connector. That implicitly treated test lifecycle provenance as sufficient
authority for producing a managed-backend compatibility report.

# Decisive Evidence

Once Phase 85 required exact production managed-launch provenance, the complete lifecycle-feature
suite rejected the fixture with `CompatibilityManagedLaunchProvenanceMissing`. The production
launch tests independently proved that a lifecycle connector cannot dispatch compatibility
admission, while the exact probe wire sequence still required direct bounded testing.

After the backend fixture was corrected, the complete app-library gate failed for the same reason:
app unit fixtures pass lifecycle connectors through production
`ProjectionConnectionService::admit`. That method must produce an `AdmittedProjectionSession`
retaining the exact production compatibility report, so the backend non-authorizing probe facts
cannot simply be substituted.

# Why It Failed

Lifecycle-test provenance describes a controlled transport fixture. It does not prove the admitted
binary path, executable identity, loopback listener, launch token, or process lifecycle owned by the
production managed launcher. Allowing it to create a compatibility report would weaken the Phase 85
authority boundary.

# Course Correction

Production compatibility admission remains restricted to genuine production managed-launch
provenance. The post-provenance probe executor has a feature-gated, non-authorizing test seam that
returns only bounded probe facts and cannot create a `ManagedBackendProbeReport` or launch
provenance. Request-flow fixtures use that seam to verify the exact sequence. Do not fabricate
production provenance or relax admission for lifecycle tests.

App fixtures require an explicitly test-only session-admission boundary whose lifecycle evidence
remains structurally distinct from a production compatibility report. That architectural test-
harness correction requires Operator authorization before implementation.
