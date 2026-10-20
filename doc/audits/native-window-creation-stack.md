# Native Window-Creation Retirement Stack

## Status

Accepted on 2026-10-20 for the reproduced unpublished-retirement stack boundary, including owning
runtime qualification and production checking. Complete Settings acceptance is separate.

## Failure And Correction

Both created-after-startup native window cases reproducibly overflowed a Windows worker under
serial nextest. Three debugger-at-startup runs passed and supplied no causal frame. Attaching to
the actual nextest child after startup captured first-chance stack overflow during original
unpublished-composer retirement and candidate-session authentication. The observed worker stack
is approximately 1 MiB; Rust test-thread stack settings do not change it.

Focused disassembly resolves nearest-public-symbol labels in the stack: the actual creation
advance-owned and advance-state frames reserve 432 and 72 bytes. The private other-state dispatch
reserves 220,560 bytes, and creation abandonment preparation reserves 160,032 bytes. Both remain
live above nested shell retirement and storage decoding. This is separate from the accepted
[staging-status correction](native-staging-status-stack.md). The capture establishes the retirement
branch, but not the earlier reason that unpublished creation entered it.

Unpublished creation now bypasses general whole-enum dispatch. A private non-inlined extraction
moves the exact original shell custody from the existing state and returns before retirement I/O.
Abandonment preparation invokes the original shell operation before a separate non-inlined helper
installs its unchanged result. Original validation, cancellation, errors, state transitions,
Pending/Continue results and the 16-step work budget remain unchanged. No heap allocation, worker
stack enlargement, alternate retirement path, storage format or public interface change is added.

## Qualification And Limits

Independent source review accepts the exact two-file semantic/lifetime correction. All 99 owning
cases in main_window_creation, initial_composer and window_abandonment pass. The combined native
and owning run selected 101 cases and finished in 205.061 seconds
(`17c99da2-9b15-4b91-bb53-ca61a0193b3e`). Normal App/binary production checking passes in 14.85 seconds
with no test-thread-stack environment override.

Both original native reproductions now finish retirement and report the earlier generic New Window
publication refusal instead of overflowing in candidate authentication. Their later fixture panic
is recorded as a non-unwinding panic abort (0xc0000409), distinct from the former stack overflow
(0xc00000fd). These were not passing end-to-end native cases at this prerequisite's acceptance.
The later exact appearance-custody publication correction and passing original native routes are
recorded in [Settings acceptance](settings-workflow-mounting.md); no generic gate removal follows. No
universal native stack-headroom claim is made.

The creation owner publishes its terminal error only after the original work settles; the reported
error therefore confirms completed retirement. Independent final review accepts this narrow boundary
with those evidence limits.

Fresh first-chance trace, exact runner inputs and focused disassembly were retained through their
final Settings acceptance and diagnosis consumer. That broader qualification does not replace this
correction's focused acceptance.
