# Managed Backend Probe Ownership

## Compound launch and probe

The Phase 4 live-retention harness initially used `ManagedBackendServer::launch_and_probe_with_options`. That call creates the managed process and authentication material internally, connects, initializes, probes compatibility, and returns the server handle only after every step succeeds.

This compound ownership boundary cannot satisfy the live proof's cleanup contract. If connection, initialize, or compatibility probing fails after launch, the caller never receives `ManagedBackendServer` and cannot explicitly shut it down, reap it, verify managed-auth cleanup, or report exact residue. The server's `Drop` implementation attempts cleanup but reduces failure to tracing warnings, which a proof harness cannot treat as verified absence.

The harness must not accept warning-only `Drop`, inspect backend storage or ambiguous global token files, bypass compatibility probing, or use raw protocol requests. Operator approved a separately planned public backend seam that retains caller ownership of the launched server while the same connection and compatibility probe runs. `ManagedBackendServer::connect_and_probe` and its progress-aware form now provide that boundary, while compound launch-and-probe delegates to the same probe implementation. Explicit shutdown also preserves simultaneous process-supervision and auth-cleanup failures in one typed error.

Affected authority and evidence:

- `doc/plan.md`, Phase 4.
- `crates/beryl-backend/src/server.rs`, managed launch/probe and `Drop` ownership.
- `crates/beryl-backend/tests/live_dynamic_tool_fork.rs`.
- Repeat Phase 4 Deep completion review.
