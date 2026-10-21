# Command Pair Qualification

HomeStore's required command pairs are accepted under its production-application/v2 and
persistent-state-integrity/v2 contract. Independent architecture-readiness and complete
implementation review found no blocking findings. Generated-media consumer integration remains
the next acceptance boundary.

## Qualified Boundary

Domain-owned mutation and validation callbacks seal one fixed requirement into their opaque plans.
All command and candidate entry points preserve it. Reciprocal protocol, operation, digest, domain
and actual participant-kind checks run before reservation callbacks and after writer-time fences,
before preparation. Missing or mismatched peers cannot write. Ordinary commands, read-only proofs,
receipt identity, local admission, persistence classification and original reconciliation custody
retain their established behavior.

The eight changed source files and three new test files under `crates/beryl-home-store` were
byte-identical between the working checkout and the isolated canonical build checkout at acceptance.
No manifest or dependency revision changed. Verification used the existing shared target, locked
dependencies, one build job and one test thread, without the development checkout's local override.
The pre-existing generated-media State/Syndic drafts were excluded from these qualified inputs.

## Verification

- Focused `command_pairs` with `test-faults`: 15/15 passed in 2.663 seconds;
  run `d4270319-fbac-4f85-a923-f0e9a92e825e`, log SHA-256
  `EF0E89337E2C4B85BF6E3D87A8787B0BEDE923A9948990889547C875D43F1FD2`.
- Typed/current commands, mutation observations, proof identity/composition, validation,
  concurrency, unchanged-record reconciliation and physical-writer faults: 84 distinct cases
  qualified. Run `017064c8-2208-4d04-9a36-c637e63c7e9d` passed 48 cases before a fixture configuration
  failure; log SHA-256 `7DB0FF7A7599539BA94C128EA04837E42D6A7F556A38FDA04EB5529FE6554EC5`.
  Run `3eeef415-7e63-40af-9879-9b5c70fe5de3` passed the remaining 36 in 2.979 seconds;
  log SHA-256 `FFE9E62B39E7448AA29D932F9B897B4E6C49E9D27ADB6A1456D96608403F3E3C`.
- Normal `cargo check --locked -p beryl-home-store` passed. Normal production
  `cargo check --locked -p beryl --bin beryl` passed in 25.89 seconds using the previously qualified
  V2 supervisor artifact; log SHA-256
  `F148AC2CA1079EF48B95B0289213C83449985878A67926F35AF3EBDCAEB44642`.
  Existing warnings remain.

The initial focused test build exposed a moved-fixture borrow in a new candidate test; the test
was corrected before qualification. Its recovery assertions also enforce original receipt
generation and stale-receipt rejection. The interrupted regression run omitted the existing
`BERYL_UNCHANGED_FIXTURE_LEDGER` input; the affected and unrun targets passed after supplying an
exact task-owned ledger. No production correction or weakened expectation was needed.

## Resource Closure

No Beryl GUI was launched. Test temporary roots and all fifteen ledger-recorded fixture homes were
contained within the exact owned `.aipm/tasks/command-pairs/` scope. After acceptance, that scope's
logs, fixtures and intermediates are reclaimed; this audit retains the relevant outcomes and log
identities. Shared build artifacts and the existing canonical checkout remain available for the
next phase.
