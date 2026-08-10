# Scope

Phase 96 accepted-next scheduler fixture ownership during same-home recovery testing.

# Invalidated Approach

Injecting a Phase 58 synthetic accepted-next topology through `FixtureBatch`, either after startup
handback or before service construction, and treating it as equivalent to a real owner-published
accepted input with canonical completed history.

# Evidence

The focused `test-faults` proof first timed out with provider issues `0` and an untouched command
ring. Moving the synthetic topology before service construction allowed promotion, but native
planning then reported missing CAS turn correlation and recovery preparation rejected the
zero-item parent as incomplete history. The scheduled worker became fatal before session dispatch.

# Why It Failed

The synthetic records bypassed ordinary `execute_accepted_input_admission` ownership and did not
contain the canonical items, valid CAS binding, or provider correlation required by native and
recovery planning. A later correction also used the wrong lifecycle order and expected
`NextTurn(PendingTurn)`: Phase 62 admits the next input while the parent gate is
`AwaitingTerminal`, durably selecting `NextTurn(UnknownTerminal)`. Completing terminal history
returns the gate to `Idle` but does not rewrite that accepted-route reason.
Connection admission alone also does not create a loaded-thread registry token: recovery audit is
intentionally scoped to loaded projections, so a fixture that asserts token preservation must own
an actual loaded projection rather than infer one from connection registration.

# Course Correction

Build a canonical parent with at least one real item, establish its native CAS binding, activate the
turn, and correlate the canonical user item. After the real connection session is available and the
stable driver reaches its pre-cycle pause, record the parent's unknown terminal, publish the next
input through `execute_accepted_input_admission` while the gate is `AwaitingTerminal`, and then
complete and finalize the parent. Before scheduler promotion, require the admitted leaf to retain
`NextTurn(UnknownTerminal)` and the released gate to be `Idle`. Continue to prove exact scheduler
and worker-pool ownership, one successful issuance, queued-command rejection, quiescence, and
closed-gate adoption. Hold one distinct loaded projection anchor through the pre-cut registry audit,
then drop it only after persistent-failure notification joins so its candidate ownership surrenders
into the cut while the registry token remains available for equality checks after adoption.

# Affected Work

- `doc/plan.md` Phase 96.
- `crates/beryl-app/tests/unit/service_epoch_adoption/command_frontier.rs`.

The production scheduler and stable-driver architecture are unchanged. Remaining verification is
the corrected focused proof and the ordinary Phase 96 functional gates.
