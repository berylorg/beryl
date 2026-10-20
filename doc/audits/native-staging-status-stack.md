# Native Staging Status Stack Qualification

## Status

Accepted on 2026-10-20 for the reproduced native candidate-authentication overflow. Complete
Settings and end-to-end recovery qualification remain unaccepted under their owning plan phase.

## Failure And Correction

Settings integration's broader regression run exposed three native lineage scenarios overflowing
a WinRT thread-pool worker during dirty-editor capture. A fresh first-chance debugger trace of
the exact Enter scenario places candidate-session authentication above staging-status decoding.
The worker has approximately 1 MiB of stack; increasing Rust test-thread stacks cannot change it.
No Settings or published-reader frames appear in the captured chain.

The staging-status function reserved approximately 215 KiB while its Building-only branch was
inactive. That branch now runs in a private non-inlined helper borrowing the existing head,
receipt and session. The earlier candidate-session read no longer overlaps those branch locals.
Authentication order, short-circuit conditions, typed failures, receipt construction, supported
limits, storage encoding and public interfaces remain unchanged. Native worker stacks are unchanged.

An existing adoption fixture's exhaustive settlement match now explicitly rejects an unexpected
accepted-input root replacement, preserving its ordinary mutation expectation.

## Evidence

Independent semantic review compared the complete extraction and found no blocking changes to
authentication, persistent-state integrity or original outcome custody. Five owning storage targets
passed 63/63 in 69.222 seconds (`29e50d02-f23a-4165-abfa-8d3b799ff111`), including corruption,
original indeterminate custody, replay, Building/terminal transitions and candidate publication.
Normal Syndic/App/binary production checking passed in 1m51s.

All three original native routes completed the former overflow path on unchanged production
workers: dirty capture finishes before the logged target preparation, durable evidence and input
delivery checkpoints. Space completed its full initial rerun in 9.634 seconds
(`cfcf04fb-e60c-404f-aab3-152ec1c5aa54`); Enter completed a later rerun in 6.432 seconds
(`d1a0e8c0-651e-4dca-9960-8339c2043355`). The latter run includes the pending Settings phase's
independently reviewed notice-focus correction.

Pointer and Space also expose a later intermittent recovery wait after graph retirement, with
original GUI capture scheduled and Advancing. Their deadline panics are distinct from the original
`0xc00000fd` overflow. Independent acceptance review confirmed the observed checkpoints qualify
the narrow frame-separation boundary without another duplicate capture-only test. It does not
accept those later recovery continuations, claim a fully passing native suite or establish
universal measured stack headroom. The App working tree still contains unaccepted Settings work.

Fresh debugger logs and test configuration remain in `.aipm/tasks/settings-mounting/` through
acceptance. The enduring stack ownership lesson is also recorded in
[ordinary close and recovery](../failures/ordinary-close-recovery.md#native-candidate-authentication-status-frame).
